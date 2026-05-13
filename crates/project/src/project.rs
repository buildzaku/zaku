pub mod worktree_store;

#[cfg(any(test, feature = "test-support"))]
use gpui::TestAppContext;

use gpui::{App, AppContext, Context, Entity, EventEmitter, Task};
use std::{
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
};

use fs::Fs;
use util::{path::PathStyle, rel_path::RelPath};
use worktree::UpdatedEntriesSet;

pub use worktree::{
    Entry, EntryKind, ProjectEntryId, REQUEST_FILE_VERSION, RequestFile, RequestFileConfig,
    RequestFileMeta, RequestFileParam, RequestFileState, Snapshot, Worktree, WorktreeId,
};

use crate::worktree_store::{WorktreeIdCounter, WorktreeStore, WorktreeStoreEvent};

pub trait ProjectItem: 'static {
    fn try_open(
        project: &Entity<Project>,
        path: &ProjectPath,
        cx: &mut App,
    ) -> Option<Task<anyhow::Result<Entity<Self>>>>
    where
        Self: Sized;
    fn entry_id(&self, cx: &App) -> Option<ProjectEntryId>;
    fn project_path(&self, cx: &App) -> Option<ProjectPath>;
    fn is_dirty(&self) -> bool;
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct ProjectPath {
    pub worktree_id: WorktreeId,
    pub path: Arc<RelPath>,
}

impl ProjectPath {
    pub fn root_path(worktree_id: WorktreeId) -> Self {
        Self {
            worktree_id,
            path: RelPath::empty().into(),
        }
    }

    pub fn starts_with(&self, other: &ProjectPath) -> bool {
        self.worktree_id == other.worktree_id && self.path.starts_with(&other.path)
    }
}

pub struct Project {
    worktree_store: Entity<WorktreeStore>,
}

pub enum ProjectEvent {
    WorktreeAdded,
    WorktreeRemoved,
    WorktreeUpdatedEntries(UpdatedEntriesSet),
}

impl EventEmitter<ProjectEvent> for Project {}

impl Project {
    pub fn new(fs: Arc<dyn Fs>, cx: &mut Context<Self>) -> Self {
        let worktree_store =
            cx.new(move |cx| WorktreeStore::new(fs.clone(), WorktreeIdCounter::get(cx)));
        cx.subscribe(&worktree_store, |_, _, event, cx| {
            Self::on_worktree_store_event(event, cx);
        })
        .detach();

        Self { worktree_store }
    }

    pub fn open_local(
        fs: Arc<dyn Fs>,
        abs_path: PathBuf,
        cx: &mut App,
    ) -> Task<anyhow::Result<Entity<Self>>> {
        let project = cx.new(move |cx| Self::new(fs.clone(), cx));
        let open_task = project.update(cx, |project, cx| {
            project.find_or_create_worktree(abs_path, true, cx)
        });

        cx.spawn(async move |_| {
            open_task.await?;
            Ok(project)
        })
    }

    #[cfg(any(test, feature = "test-support"))]
    pub async fn test_new(
        fs: Arc<dyn Fs>,
        root_path: &Path,
        cx: &mut TestAppContext,
    ) -> Entity<Project> {
        let project = cx.update(|cx| {
            cx.new({
                let fs = fs.clone();
                move |cx| Self::new(fs.clone(), cx)
            })
        });

        let worktree = project
            .update(cx, |project, cx| {
                project.find_or_create_worktree(root_path, true, cx)
            })
            .await
            .unwrap();

        worktree
            .read_with(cx, |worktree, _| {
                worktree.as_local().unwrap().scan_complete()
            })
            .await;

        project
    }

    fn on_worktree_store_event(event: &WorktreeStoreEvent, cx: &mut Context<Self>) {
        match event {
            WorktreeStoreEvent::WorktreeAdded => cx.emit(ProjectEvent::WorktreeAdded),
            WorktreeStoreEvent::WorktreeRemoved => cx.emit(ProjectEvent::WorktreeRemoved),
            WorktreeStoreEvent::WorktreeUpdatedEntries(changes) => {
                cx.emit(ProjectEvent::WorktreeUpdatedEntries(changes.clone()));
            }
        }
    }

    pub fn worktree(&self, cx: &App) -> Option<Entity<Worktree>> {
        self.worktree_store.read(cx).worktree()
    }

    #[inline]
    pub fn worktree_for_id(&self, id: WorktreeId, cx: &App) -> Option<Entity<Worktree>> {
        self.worktree_store.read(cx).worktree_for_id(id, cx)
    }

    pub fn worktree_for_entry(
        &self,
        entry_id: ProjectEntryId,
        cx: &App,
    ) -> Option<Entity<Worktree>> {
        self.worktree_store
            .read(cx)
            .worktree_for_entry(entry_id, cx)
    }

    #[inline]
    pub fn worktree_id_for_entry(&self, entry_id: ProjectEntryId, cx: &App) -> Option<WorktreeId> {
        self.worktree_for_entry(entry_id, cx)
            .map(|worktree| worktree.read(cx).id())
    }

    pub fn snapshot(&self, cx: &App) -> Option<Snapshot> {
        self.worktree_store.read(cx).snapshot(cx)
    }

    pub fn root(&self, cx: &App) -> Option<PathBuf> {
        self.worktree_store.read(cx).root(cx)
    }

    pub fn path_style(&self, cx: &App) -> PathStyle {
        self.worktree_store.read(cx).path_style()
    }

    pub fn worktree_store(&self) -> Entity<WorktreeStore> {
        self.worktree_store.clone()
    }

    pub fn wait_for_initial_scan(&self, cx: &App) -> impl Future<Output = ()> + use<> {
        self.worktree_store.read(cx).wait_for_initial_scan()
    }

    pub fn find_or_create_worktree(
        &mut self,
        abs_path: impl AsRef<Path>,
        visible: bool,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Entity<Worktree>>> {
        self.worktree_store.update(cx, |worktree_store, cx| {
            worktree_store.find_or_create_worktree(abs_path, visible, cx)
        })
    }

    pub fn remove_worktree(&mut self, cx: &mut Context<Self>) {
        self.worktree_store.update(cx, |worktree_store, cx| {
            worktree_store.remove_worktree(cx);
        });
    }

    pub fn entry_for_path<'a>(&'a self, path: &ProjectPath, cx: &'a App) -> Option<&'a Entry> {
        self.worktree_store.read(cx).entry_for_path(path, cx)
    }

    pub fn path_for_entry(&self, entry_id: ProjectEntryId, cx: &App) -> Option<ProjectPath> {
        let worktree = self.worktree_for_entry(entry_id, cx)?;
        let worktree = worktree.read(cx);
        let worktree_id = worktree.id();
        let path = worktree.entry_for_id(entry_id)?.path.clone();
        Some(ProjectPath { worktree_id, path })
    }

    pub fn absolute_path(&self, project_path: &ProjectPath, cx: &App) -> Option<PathBuf> {
        Some(
            self.worktree_for_id(project_path.worktree_id, cx)?
                .read(cx)
                .absolutize(&project_path.path),
        )
    }

    pub fn project_path_for_absolute_path(&self, abs_path: &Path, cx: &App) -> Option<ProjectPath> {
        self.worktree_store
            .read(cx)
            .project_path_for_absolute_path(abs_path, cx)
    }

    pub fn absolutize(&self, path: &RelPath, cx: &App) -> Option<PathBuf> {
        let worktree = self.worktree_store.read(cx).worktree()?;
        Some(worktree.read(cx).absolutize(path))
    }
}
