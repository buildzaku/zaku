pub mod buffer_store;
pub mod worktree_store;

pub use request_buffer::{RequestBuffer, RequestBufferEvent};
pub use worktree::{
    Entry, EntryKind, ProjectEntryId, REQUEST_FILE_VERSION, RequestFile, RequestFileBody,
    RequestFileBodyType, RequestFileHeader, RequestFileHttp, RequestFileMeta, RequestFileParam,
    RequestFileState, Snapshot, Worktree, WorktreeId,
};

#[cfg(any(test, feature = "test-support"))]
use gpui::TestAppContext;

use gpui::{App, AppContext, Context, Entity, EventEmitter, Task, TaskExt};
use std::{
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
};

use fs::Fs;
use util::{path::PathStyle, rel_path::RelPath};
use worktree::UpdatedEntriesSet;

use crate::{
    buffer_store::{BufferStore, BufferStoreEvent},
    worktree_store::{WorktreeIdCounter, WorktreeStore, WorktreeStoreEvent},
};

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

impl<P: Into<Arc<RelPath>>> From<(WorktreeId, P)> for ProjectPath {
    fn from((worktree_id, path): (WorktreeId, P)) -> Self {
        Self {
            worktree_id,
            path: path.into(),
        }
    }
}

impl ProjectItem for RequestBuffer {
    fn try_open(
        project: &Entity<Project>,
        path: &ProjectPath,
        cx: &mut App,
    ) -> Option<Task<anyhow::Result<Entity<Self>>>> {
        if !project.read(cx).entry_for_path(path, cx)?.is_request {
            return None;
        }

        Some(project.update(cx, |project, cx| {
            project.open_request_buffer(path.clone(), cx)
        }))
    }

    fn entry_id(&self, _cx: &App) -> Option<ProjectEntryId> {
        self.file().project_entry_id()
    }

    fn project_path(&self, cx: &App) -> Option<ProjectPath> {
        let file = self.file();

        Some(ProjectPath {
            worktree_id: file.worktree_id(cx),
            path: file.path().clone(),
        })
    }

    fn is_dirty(&self) -> bool {
        RequestBuffer::is_dirty(self)
    }
}

pub struct Project {
    worktree_store: Entity<WorktreeStore>,
    buffer_store: Entity<BufferStore>,
}

pub enum ProjectEvent {
    WorktreeAdded,
    WorktreeRemoved,
    WorktreeUpdatedEntries(UpdatedEntriesSet),
    DeletedEntry(ProjectEntryId),
}

impl EventEmitter<ProjectEvent> for Project {}

impl Project {
    pub fn new(fs: Arc<dyn Fs>, cx: &mut Context<Self>) -> Self {
        let worktree_store =
            cx.new(move |cx| WorktreeStore::new(fs.clone(), WorktreeIdCounter::get(cx)));
        let buffer_store = cx.new({
            let worktree_store = worktree_store.clone();
            move |cx| BufferStore::new(worktree_store.clone(), cx)
        });
        cx.subscribe(&worktree_store, |_, _, event, cx| {
            Self::on_worktree_store_event(event, cx);
        })
        .detach();
        cx.subscribe(&buffer_store, |_, _, event, cx| {
            Self::on_buffer_store_event(event, cx);
        })
        .detach();

        Self {
            worktree_store,
            buffer_store,
        }
    }

    pub fn open_local(
        fs: Arc<dyn Fs>,
        abs_path: PathBuf,
        cx: &mut App,
    ) -> Task<anyhow::Result<Entity<Self>>> {
        let project = cx.new(move |cx| Self::new(fs.clone(), cx));
        let open_task = project.update(cx, |project, cx| {
            project.find_or_create_worktree(abs_path, cx)
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
                project.find_or_create_worktree(root_path, cx)
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
            WorktreeStoreEvent::WorktreeAdded(_) => cx.emit(ProjectEvent::WorktreeAdded),
            WorktreeStoreEvent::WorktreeRemoved => cx.emit(ProjectEvent::WorktreeRemoved),
            WorktreeStoreEvent::WorktreeUpdatedEntries(changes) => {
                cx.emit(ProjectEvent::WorktreeUpdatedEntries(changes.clone()));
            }
            WorktreeStoreEvent::WorktreeDeletedEntry(entry_id) => {
                cx.emit(ProjectEvent::DeletedEntry(*entry_id));
            }
        }
    }

    fn on_buffer_store_event(event: &BufferStoreEvent, cx: &mut Context<Self>) {
        match event {
            BufferStoreEvent::BufferAdded(buffer) => {
                Self::register_request_buffer(buffer, cx);
            }
            BufferStoreEvent::BufferDropped(_) | BufferStoreEvent::BufferChangedFilePath { .. } => {
            }
        }
    }

    fn register_request_buffer(buffer: &Entity<RequestBuffer>, cx: &mut Context<Self>) {
        cx.subscribe(buffer, |this, buffer, event, cx| {
            this.on_request_buffer_event(&buffer, *event, cx);
        })
        .detach();
    }

    fn on_request_buffer_event(
        &mut self,
        buffer: &Entity<RequestBuffer>,
        event: RequestBufferEvent,
        cx: &mut Context<Self>,
    ) {
        if event == RequestBufferEvent::ReloadNeeded {
            self.reload_request_buffer(buffer, cx)
                .detach_and_log_err(cx);
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
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Entity<Worktree>>> {
        self.worktree_store
            .update(cx, |store, cx| store.find_or_create_worktree(abs_path, cx))
    }

    pub fn remove_worktree(&mut self, cx: &mut Context<Self>) {
        self.worktree_store.update(cx, |store, cx| {
            store.remove_worktree(cx);
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

    pub fn reveal_path(&self, path: &Path, cx: &mut Context<Self>) {
        cx.reveal_path(path);
    }

    pub fn project_path_for_absolute_path(&self, abs_path: &Path, cx: &App) -> Option<ProjectPath> {
        self.worktree_store
            .read(cx)
            .project_path_for_absolute_path(abs_path, cx)
    }

    pub fn create_entry(
        &mut self,
        project_path: impl Into<ProjectPath>,
        is_directory: bool,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Entry>> {
        let project_path = project_path.into();
        let Some(worktree) = self.worktree_for_id(project_path.worktree_id, cx) else {
            return Task::ready(Err(anyhow::anyhow!(format!(
                "No worktree for path {project_path:?}"
            ))));
        };

        let content = if is_directory {
            None
        } else {
            let contents = match worktree::serialize_request_file(&RequestFile::default()) {
                Ok(contents) => contents,
                Err(error) => return Task::ready(Err(error)),
            };
            Some(contents.into_bytes())
        };

        worktree.update(cx, |worktree, cx| {
            worktree.create_entry(project_path.path, is_directory, content, cx)
        })
    }

    #[inline]
    pub fn copy_entry(
        &mut self,
        entry_id: ProjectEntryId,
        new_project_path: ProjectPath,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Option<Entry>>> {
        self.worktree_store.update(cx, |worktree_store, cx| {
            worktree_store.copy_entry(entry_id, new_project_path, cx)
        })
    }

    #[inline]
    pub fn rename_entry(
        &mut self,
        entry_id: ProjectEntryId,
        new_path: ProjectPath,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Entry>> {
        self.worktree_store.update(cx, |worktree_store, cx| {
            worktree_store.rename_entry(entry_id, new_path, cx)
        })
    }

    #[inline]
    pub fn delete_file(
        &mut self,
        path: &ProjectPath,
        trash: bool,
        cx: &mut Context<Self>,
    ) -> Option<Task<anyhow::Result<()>>> {
        let entry = self.entry_for_path(path, cx)?;
        self.delete_entry(entry.id, trash, cx)
    }

    #[inline]
    pub fn delete_entry(
        &mut self,
        entry_id: ProjectEntryId,
        trash: bool,
        cx: &mut Context<Self>,
    ) -> Option<Task<anyhow::Result<()>>> {
        let worktree = self.worktree_for_entry(entry_id, cx)?;
        worktree.update(cx, |worktree, cx| {
            worktree.delete_entry(entry_id, trash, cx)
        })
    }

    #[inline]
    pub fn expand_entry(
        &mut self,
        entry_id: ProjectEntryId,
        cx: &mut Context<Self>,
    ) -> Option<Task<anyhow::Result<()>>> {
        let worktree = self.worktree_for_entry(entry_id, cx)?;
        worktree.update(cx, |worktree, cx| worktree.expand_entry(entry_id, cx))
    }

    #[inline]
    pub fn entry_is_worktree_root(&self, entry_id: ProjectEntryId, cx: &App) -> bool {
        self.worktree_for_entry(entry_id, cx)
            .and_then(|worktree| {
                worktree
                    .read(cx)
                    .root_entry()
                    .map(|entry| entry.id == entry_id)
            })
            .unwrap_or(false)
    }

    pub fn absolutize(&self, path: &RelPath, cx: &App) -> Option<PathBuf> {
        let worktree = self.worktree_store.read(cx).worktree()?;
        Some(worktree.read(cx).absolutize(path))
    }

    fn open_request_buffer(
        &mut self,
        path: ProjectPath,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Entity<RequestBuffer>>> {
        self.buffer_store
            .update(cx, |store, cx| store.open_request_buffer(path, cx))
    }

    pub fn save_request_buffer(
        &self,
        buffer: &Entity<RequestBuffer>,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        self.buffer_store
            .update(cx, |store, cx| store.save_request_buffer(buffer, cx))
    }

    pub fn reload_request_buffer(
        &self,
        buffer: &Entity<RequestBuffer>,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        self.buffer_store
            .update(cx, |store, cx| store.reload_request_buffer(buffer, cx))
    }
}
