pub mod buffer_store;
pub mod request_buffer_store;
pub mod worktree_store;

pub use request_buffer::{RequestBuffer, RequestBufferEvent};
pub use worktree::{
    Entry, EntryKind, File, ProjectEntryId, REQUEST_FILE_VERSION, RequestFile, RequestFileBody,
    RequestFileBodyType, RequestFileHeader, RequestFileHttp, RequestFileMeta, RequestFileParam,
    RequestFileState, Snapshot, UpdatedEntriesSet, Worktree, WorktreeId, request_method_label,
};

use anyhow::anyhow;
use futures::{FutureExt, StreamExt};
#[cfg(any(test, feature = "test"))]
use gpui::TestAppContext;
use gpui::{App, AppContext, Context, Entity, EventEmitter, Task, TaskExt};
use std::{
    collections::HashMap,
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
};

use fs::{Fs, MTime};
use language::{AvailableLanguage, Buffer, BufferEvent, Language, LanguageRegistry, PLAIN_TEXT};
use path::{PathStyle, RelPath};
use util::ResultExt;

use crate::{
    buffer_store::{BufferStore, BufferStoreEvent},
    request_buffer_store::{RequestBufferStore, RequestBufferStoreEvent},
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryMetadata {
    pub prefix_label: Option<String>,
    pub is_invalid: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EntryMetadataVersion {
    path: Arc<RelPath>,
    inode: u64,
    mtime: Option<MTime>,
    size: u64,
}

impl EntryMetadataVersion {
    fn for_entry(entry: &Entry) -> Self {
        Self {
            path: entry.path.clone(),
            inode: entry.inode,
            mtime: entry.mtime,
            size: entry.size,
        }
    }
}

enum EntryMetadataState {
    Pending {
        version: EntryMetadataVersion,
        metadata: Option<EntryMetadata>,
        _task: Task<()>,
    },
    Loaded {
        version: EntryMetadataVersion,
        metadata: EntryMetadata,
    },
}

impl EntryMetadataState {
    fn version(&self) -> &EntryMetadataVersion {
        match self {
            Self::Pending { version, .. } | Self::Loaded { version, .. } => version,
        }
    }

    fn metadata(&self) -> Option<&EntryMetadata> {
        match self {
            Self::Loaded { metadata, .. } => Some(metadata),
            Self::Pending { metadata, .. } => metadata.as_ref(),
        }
    }

    fn is_current(&self, entry: &Entry) -> bool {
        self.version() == &EntryMetadataVersion::for_entry(entry)
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
            path: file.path.clone(),
        })
    }

    fn is_dirty(&self) -> bool {
        RequestBuffer::is_dirty(self)
    }
}

impl ProjectItem for Buffer {
    fn try_open(
        project: &Entity<Project>,
        path: &ProjectPath,
        cx: &mut App,
    ) -> Option<Task<anyhow::Result<Entity<Self>>>> {
        if project
            .read(cx)
            .entry_for_path(path, cx)
            .is_some_and(|entry| entry.is_request)
        {
            return None;
        }

        Some(project.update(cx, |project, cx| project.open_buffer(path.clone(), cx)))
    }

    fn entry_id(&self, _: &App) -> Option<ProjectEntryId> {
        File::from_dyn(self.file()).and_then(File::project_entry_id)
    }

    fn project_path(&self, cx: &App) -> Option<ProjectPath> {
        let file = self.file()?;

        Some(ProjectPath {
            worktree_id: file.worktree_id(cx),
            path: file.path().clone(),
        })
    }

    fn is_dirty(&self) -> bool {
        self.is_dirty()
    }
}

pub struct Project {
    worktree_store: Entity<WorktreeStore>,
    buffer_store: Entity<BufferStore>,
    request_buffer_store: Entity<RequestBufferStore>,
    languages: Arc<LanguageRegistry>,
    active_entry: Option<ProjectEntryId>,
    metadata_by_entry_id: HashMap<ProjectEntryId, EntryMetadataState>,
    _maintain_buffer_languages: Task<()>,
}

pub enum ProjectEvent {
    ActiveEntryChanged(Option<ProjectEntryId>),
    WorktreeAdded(WorktreeId),
    WorktreeRemoved(WorktreeId),
    WorktreeUpdatedEntries(WorktreeId, UpdatedEntriesSet),
    DeletedEntry(WorktreeId, ProjectEntryId),
    EntryMetadataUpdated(ProjectEntryId),
}

impl Project {
    pub fn new(fs: Arc<dyn Fs>, languages: Arc<LanguageRegistry>, cx: &mut Context<Self>) -> Self {
        let worktree_store =
            cx.new(move |cx| WorktreeStore::new(fs.clone(), WorktreeIdCounter::get(cx)));
        let buffer_store = cx.new({
            let worktree_store = worktree_store.clone();
            move |cx| BufferStore::new(&worktree_store, cx)
        });
        let request_buffer_store = cx.new({
            let worktree_store = worktree_store.clone();
            move |cx| RequestBufferStore::new(worktree_store.clone(), cx)
        });
        cx.subscribe(&worktree_store, |this, _, event, cx| {
            this.on_worktree_store_event(event, cx);
        })
        .detach();
        cx.subscribe(&buffer_store, |this, _, event, cx| {
            this.on_buffer_store_event(event, cx);
        })
        .detach();
        cx.subscribe(&request_buffer_store, |_, _, event, cx| {
            Self::on_request_buffer_store_event(event, cx);
        })
        .detach();
        let maintain_buffer_languages = Self::maintain_buffer_languages(languages.clone(), cx);

        Self {
            worktree_store,
            buffer_store,
            request_buffer_store,
            languages,
            active_entry: None,
            metadata_by_entry_id: HashMap::new(),
            _maintain_buffer_languages: maintain_buffer_languages,
        }
    }

    pub fn open(
        fs: Arc<dyn Fs>,
        languages: Arc<LanguageRegistry>,
        abs_path: PathBuf,
        cx: &mut App,
    ) -> Task<anyhow::Result<Entity<Self>>> {
        let project = cx.new(move |cx| Self::new(fs.clone(), languages.clone(), cx));
        let open_task = project.update(cx, |project, cx| {
            project.find_or_create_worktree(abs_path, true, cx)
        });

        cx.spawn(async move |_| {
            open_task.await?;
            Ok(project)
        })
    }

    #[cfg(any(test, feature = "test"))]
    pub async fn test_new(
        fs: Arc<dyn Fs>,
        root_path: &Path,
        cx: &mut TestAppContext,
    ) -> Entity<Project> {
        let languages = Arc::new(LanguageRegistry::test_new(cx.executor()));
        let project = cx.update(|cx| {
            cx.new({
                let fs = fs.clone();
                let languages = languages.clone();
                move |cx| Self::new(fs.clone(), languages.clone(), cx)
            })
        });

        let (worktree, _) = project
            .update(cx, |project, cx| {
                project.find_or_create_worktree(root_path, true, cx)
            })
            .await
            .expect("test project should create root worktree");

        worktree
            .read_with(cx, |worktree, _| worktree.scan_complete())
            .await;

        project
    }

    fn on_worktree_store_event(&mut self, event: &WorktreeStoreEvent, cx: &mut Context<Self>) {
        match event {
            WorktreeStoreEvent::WorktreeAdded(worktree) => {
                cx.emit(ProjectEvent::WorktreeAdded(worktree.read(cx).id()));
            }
            WorktreeStoreEvent::WorktreeRemoved(worktree_id) => {
                cx.emit(ProjectEvent::WorktreeRemoved(*worktree_id));
            }
            WorktreeStoreEvent::WorktreeUpdatedEntries(worktree_id, changes) => {
                cx.emit(ProjectEvent::WorktreeUpdatedEntries(
                    *worktree_id,
                    changes.clone(),
                ));
            }
            WorktreeStoreEvent::WorktreeDeletedEntry(worktree_id, entry_id) => {
                self.metadata_by_entry_id.remove(entry_id);
                cx.emit(ProjectEvent::DeletedEntry(*worktree_id, *entry_id));
            }
        }
    }

    fn on_buffer_store_event(&mut self, event: &BufferStoreEvent, cx: &mut Context<Self>) {
        match event {
            BufferStoreEvent::BufferAdded(buffer) => {
                self.on_buffer_added(buffer, cx);
            }
            BufferStoreEvent::BufferChangedFilePath { buffer, .. } => {
                self.detect_language_for_buffer(buffer, cx);
            }
            BufferStoreEvent::BufferDropped(_) => {}
        }
    }

    fn on_buffer_added(&mut self, buffer: &Entity<Buffer>, cx: &mut Context<Self>) {
        Self::register_buffer(buffer, cx);
        self.detect_language_for_buffer(buffer, cx);
    }

    fn register_buffer(buffer: &Entity<Buffer>, cx: &mut Context<Self>) {
        cx.subscribe(buffer, |this, buffer, event, cx| {
            this.on_buffer_event(&buffer, event, cx);
        })
        .detach();
    }

    fn on_buffer_event(
        &mut self,
        buffer: &Entity<Buffer>,
        event: &BufferEvent,
        cx: &mut Context<Self>,
    ) {
        if event == &BufferEvent::ReloadNeeded {
            self.reload_buffer(buffer, cx).detach_and_log_err(cx);
        }
    }

    fn detect_language_for_buffer(
        &mut self,
        buffer_handle: &Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Option<AvailableLanguage> {
        let available_language = {
            let buffer = buffer_handle.read(cx);
            let file = buffer.file()?;
            let content = buffer.as_rope();
            self.languages.language_for_file(file, Some(content), cx)
        };

        if let Some(available_language) = &available_language
            && let Some(Ok(Ok(new_language))) = self
                .languages
                .load_language(available_language)
                .now_or_never()
        {
            Self::set_language_for_buffer(buffer_handle, new_language, cx);
        }

        available_language
    }

    fn set_language_for_buffer(
        buffer: &Entity<Buffer>,
        new_language: Arc<Language>,
        cx: &mut Context<Self>,
    ) {
        if buffer
            .read(cx)
            .language()
            .is_none_or(|old_language| !Arc::ptr_eq(old_language, &new_language))
        {
            buffer.update(cx, move |buffer, cx| {
                buffer.set_language_async(Some(new_language), cx);
            });
        }
    }

    fn maintain_buffer_languages(
        languages: Arc<LanguageRegistry>,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let mut subscription = languages.subscribe();
        let mut previous_reload_count = languages.reload_count();
        cx.spawn(async move |this, cx| {
            while let Some(()) = subscription.next().await {
                let Some(this) = this.upgrade() else {
                    break;
                };

                let reload_count = languages.reload_count();
                if reload_count > previous_reload_count {
                    previous_reload_count = reload_count;
                    this.update(cx, |this, cx| {
                        let buffers = this.buffer_store.read(cx).buffers().collect::<Vec<_>>();
                        for buffer in buffers {
                            if buffer.read(cx).file().is_some() {
                                buffer.update(cx, |buffer, cx| {
                                    buffer.set_language_async(None, cx);
                                });
                            }
                        }
                    });
                }

                this.update(cx, |this, cx| {
                    let mut plain_text_buffers = Vec::new();
                    for handle in this.buffer_store.read(cx).buffers() {
                        let buffer = handle.read(cx);
                        if buffer.language().is_none() || buffer.language() == Some(&*PLAIN_TEXT) {
                            plain_text_buffers.push(handle);
                        }
                    }

                    for buffer in plain_text_buffers {
                        this.detect_language_for_buffer(&buffer, cx);
                    }
                });
            }
        })
    }

    fn on_request_buffer_store_event(event: &RequestBufferStoreEvent, cx: &mut Context<Self>) {
        match event {
            RequestBufferStoreEvent::BufferAdded(buffer) => {
                Self::register_request_buffer(buffer, cx);
            }
            RequestBufferStoreEvent::BufferDropped(_)
            | RequestBufferStoreEvent::BufferChangedFilePath { .. } => {}
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

    #[inline]
    pub fn worktrees<'a>(
        &self,
        cx: &'a App,
    ) -> impl 'a + DoubleEndedIterator<Item = Entity<Worktree>> {
        self.worktree_store.read(cx).worktrees()
    }

    #[inline]
    pub fn visible_worktrees<'a>(
        &'a self,
        cx: &'a App,
    ) -> impl 'a + DoubleEndedIterator<Item = Entity<Worktree>> {
        self.worktree_store.read(cx).visible_worktrees(cx)
    }

    #[inline]
    pub fn root_worktree(&self, cx: &App) -> Option<Entity<Worktree>> {
        self.worktree_store.read(cx).root_worktree(cx)
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
    ) -> Task<anyhow::Result<(Entity<Worktree>, Arc<RelPath>)>> {
        self.worktree_store.update(cx, |store, cx| {
            store.find_or_create_worktree(abs_path, visible, cx)
        })
    }

    pub fn remove_worktree(&mut self, cx: &mut Context<Self>) {
        self.worktree_store.update(cx, |store, cx| {
            store.remove_worktree(cx);
        });
    }

    pub fn entry_for_path<'a>(&'a self, path: &ProjectPath, cx: &'a App) -> Option<&'a Entry> {
        self.worktree_store.read(cx).entry_for_path(path, cx)
    }

    pub fn set_active_path(&mut self, entry: Option<ProjectPath>, cx: &mut Context<Self>) {
        let new_active_entry = entry.and_then(|project_path| {
            let worktree = self.worktree_for_id(project_path.worktree_id, cx)?;
            let entry = worktree.read(cx).entry_for_path(&project_path.path)?;
            Some(entry.id)
        });

        if new_active_entry != self.active_entry {
            self.active_entry = new_active_entry;
            cx.emit(ProjectEvent::ActiveEntryChanged(new_active_entry));
        }
    }

    pub fn active_entry(&self) -> Option<ProjectEntryId> {
        self.active_entry
    }

    pub fn entry_metadata(&self, entry: &Entry) -> Option<&EntryMetadata> {
        self.metadata_by_entry_id
            .get(&entry.id)
            .filter(|metadata| metadata.is_current(entry))
            .and_then(EntryMetadataState::metadata)
    }

    pub fn load_entry_metadata(&mut self, entry: &Entry, cx: &mut Context<Self>) {
        if !entry.kind.is_file() || !entry.is_request {
            return;
        }

        let version = EntryMetadataVersion::for_entry(entry);
        if self
            .metadata_by_entry_id
            .get(&entry.id)
            .is_some_and(|metadata| metadata.version() == &version)
        {
            return;
        }

        let Some(project_path) = self.path_for_entry(entry.id, cx) else {
            return;
        };
        let Some(worktree) = self.worktree_for_id(project_path.worktree_id, cx) else {
            return;
        };

        let path = project_path.path.clone();
        let load_file_task =
            worktree.update(cx, |worktree, cx| worktree.load_file(path.as_ref(), cx));
        let entry_id = entry.id;
        let version_for_task = version.clone();
        let metadata_task = cx.spawn(async move |this, cx| {
            let request_file = match load_file_task.await.log_err() {
                Some(loaded) => {
                    let parse_task =
                        cx.background_spawn(
                            async move { worktree::parse_request_file(&loaded.text) },
                        );
                    Some(parse_task.await)
                }
                None => None,
            };

            this.update(cx, |this, cx| {
                let is_current = matches!(
                    this.metadata_by_entry_id.get(&entry_id),
                    Some(EntryMetadataState::Pending { version, .. })
                        if version == &version_for_task
                );
                if !is_current {
                    return;
                }

                match request_file {
                    Some(RequestFileState::Parsed(request_file)) => {
                        this.metadata_by_entry_id.insert(
                            entry_id,
                            EntryMetadataState::Loaded {
                                version: version_for_task.clone(),
                                metadata: EntryMetadata {
                                    prefix_label: Some(worktree::request_method_label(
                                        &request_file.http.method,
                                    )),
                                    is_invalid: false,
                                },
                            },
                        );
                    }
                    Some(RequestFileState::Invalid(_)) => {
                        this.metadata_by_entry_id.insert(
                            entry_id,
                            EntryMetadataState::Loaded {
                                version: version_for_task.clone(),
                                metadata: EntryMetadata {
                                    prefix_label: None,
                                    is_invalid: true,
                                },
                            },
                        );
                    }
                    None => {
                        this.metadata_by_entry_id.remove(&entry_id);
                    }
                }
                cx.emit(ProjectEvent::EntryMetadataUpdated(entry_id));
                cx.notify();
            })
            .log_err();
        });

        let metadata = self
            .metadata_by_entry_id
            .get(&entry.id)
            .and_then(EntryMetadataState::metadata)
            .cloned();

        self.metadata_by_entry_id.insert(
            entry.id,
            EntryMetadataState::Pending {
                version,
                metadata,
                _task: metadata_task,
            },
        );
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
            return Task::ready(Err(anyhow!(format!(
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
        let root_worktree = self.root_worktree(cx)?;
        Some(root_worktree.read(cx).absolutize(path))
    }

    pub fn open_buffer_at(
        &mut self,
        abs_path: impl AsRef<Path>,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Entity<Buffer>>> {
        let worktree_task = self.find_or_create_worktree(abs_path.as_ref(), false, cx);
        cx.spawn(async move |this, cx| {
            let (worktree, relative_path) = worktree_task.await?;
            this.update(cx, |this, cx| {
                this.open_buffer((worktree.read(cx).id(), relative_path), cx)
            })?
            .await
        })
    }

    pub fn open_buffer(
        &mut self,
        path: impl Into<ProjectPath>,
        cx: &mut App,
    ) -> Task<anyhow::Result<Entity<Buffer>>> {
        self.buffer_store
            .update(cx, |store, cx| store.open_buffer(path.into(), cx))
    }

    pub fn save_buffer(
        &self,
        buffer: Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        self.buffer_store
            .update(cx, |store, cx| store.save_buffer(buffer, cx))
    }

    pub fn reload_buffer(
        &self,
        buffer: &Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        self.buffer_store
            .update(cx, |store, cx| store.reload_buffer(buffer, cx))
    }

    pub fn get_open_buffer(&self, path: &ProjectPath, cx: &App) -> Option<Entity<Buffer>> {
        self.buffer_store.read(cx).get_by_path(path)
    }

    fn open_request_buffer(
        &mut self,
        path: ProjectPath,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Entity<RequestBuffer>>> {
        self.request_buffer_store
            .update(cx, |store, cx| store.open_request_buffer(path, cx))
    }

    pub fn save_request_buffer(
        &self,
        buffer: &Entity<RequestBuffer>,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        self.request_buffer_store
            .update(cx, |store, cx| store.save_request_buffer(buffer, cx))
    }

    pub fn reload_request_buffer(
        &self,
        buffer: &Entity<RequestBuffer>,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        self.request_buffer_store
            .update(cx, |store, cx| store.reload_request_buffer(buffer, cx))
    }
}

impl EventEmitter<ProjectEvent> for Project {}
