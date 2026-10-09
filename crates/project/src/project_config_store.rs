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
    ProjectPath,
    buffer_store::is_not_found_error,
    worktree_store::{WorktreeStore, WorktreeStoreEvent},
};

pub enum ProjectConfigStoreEvent {
    ConfigFileUpdated(Result<Arc<RelPath>, InvalidConfigFileError>),
    ActiveEnvironmentChanged,
    ConfigFilesLoaded,
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
    worktree_store: Entity<WorktreeStore>,
    fs: Arc<dyn Fs>,
    root_worktree_id: Option<WorktreeId>,
    project_file: Option<ProjectFile>,
    environment_files: BTreeMap<String, EnvironmentFile>,
    folder_files: BTreeMap<Arc<RelPath>, FolderFile>,
    active_environment: Option<String>,
    loading_files: HashMap<Arc<RelPath>, Task<()>>,
    initial_load_completed: bool,
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
            worktree_store: worktree_store.clone(),
            fs,
            root_worktree_id: None,
            project_file: None,
            environment_files: BTreeMap::default(),
            folder_files: BTreeMap::default(),
            active_environment: None,
            loading_files: HashMap::default(),
            initial_load_completed: true,
            _worktree_store_subscription: worktree_store_subscription,
        }
    }

    pub fn initial_load_completed(&self) -> bool {
        self.initial_load_completed
    }

    pub fn project_file(&self) -> Option<&ProjectFile> {
        self.project_file.as_ref()
    }

    pub fn environments(&self) -> impl Iterator<Item = (&str, &EnvironmentFile)> {
        self.environment_files
            .iter()
            .map(|(name, environment_file)| (name.as_str(), environment_file))
    }

    pub fn environment_file(&self, name: &str) -> Option<&EnvironmentFile> {
        self.environment_files.get(name)
    }

    pub fn folder_file(&self, folder: &RelPath) -> Option<&FolderFile> {
        self.folder_files.get(folder)
    }

    pub fn active_environment(&self) -> Option<&str> {
        self.active_environment.as_deref()
    }

    pub fn activate_environment(&mut self, name: Option<String>, cx: &mut Context<Self>) {
        if self.active_environment != name {
            self.active_environment = name;
            cx.emit(ProjectConfigStoreEvent::ActiveEnvironmentChanged);
        }
    }

    pub fn is_active_environment_missing(&self) -> bool {
        self.initial_load_completed()
            && self
                .active_environment
                .as_ref()
                .is_some_and(|name| !self.environment_files.contains_key(name))
    }

    pub fn variables_for_request(&self, request_path: &ProjectPath) -> HashMap<String, String> {
        if self.root_worktree_id != Some(request_path.worktree_id) {
            return HashMap::default();
        }

        let folder_files = request_path
            .path
            .ancestors()
            .skip(1)
            .filter_map(|folder| self.folder_files.get(folder))
            .collect::<Vec<_>>();

        // Later scopes override earlier ones.
        self.project_file
            .iter()
            .flat_map(|project_file| &project_file.request.variables)
            .chain(
                self.active_environment
                    .as_ref()
                    .and_then(|name| self.environment_file(name))
                    .into_iter()
                    .flat_map(|environment_file| &environment_file.environment.variables),
            )
            .chain(
                folder_files
                    .into_iter()
                    .rev()
                    .flat_map(|folder_file| &folder_file.request.variables),
            )
            .filter(|variable| !variable.disabled)
            .map(|variable| (variable.name.clone(), variable.value.clone()))
            .collect()
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
            WorktreeStoreEvent::WorktreeAdded(worktree) => {
                if worktree.read(cx).is_visible() {
                    self.initial_load_completed = false;
                    let initial_scan = worktree_store.read(cx).wait_for_initial_scan();
                    cx.spawn(async move |this, cx| {
                        initial_scan.await;
                        if let Err(error) = this.update(cx, |this, cx| {
                            this.update_initial_load_state(cx);
                        }) {
                            log::trace!(
                                "Failed to update project config store after initial scan: {error:?}"
                            );
                        }
                    })
                    .detach();
                }
            }
            WorktreeStoreEvent::WorktreeRemoved(worktree_id) => {
                if self.root_worktree_id == Some(*worktree_id) {
                    self.root_worktree_id = None;
                    self.project_file = None;
                    self.environment_files.clear();
                    self.folder_files.clear();
                    self.loading_files.clear();
                    self.update_initial_load_state(cx);
                }
            }
            WorktreeStoreEvent::WorktreeUpdatedGitRepositories(_, _)
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
        self.update_initial_load_state(cx);
    }

    fn update_initial_load_state(&mut self, cx: &mut Context<Self>) {
        if !self.initial_load_completed()
            && self.loading_files.is_empty()
            && self.worktree_store.read(cx).initial_scan_completed()
        {
            self.initial_load_completed = true;
            cx.emit(ProjectConfigStoreEvent::ConfigFilesLoaded);
        }
    }
}

impl EventEmitter<ProjectConfigStoreEvent> for ProjectConfigStore {}
