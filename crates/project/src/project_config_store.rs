use gpui::{Context, Entity, EventEmitter, Subscription, Task};
use std::sync::Arc;

use collections::{BTreeMap, HashMap};
use fs::Fs;
use path::{PathStyle, RelPath};
use util::ResultExt;
use worktree::{
    EnvironmentFile, FolderFile, PathChange, ProjectFile, UpdatedEntriesSet, Worktree, WorktreeId,
};

use crate::{
    buffer_store::is_not_found_error,
    worktree_store::{WorktreeStore, WorktreeStoreEvent},
};

pub enum ProjectConfigStoreEvent {
    ConfigFileUpdated(Result<Arc<RelPath>, InvalidConfigFileError>),
}

#[derive(Debug, Clone)]
pub struct InvalidConfigFileError {
    pub path: Arc<RelPath>,
    pub message: String,
}

enum ConfigFileKind {
    Project,
    Environment(String),
    Folder(Arc<RelPath>),
}

impl ConfigFileKind {
    fn for_path(path: &RelPath) -> Option<Self> {
        if path == path::project_config_file_relative_path() {
            return Some(Self::Project);
        }

        if path.parent() == Some(path::project_environments_folder_relative_path())
            && path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"))
        {
            return path
                .file_stem()
                .map(|name| Self::Environment(name.to_string()));
        }

        let folder_file_relative_path = path::project_folder_file_relative_path();
        let folder = path
            .ancestors()
            .nth(folder_file_relative_path.component_count())?;
        let is_folder_file = !folder.is_empty()
            && !folder
                .components()
                .any(|component| component == path::project_config_folder_name())
            && &*folder.join(folder_file_relative_path) == path;
        is_folder_file.then(|| Self::Folder(folder.into()))
    }
}

pub struct ProjectConfigStore {
    fs: Arc<dyn Fs>,
    root_worktree_id: Option<WorktreeId>,
    project_file: Option<ProjectFile>,
    environment_files: BTreeMap<String, EnvironmentFile>,
    folder_files: BTreeMap<Arc<RelPath>, FolderFile>,
    loading_files: HashMap<Arc<RelPath>, Task<()>>,
    _worktree_store_subscription: Subscription,
}

impl ProjectConfigStore {
    pub fn new(
        worktree_store: &Entity<WorktreeStore>,
        fs: Arc<dyn Fs>,
        cx: &mut Context<Self>,
    ) -> Self {
        let worktree_store_subscription =
            cx.subscribe(worktree_store, |this, worktree_store, event, cx| {
                this.on_worktree_store_event(&worktree_store, event, cx);
            });

        Self {
            fs,
            root_worktree_id: None,
            project_file: None,
            environment_files: BTreeMap::default(),
            folder_files: BTreeMap::default(),
            loading_files: HashMap::default(),
            _worktree_store_subscription: worktree_store_subscription,
        }
    }

    pub fn project_file(&self) -> Option<&ProjectFile> {
        self.project_file.as_ref()
    }

    pub fn environments(&self) -> impl Iterator<Item = (&str, &EnvironmentFile)> {
        self.environment_files
            .iter()
            .map(|(name, environment_file)| (name.as_str(), environment_file))
    }

    pub fn folder_file(&self, folder: &RelPath) -> Option<&FolderFile> {
        self.folder_files.get(folder)
    }

    fn on_worktree_store_event(
        &mut self,
        worktree_store: &Entity<WorktreeStore>,
        event: &WorktreeStoreEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            WorktreeStoreEvent::WorktreeUpdatedEntries(worktree_id, changes) => {
                if let Some(root_worktree) = worktree_store.read(cx).root_worktree(cx)
                    && root_worktree.read(cx).id() == *worktree_id
                {
                    self.root_worktree_id = Some(*worktree_id);
                    self.update_config_files(&root_worktree, changes, cx);
                }
            }
            WorktreeStoreEvent::WorktreeRemoved(worktree_id) => {
                if self.root_worktree_id == Some(*worktree_id) {
                    self.root_worktree_id = None;
                    self.project_file = None;
                    self.environment_files.clear();
                    self.folder_files.clear();
                    self.loading_files.clear();
                }
            }
            WorktreeStoreEvent::WorktreeAdded(_)
            | WorktreeStoreEvent::WorktreeUpdatedGitRepositories(_, _)
            | WorktreeStoreEvent::WorktreeDeletedEntry(_, _) => {}
        }
    }

    fn update_config_files(
        &mut self,
        worktree: &Entity<Worktree>,
        changes: &UpdatedEntriesSet,
        cx: &mut Context<Self>,
    ) {
        for (path, _, change) in changes.iter() {
            let Some(kind) = ConfigFileKind::for_path(path) else {
                continue;
            };

            if change == &PathChange::Removed {
                self.loading_files.remove(path);
                self.update_config_file(path.clone(), kind, None, cx);
                continue;
            }

            let fs = self.fs.clone();
            let abs_path = worktree.read(cx).absolutize(path);
            let loading_task = cx.spawn({
                let path = path.clone();
                async move |this, cx| {
                    let contents = match fs.load(&abs_path).await {
                        Ok(contents) => Some(contents),
                        Err(error) if is_not_found_error(&error) => None,
                        Err(error) => Err(error).log_err(),
                    };

                    this.update(cx, |this, cx| {
                        this.loading_files.remove(&path);
                        this.update_config_file(path, kind, contents.as_deref(), cx);
                    })
                    .log_err();
                }
            });
            self.loading_files.insert(path.clone(), loading_task);
        }
    }

    fn update_config_file(
        &mut self,
        path: Arc<RelPath>,
        kind: ConfigFileKind,
        contents: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let config_file_kind = match &kind {
            ConfigFileKind::Project => "project",
            ConfigFileKind::Environment(_) => "environment",
            ConfigFileKind::Folder(_) => "folder",
        };
        let update_result = match (kind, contents) {
            (ConfigFileKind::Project, Some(contents)) => {
                worktree::parse_config_file(contents).map(|project_file| {
                    self.project_file = Some(project_file);
                })
            }
            (ConfigFileKind::Project, None) => {
                self.project_file = None;
                Ok(())
            }
            (ConfigFileKind::Environment(name), Some(contents)) => {
                worktree::parse_config_file(contents).map(|environment_file| {
                    self.environment_files.insert(name, environment_file);
                })
            }
            (ConfigFileKind::Environment(name), None) => {
                self.environment_files.remove(&name);
                Ok(())
            }
            (ConfigFileKind::Folder(folder), Some(contents)) => {
                worktree::parse_config_file(contents).map(|folder_file| {
                    self.folder_files.insert(folder, folder_file);
                })
            }
            (ConfigFileKind::Folder(folder), None) => {
                self.folder_files.remove(&folder);
                Ok(())
            }
        };
        let result = match update_result {
            Ok(()) => Ok(path),
            Err(error) => {
                let message = format!(
                    "Failed to parse {config_file_kind} config file {}:\n{error}",
                    path.display(PathStyle::local())
                );
                log::error!("{message}");
                Err(InvalidConfigFileError { path, message })
            }
        };
        cx.emit(ProjectConfigStoreEvent::ConfigFileUpdated(result));
    }
}

impl EventEmitter<ProjectConfigStoreEvent> for ProjectConfigStore {}
