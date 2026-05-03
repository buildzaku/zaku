mod request;

use anyhow::{Context as AnyhowContext, anyhow};
use async_lock::Mutex;
use futures::{FutureExt, Stream, StreamExt, select_biased};
use gpui::{
    AppContext, AsyncApp, BackgroundExecutor, Context, Entity, EventEmitter, Priority, Task,
};
use smallvec::{SmallVec, smallvec};
use smol::channel;
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    ffi::OsStr,
    fmt,
    future::Future,
    ops::{Deref, DerefMut, Range},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering::SeqCst},
    },
    task::Poll,
    time::Duration,
};
use sum_tree::{Bias, ContextLessSummary, Dimension, Edit, Item, KeyedItem, SeekTarget, SumTree};
use tokio::sync::{oneshot, watch};

use request::parse_request_file;
pub use request::{
    RequestFile, RequestFileBody, RequestFileBodyKind, RequestFileConfig, RequestFileHeader,
    RequestFileMeta, RequestFileState,
};

use fs::{
    FileHandle, Fs, MTime, Metadata as FsMetadata, PathEvent, PathEventKind, Watcher as FsWatcher,
};
use util::{
    ResultExt,
    path::{PathStyle, SanitizedPath},
    rel_path::RelPath,
};

pub const FS_WATCH_LATENCY: Duration = Duration::from_millis(100);

pub struct LocalWorktree {
    snapshot: LocalSnapshot,
    scan_requests_tx: channel::Sender<ScanRequest>,
    path_prefixes_to_scan_tx: channel::Sender<PathPrefixScanRequest>,
    is_scanning: (watch::Sender<bool>, watch::Receiver<bool>),
    _background_scanner_tasks: Vec<Task<()>>,
    fs: Arc<dyn Fs>,
    fs_case_sensitive: bool,
    visible: bool,
    next_entry_id: Arc<AtomicUsize>,
    scanning_enabled: bool,
}

impl LocalWorktree {
    pub fn fs(&self) -> &Arc<dyn Fs> {
        &self.fs
    }

    pub fn fs_is_case_sensitive(&self) -> bool {
        self.fs_case_sensitive
    }

    pub fn snapshot(&self) -> LocalSnapshot {
        self.snapshot.clone()
    }

    pub fn refresh_entries_for_paths(
        &self,
        relative_paths: Vec<Arc<RelPath>>,
    ) -> oneshot::Receiver<()> {
        let (completion_sender, completion_receiver) = oneshot::channel();
        self.scan_requests_tx
            .try_send(ScanRequest {
                relative_paths,
                completion_senders: smallvec![completion_sender],
            })
            .ok();
        completion_receiver
    }

    pub fn add_path_prefix_to_scan(&self, path_prefix: Arc<RelPath>) -> oneshot::Receiver<()> {
        let (completion_sender, completion_receiver) = oneshot::channel();
        self.path_prefixes_to_scan_tx
            .try_send(PathPrefixScanRequest {
                path: path_prefix,
                completion_senders: smallvec![completion_sender],
            })
            .ok();
        completion_receiver
    }

    pub fn scan_complete(&self) -> impl Future<Output = ()> + use<> {
        let mut is_scanning_rx = self.is_scanning.1.clone();
        async move {
            let mut is_scanning = *is_scanning_rx.borrow_and_update();
            while is_scanning {
                if is_scanning_rx.changed().await.is_ok() {
                    is_scanning = *is_scanning_rx.borrow_and_update();
                } else {
                    break;
                }
            }
        }
    }

    fn restart_background_scanners(&mut self, cx: &Context<Worktree>) {
        let (scan_requests_tx, scan_requests_rx) = channel::unbounded();
        let (path_prefixes_to_scan_tx, path_prefixes_to_scan_rx) = channel::unbounded();
        self.scan_requests_tx = scan_requests_tx;
        self.path_prefixes_to_scan_tx = path_prefixes_to_scan_tx;
        self.start_background_scanner(scan_requests_rx, path_prefixes_to_scan_rx, cx);
    }

    fn update_abs_path_and_refresh(
        &mut self,
        new_path: Arc<SanitizedPath>,
        cx: &Context<Worktree>,
    ) {
        let root_name = new_path
            .as_path()
            .file_name()
            .and_then(|file_name| file_name.to_str())
            .map_or(RelPath::empty().into(), |file_name| {
                RelPath::unix(file_name).unwrap().into()
            });

        self.snapshot.update_abs_path(new_path, root_name);
        self.restart_background_scanners(cx);
    }

    fn start_background_scanner(
        &mut self,
        scan_requests_rx: channel::Receiver<ScanRequest>,
        path_prefixes_to_scan_rx: channel::Receiver<PathPrefixScanRequest>,
        cx: &Context<Worktree>,
    ) {
        let snapshot = self.snapshot.clone();
        let watch_abs_path = snapshot.abs_path().to_path_buf();
        let fs = self.fs.clone();
        let next_entry_id = self.next_entry_id.clone();
        let scanning_enabled = self.scanning_enabled;
        let executor = cx.background_executor().clone();
        let (scan_states_tx, mut scan_states_rx) = futures::channel::mpsc::unbounded();

        let background_scanner = cx.background_spawn(async move {
            let (events, watcher) = if scanning_enabled {
                fs.watch(watch_abs_path.as_path(), FS_WATCH_LATENCY).await
            } else {
                (
                    Box::pin(futures::stream::pending())
                        as Pin<Box<dyn Send + Stream<Item = Vec<PathEvent>>>>,
                    Arc::new(NullWatcher) as Arc<dyn FsWatcher>,
                )
            };
            let fs_case_sensitive = fs.is_case_sensitive().await;

            let mut background_scanner = BackgroundScanner {
                state: Mutex::new(BackgroundScannerState {
                    prev_snapshot: snapshot.snapshot.clone(),
                    snapshot,
                    scanned_dirs: Default::default(),
                    path_prefixes_to_scan: Default::default(),
                    paths_to_scan: Default::default(),
                    removed_entries: Default::default(),
                    changed_paths: Default::default(),
                    scanning_enabled,
                }),
                fs,
                fs_case_sensitive,
                watcher,
                next_entry_id,
                status_updates_tx: scan_states_tx,
                executor,
                scan_requests_rx,
                path_prefixes_to_scan_rx,
                phase: BackgroundScannerPhase::InitialScan,
            };
            background_scanner.run(events).await;
        });

        let scan_state_updater = cx.spawn(async move |this, cx| {
            while let Some((state, this)) = scan_states_rx.next().await.zip(this.upgrade()) {
                this.update(cx, |this, cx| {
                    let Some(worktree) = this.as_local_mut() else {
                        return;
                    };
                    match state {
                        ScanState::Started => {
                            worktree.is_scanning.0.send_replace(true);
                        }
                        ScanState::Updated {
                            snapshot,
                            changes,
                            completion_senders,
                            scanning,
                        } => {
                            worktree.is_scanning.0.send_replace(scanning);
                            worktree.set_snapshot(snapshot, changes, cx);
                            for completion_sender in completion_senders {
                                if completion_sender.send(()).is_err() {
                                    log::trace!("Worktree scan completion receiver dropped");
                                }
                            }
                        }
                        ScanState::RootUpdated { new_path } => {
                            worktree.update_abs_path_and_refresh(new_path, cx);
                        }
                    }
                });
            }
        });

        self._background_scanner_tasks = vec![background_scanner, scan_state_updater];
        self.is_scanning.0.send_replace(true);
    }

    fn set_snapshot(
        &mut self,
        snapshot: LocalSnapshot,
        changes: UpdatedEntriesSet,
        cx: &mut Context<Worktree>,
    ) {
        self.snapshot = snapshot;
        if !changes.is_empty() {
            cx.emit(WorktreeEvent::UpdatedEntries(changes));
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorktreeId(usize);

impl WorktreeId {
    pub fn from_usize(id: usize) -> Self {
        Self(id)
    }

    pub fn to_usize(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProjectEntryId(usize);

impl ProjectEntryId {
    pub fn new(counter: &AtomicUsize) -> Self {
        Self(counter.fetch_add(1, SeqCst))
    }

    pub fn from_usize(id: usize) -> Self {
        Self(id)
    }

    pub fn to_usize(self) -> usize {
        self.0
    }
}

pub enum Worktree {
    Local(LocalWorktree),
}

impl Worktree {
    pub async fn local(
        path: impl Into<Arc<Path>>,
        visible: bool,
        fs: Arc<dyn Fs>,
        next_entry_id: Arc<AtomicUsize>,
        scanning_enabled: bool,
        worktree_id: WorktreeId,
        cx: &mut AsyncApp,
    ) -> anyhow::Result<Entity<Self>> {
        let opened_abs_path = path.into();

        let metadata = fs
            .metadata(opened_abs_path.as_ref())
            .await
            .context("failed to stat worktree path")?;

        if metadata.as_ref().is_some_and(|metadata| !metadata.is_dir) {
            return Err(anyhow!(
                "worktree root must be a directory: {}",
                opened_abs_path.display()
            ));
        }

        let path_style = PathStyle::local();
        let fs_case_sensitive = fs.is_case_sensitive().await;
        let root_name = opened_abs_path
            .file_name()
            .and_then(|file_name| file_name.to_str())
            .map_or(RelPath::empty().into(), |file_name| {
                RelPath::unix(file_name).unwrap().into()
            });
        let root_file_handle = if metadata.as_ref().is_some() {
            fs.open_handle(opened_abs_path.as_ref())
                .await
                .with_context(|| {
                    format!(
                        "failed to open local worktree root at {}",
                        opened_abs_path.display()
                    )
                })
                .log_err()
        } else {
            None
        };

        Ok(cx.new(move |cx: &mut Context<Self>| {
            let mut snapshot = LocalSnapshot {
                snapshot: Snapshot::new(
                    worktree_id,
                    root_name,
                    opened_abs_path.clone(),
                    path_style,
                ),
                root_file_handle,
            };
            if let Some(metadata) = metadata {
                let mut root_entry = Entry::new(
                    Arc::from(RelPath::empty()),
                    &metadata,
                    ProjectEntryId::new(next_entry_id.as_ref()),
                    None,
                );
                root_entry.kind = if scanning_enabled {
                    EntryKind::PendingDir
                } else {
                    EntryKind::UnloadedDir
                };
                snapshot.insert_entry(root_entry);
            }

            let (scan_requests_tx, scan_requests_rx) = channel::unbounded();
            let (path_prefixes_to_scan_tx, path_prefixes_to_scan_rx) = channel::unbounded();
            let mut worktree = LocalWorktree {
                snapshot,
                scan_requests_tx,
                path_prefixes_to_scan_tx,
                is_scanning: watch::channel(true),
                _background_scanner_tasks: Vec::new(),
                fs,
                fs_case_sensitive,
                visible,
                next_entry_id,
                scanning_enabled,
            };
            worktree.start_background_scanner(scan_requests_rx, path_prefixes_to_scan_rx, cx);
            Self::Local(worktree)
        }))
    }

    pub fn as_local(&self) -> Option<&LocalWorktree> {
        match self {
            Self::Local(worktree) => Some(worktree),
        }
    }

    pub fn as_local_mut(&mut self) -> Option<&mut LocalWorktree> {
        match self {
            Self::Local(worktree) => Some(worktree),
        }
    }

    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local(_))
    }

    pub fn is_visible(&self) -> bool {
        match self {
            Self::Local(worktree) => worktree.visible,
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        match self {
            Self::Local(worktree) => worktree.snapshot.snapshot.clone(),
        }
    }
}

impl Deref for Worktree {
    type Target = Snapshot;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Local(worktree) => &worktree.snapshot,
        }
    }
}

#[derive(Clone)]
pub struct Snapshot {
    id: WorktreeId,
    abs_path: Arc<SanitizedPath>,
    path_style: PathStyle,
    root_name: Arc<RelPath>,
    entries_by_path: SumTree<Entry>,
    entries_by_id: SumTree<PathEntry>,
    scan_id: usize,
    completed_scan_id: usize,
}

impl Snapshot {
    pub fn new(
        id: WorktreeId,
        root_name: Arc<RelPath>,
        abs_path: Arc<Path>,
        path_style: PathStyle,
    ) -> Self {
        Self {
            id,
            abs_path: SanitizedPath::from_arc(abs_path),
            path_style,
            root_name,
            entries_by_path: SumTree::new(()),
            entries_by_id: SumTree::new(()),
            scan_id: 1,
            completed_scan_id: 0,
        }
    }

    pub fn id(&self) -> WorktreeId {
        self.id
    }

    pub fn abs_path(&self) -> &Arc<Path> {
        SanitizedPath::cast_arc_ref(&self.abs_path)
    }

    pub fn path_style(&self) -> PathStyle {
        self.path_style
    }

    pub fn root_name(&self) -> &RelPath {
        self.root_name.as_ref()
    }

    pub fn root_name_str(&self) -> &str {
        self.root_name.as_unix_str()
    }

    pub fn scan_id(&self) -> usize {
        self.scan_id
    }

    pub fn completed_scan_id(&self) -> usize {
        self.completed_scan_id
    }

    fn update_abs_path(&mut self, abs_path: Arc<SanitizedPath>, root_name: Arc<RelPath>) {
        self.abs_path = abs_path;
        self.root_name = root_name;
    }

    pub fn contains_entry(&self, id: ProjectEntryId) -> bool {
        self.entries_by_id.get(&id, ()).is_some()
    }

    pub fn entry_count(&self) -> usize {
        self.entries_by_path.summary().count
    }

    pub fn dir_count(&self) -> usize {
        let summary = self.entries_by_path.summary();
        summary.count - summary.file_count
    }

    pub fn file_count(&self) -> usize {
        self.entries_by_path.summary().file_count
    }

    pub fn root_entry(&self) -> Option<&Entry> {
        self.entries_by_path.first()
    }

    pub fn root_dir(&self) -> Option<Arc<Path>> {
        self.root_entry()
            .filter(|entry| entry.is_dir())
            .map(|_| self.abs_path().clone())
    }

    fn traverse_from_offset(
        &self,
        include_files: bool,
        include_dirs: bool,
        start_offset: usize,
    ) -> Traversal<'_> {
        let mut cursor = self.entries_by_path.cursor(());
        cursor.seek(
            &TraversalTarget::Count {
                count: start_offset,
                include_files,
                include_dirs,
            },
            Bias::Right,
        );
        Traversal {
            snapshot: self,
            cursor,
            include_files,
            include_dirs,
        }
    }

    pub fn traverse_from_path(
        &self,
        include_files: bool,
        include_dirs: bool,
        path: &RelPath,
    ) -> Traversal<'_> {
        Traversal::new(self, include_files, include_dirs, path)
    }

    pub fn files(&self, start: usize) -> Traversal<'_> {
        self.traverse_from_offset(true, false, start)
    }

    pub fn directories(&self, start: usize) -> Traversal<'_> {
        self.traverse_from_offset(false, true, start)
    }

    pub fn entries(&self, start: usize) -> Traversal<'_> {
        self.traverse_from_offset(true, true, start)
    }

    pub fn paths(&self) -> impl Iterator<Item = &RelPath> {
        self.entries_by_path
            .cursor::<()>(())
            .filter(|entry| !entry.path.is_empty())
            .map(|entry| entry.path.as_ref())
    }

    pub fn entry_for_path(&self, path: &RelPath) -> Option<&Entry> {
        self.traverse_from_path(true, true, path)
            .entry()
            .and_then(|entry| {
                if entry.path.as_ref() == path {
                    Some(entry)
                } else {
                    None
                }
            })
    }

    pub fn entry_for_id(&self, id: ProjectEntryId) -> Option<&Entry> {
        let path_entry = self.entries_by_id.get(&id, ())?;
        self.entry_for_path(&path_entry.path)
    }

    pub fn child_entries<'a>(&'a self, parent_path: &'a RelPath) -> ChildEntriesIter<'a> {
        let options = ChildEntriesOptions {
            include_files: true,
            include_dirs: true,
        };
        self.child_entries_with_options(parent_path, options)
    }

    pub fn child_entries_with_options<'a>(
        &'a self,
        parent_path: &'a RelPath,
        options: ChildEntriesOptions,
    ) -> ChildEntriesIter<'a> {
        let mut cursor = self.entries_by_path.cursor(());
        cursor.seek(&TraversalTarget::path(parent_path), Bias::Right);
        let traversal = Traversal {
            snapshot: self,
            cursor,
            include_files: options.include_files,
            include_dirs: options.include_dirs,
        };
        ChildEntriesIter {
            parent_path,
            traversal,
        }
    }

    pub fn absolutize(&self, path: &RelPath) -> PathBuf {
        if path.file_name().is_some() {
            let mut abs_path = self.abs_path.to_string();
            for component in path.components() {
                if !abs_path.ends_with(self.path_style.primary_separator()) {
                    abs_path.push_str(self.path_style.primary_separator());
                }
                abs_path.push_str(component);
            }
            PathBuf::from(abs_path)
        } else {
            self.abs_path.as_path().to_path_buf()
        }
    }

    fn ancestor_inodes_for_path(&self, path: &RelPath) -> HashSet<u64> {
        let mut inodes = HashSet::default();
        for ancestor in path.ancestors().skip(1) {
            if let Some(entry) = self.entry_for_path(ancestor) {
                inodes.insert(entry.inode);
            }
        }
        inodes
    }
}

impl fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        struct EntriesById<'a>(&'a SumTree<PathEntry>);
        struct EntriesByPath<'a>(&'a SumTree<Entry>);

        impl fmt::Debug for EntriesByPath<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_map()
                    .entries(self.0.iter().map(|entry| (&entry.path, entry.id)))
                    .finish()
            }
        }

        impl fmt::Debug for EntriesById<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_list().entries(self.0.iter()).finish()
            }
        }

        f.debug_struct("Snapshot")
            .field("id", &self.id)
            .field("root_name", &self.root_name)
            .field("entries_by_path", &EntriesByPath(&self.entries_by_path))
            .field("entries_by_id", &EntriesById(&self.entries_by_id))
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: ProjectEntryId,
    pub kind: EntryKind,
    pub path: Arc<RelPath>,
    pub inode: u64,
    pub mtime: Option<MTime>,
    pub canonical_path: Option<Arc<Path>>,
    pub is_external: bool,
    pub is_fifo: bool,
    pub size: u64,
    pub request: Option<RequestFileState>,
}

impl Entry {
    fn new(
        path: Arc<RelPath>,
        metadata: &FsMetadata,
        id: ProjectEntryId,
        canonical_path: Option<Arc<Path>>,
    ) -> Self {
        Self {
            id,
            kind: if metadata.is_dir {
                EntryKind::PendingDir
            } else {
                EntryKind::File
            },
            path,
            inode: metadata.inode,
            mtime: Some(metadata.mtime),
            canonical_path,
            is_external: false,
            is_fifo: metadata.is_fifo,
            size: metadata.len,
            request: None,
        }
    }

    pub fn is_created(&self) -> bool {
        self.mtime.is_some()
    }

    pub fn is_dir(&self) -> bool {
        self.kind.is_dir()
    }

    pub fn is_file(&self) -> bool {
        self.kind.is_file()
    }

    fn to_path_entry(&self) -> PathEntry {
        PathEntry {
            id: self.id,
            path: self.path.clone(),
        }
    }
}

impl Item for Entry {
    type Summary = EntrySummary;

    fn summary(&self, (): ()) -> Self::Summary {
        EntrySummary {
            count: 1,
            file_count: usize::from(self.is_file()),
            max_path: self.path.clone(),
        }
    }
}

impl KeyedItem for Entry {
    type Key = PathKey;

    fn key(&self) -> Self::Key {
        PathKey(self.path.clone())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    UnloadedDir,
    PendingDir,
    Dir,
    File,
}

impl EntryKind {
    pub fn is_dir(&self) -> bool {
        matches!(self, Self::UnloadedDir | Self::PendingDir | Self::Dir)
    }

    pub fn is_unloaded(&self) -> bool {
        matches!(self, Self::UnloadedDir)
    }

    pub fn is_file(&self) -> bool {
        matches!(self, Self::File)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathChange {
    Added,
    Removed,
    Updated,
    AddedOrUpdated,
    Loaded,
}

pub type UpdatedEntriesSet = Arc<[(Arc<RelPath>, ProjectEntryId, PathChange)]>;

#[derive(Clone, Debug)]
pub enum WorktreeEvent {
    UpdatedEntries(UpdatedEntriesSet),
}

impl EventEmitter<WorktreeEvent> for Worktree {}

impl Deref for LocalWorktree {
    type Target = LocalSnapshot;

    fn deref(&self) -> &Self::Target {
        &self.snapshot
    }
}

#[derive(Clone)]
pub struct LocalSnapshot {
    snapshot: Snapshot,
    root_file_handle: Option<Arc<dyn FileHandle>>,
}

impl LocalSnapshot {
    fn insert_entry(&mut self, entry: Entry) -> Entry {
        let old_entry = self
            .snapshot
            .entries_by_path
            .get(&PathKey(entry.path.clone()), ())
            .cloned();
        if let Some(old_entry) = old_entry
            && old_entry.id != entry.id
        {
            self.snapshot
                .entries_by_id
                .edit(vec![Edit::Remove(old_entry.id)], ());
        }

        self.snapshot
            .entries_by_id
            .edit(vec![Edit::Insert(entry.to_path_entry())], ());
        self.snapshot
            .entries_by_path
            .edit(vec![Edit::Insert(entry.clone())], ());

        entry
    }

    #[cfg(feature = "test-support")]
    pub fn check_invariants(&self) {
        assert_eq!(
            self.snapshot
                .entries_by_path
                .cursor::<()>(())
                .map(|entry| (&entry.path, entry.id))
                .collect::<Vec<_>>(),
            self.snapshot
                .entries_by_id
                .cursor::<()>(())
                .map(|entry| (&entry.path, entry.id))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>(),
            "entries_by_path and entries_by_id are inconsistent"
        );

        let mut file_entries = self.snapshot.files(0);
        for entry in self.snapshot.entries_by_path.cursor::<()>(()) {
            if entry.is_file() {
                assert_eq!(file_entries.next().unwrap().inode, entry.inode);
            }
        }

        assert!(file_entries.next().is_none());

        let mut paths_from_child_entries = Vec::new();
        let mut pending_paths = self
            .snapshot
            .root_entry()
            .map(|entry| entry.path.as_ref())
            .into_iter()
            .collect::<Vec<_>>();
        while let Some(path) = pending_paths.pop() {
            paths_from_child_entries.push(path);
            let index = pending_paths.len();
            for child_entry in self.snapshot.child_entries(path) {
                pending_paths.insert(index, &child_entry.path);
            }
        }

        let indexed_paths = self
            .snapshot
            .entries_by_path
            .cursor::<()>(())
            .map(|entry| entry.path.as_ref())
            .collect::<Vec<_>>();
        assert_eq!(paths_from_child_entries, indexed_paths);

        let paths_from_entries = self
            .snapshot
            .entries(0)
            .map(|entry| entry.path.as_ref())
            .collect::<Vec<_>>();
        assert_eq!(paths_from_entries, indexed_paths);
    }
}

impl Deref for LocalSnapshot {
    type Target = Snapshot;

    fn deref(&self) -> &Self::Target {
        &self.snapshot
    }
}

impl DerefMut for LocalSnapshot {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.snapshot
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntrySummary {
    count: usize,
    file_count: usize,
    max_path: Arc<RelPath>,
}

impl Default for EntrySummary {
    fn default() -> Self {
        Self {
            max_path: Arc::from(RelPath::empty()),
            count: 0,
            file_count: 0,
        }
    }
}

impl ContextLessSummary for EntrySummary {
    fn zero() -> Self {
        Self::default()
    }

    fn add_summary(&mut self, summary: &Self) {
        self.max_path = summary.max_path.clone();
        self.count += summary.count;
        self.file_count += summary.file_count;
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PathKey(pub Arc<RelPath>);

impl Default for PathKey {
    fn default() -> Self {
        Self(RelPath::empty().into())
    }
}

impl<'a> Dimension<'a, EntrySummary> for PathKey {
    fn zero((): ()) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &'a EntrySummary, (): ()) {
        self.0 = summary.max_path.clone();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PathEntry {
    id: ProjectEntryId,
    path: Arc<RelPath>,
}

impl Item for PathEntry {
    type Summary = PathEntrySummary;

    fn summary(&self, (): ()) -> Self::Summary {
        PathEntrySummary { max_id: self.id }
    }
}

impl KeyedItem for PathEntry {
    type Key = ProjectEntryId;

    fn key(&self) -> Self::Key {
        self.id
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct PathEntrySummary {
    max_id: ProjectEntryId,
}

impl ContextLessSummary for PathEntrySummary {
    fn zero() -> Self {
        Self::default()
    }

    fn add_summary(&mut self, summary: &Self) {
        self.max_id = summary.max_id;
    }
}

impl<'a> Dimension<'a, PathEntrySummary> for ProjectEntryId {
    fn zero((): ()) -> Self {
        Self::default()
    }

    fn add_summary(&mut self, summary: &'a PathEntrySummary, (): ()) {
        *self = summary.max_id;
    }
}

struct ScanRequest {
    relative_paths: Vec<Arc<RelPath>>,
    completion_senders: SmallVec<[oneshot::Sender<()>; 1]>,
}

struct PathPrefixScanRequest {
    path: Arc<RelPath>,
    completion_senders: SmallVec<[oneshot::Sender<()>; 1]>,
}

enum ScanState {
    Started,
    Updated {
        snapshot: LocalSnapshot,
        changes: UpdatedEntriesSet,
        completion_senders: SmallVec<[oneshot::Sender<()>; 1]>,
        scanning: bool,
    },
    RootUpdated {
        new_path: Arc<SanitizedPath>,
    },
}

struct BackgroundScanner {
    state: Mutex<BackgroundScannerState>,
    fs: Arc<dyn Fs>,
    fs_case_sensitive: bool,
    watcher: Arc<dyn FsWatcher>,
    next_entry_id: Arc<AtomicUsize>,
    status_updates_tx: futures::channel::mpsc::UnboundedSender<ScanState>,
    executor: BackgroundExecutor,
    scan_requests_rx: channel::Receiver<ScanRequest>,
    path_prefixes_to_scan_rx: channel::Receiver<PathPrefixScanRequest>,
    phase: BackgroundScannerPhase,
}

impl BackgroundScanner {
    async fn run(&mut self, mut events: Pin<Box<dyn Send + Stream<Item = Vec<PathEvent>>>>) {
        let (scan_job_tx, scan_job_rx) = channel::unbounded();
        let root_scan_job = {
            let mut state = self.state.lock().await;
            state.snapshot.scan_id += 1;
            if state.scanning_enabled {
                state
                    .snapshot
                    .root_entry()
                    .filter(|root_entry| root_entry.is_dir())
                    .map(|root_entry| {
                        let mut ancestor_inodes = HashSet::default();
                        ancestor_inodes.insert(root_entry.inode);
                        ScanJob {
                            abs_path: Arc::<Path>::from(state.snapshot.abs_path.as_path()),
                            path: Arc::from(RelPath::empty()),
                            scan_queue: scan_job_tx.clone(),
                            ancestor_inodes,
                            is_external: root_entry.is_external,
                        }
                    })
            } else {
                None
            }
        };

        if let Some(root_scan_job) = root_scan_job
            && scan_job_tx.try_send(root_scan_job).is_err()
        {
            return;
        }

        drop(scan_job_tx);
        self.scan_dirs(true, scan_job_rx).await;
        {
            let mut state = self.state.lock().await;
            state.snapshot.completed_scan_id = state.snapshot.scan_id;
        }

        self.send_status_update(false, SmallVec::new(), &[]).await;

        self.phase = BackgroundScannerPhase::EventsReceivedDuringInitialScan;
        if let Poll::Ready(Some(mut event_batch)) = futures::poll!(events.next()) {
            while let Poll::Ready(Some(more_events)) = futures::poll!(events.next()) {
                event_batch.extend(more_events);
            }
            self.process_events(
                event_batch
                    .into_iter()
                    .filter(|event| event.kind.is_some())
                    .collect(),
            )
            .await;
        }

        self.phase = BackgroundScannerPhase::Events;

        loop {
            select_biased! {
                request = self.next_scan_request().fuse() => {
                    let Ok(request) = request else {
                        break;
                    };
                    if !self.process_scan_request(request, false).await {
                        break;
                    }
                }
                path_prefix_request = self.path_prefixes_to_scan_rx.recv().fuse() => {
                    let Ok(request) = path_prefix_request else {
                        break;
                    };
                    log::trace!("Adding path prefix {:?}", request.path);
                    let did_scan = self
                        .forcibly_load_paths(std::slice::from_ref(&request.path))
                        .await;
                    if did_scan {
                        let abs_path = {
                            let mut state = self.state.lock().await;
                            state.path_prefixes_to_scan.insert(request.path.clone());
                            state.snapshot.absolutize(&request.path)
                        };
                        if let Some(abs_path) = self.fs.canonicalize(&abs_path).await.log_err() {
                            self.process_events(vec![PathEvent {
                                path: abs_path,
                                kind: Some(PathEventKind::Changed),
                            }])
                            .await;
                        }
                    }
                    self.send_status_update(false, request.completion_senders, &[])
                        .await;
                }
                event_batch = events.next().fuse() => {
                    let Some(mut event_batch) = event_batch else {
                        break;
                    };
                    while let Poll::Ready(Some(more_events)) = futures::poll!(events.next()) {
                        event_batch.extend(more_events);
                    }
                    self.process_events(
                        event_batch
                            .into_iter()
                            .filter(|event| event.kind.is_some())
                            .collect(),
                    )
                    .await;
                }
            }
        }
    }

    async fn next_scan_request(&self) -> Result<ScanRequest, smol::channel::RecvError> {
        let mut request = self.scan_requests_rx.recv().await?;
        while let Ok(next_request) = self.scan_requests_rx.try_recv() {
            request.relative_paths.extend(next_request.relative_paths);
            request
                .completion_senders
                .extend(next_request.completion_senders);
        }
        Ok(request)
    }

    async fn process_scan_request(&self, mut request: ScanRequest, scanning: bool) -> bool {
        log::debug!("Rescanning paths {:?}", request.relative_paths);

        request.relative_paths.sort_unstable();
        self.forcibly_load_paths(&request.relative_paths).await;

        let root_path = self.state.lock().await.snapshot.abs_path.clone();
        let root_canonical_path = self.fs.canonicalize(root_path.as_path()).await;
        let root_canonical_path = match root_canonical_path.as_ref() {
            Ok(path) => SanitizedPath::new(path),
            Err(error) => {
                log::error!(
                    "Failed to canonicalize worktree root {}: {error:#}",
                    root_path.as_path().display()
                );
                for completion_sender in request.completion_senders {
                    if completion_sender.send(()).is_err() {
                        log::trace!("Worktree scan completion receiver dropped");
                    }
                }
                return true;
            }
        };

        let abs_paths = request
            .relative_paths
            .iter()
            .map(|path| {
                if path.file_name().is_some() {
                    root_canonical_path.as_path().join(path.as_std_path())
                } else {
                    root_canonical_path.as_path().to_path_buf()
                }
            })
            .collect::<Vec<_>>();

        {
            let mut state = self.state.lock().await;
            let is_idle = state.snapshot.completed_scan_id == state.snapshot.scan_id;
            state.snapshot.scan_id += 1;
            if is_idle {
                state.snapshot.completed_scan_id = state.snapshot.scan_id;
            }
        }

        self.reload_entries_for_paths(
            root_path.as_ref(),
            root_canonical_path,
            &request.relative_paths,
            abs_paths,
            None,
        )
        .await;

        self.send_status_update(scanning, request.completion_senders, &[])
            .await
    }

    async fn forcibly_load_paths(&self, paths: &[Arc<RelPath>]) -> bool {
        let (scan_job_tx, scan_job_rx) = channel::unbounded();
        {
            let mut state = self.state.lock().await;
            for path in paths {
                for ancestor in path.ancestors() {
                    if let Some(entry) = state.snapshot.entry_for_path(ancestor)
                        && entry.kind == EntryKind::UnloadedDir
                    {
                        let abs_path = state.snapshot.absolutize(ancestor);
                        state.enqueue_scan_dir(abs_path.into(), entry, &scan_job_tx);
                        state.paths_to_scan.insert(path.clone());
                        break;
                    }
                }
            }
            drop(scan_job_tx);
        }
        while let Ok(job) = scan_job_rx.recv().await {
            if let Err(error) = self.scan_dir(&job).await {
                log::error!("Failed to scan {}: {error:#}", job.abs_path.display());
            }
        }

        !std::mem::take(&mut self.state.lock().await.paths_to_scan).is_empty()
    }

    async fn process_events(&self, mut events: Vec<PathEvent>) {
        let root_path = self.state.lock().await.snapshot.abs_path.clone();
        let root_canonical_path = self.fs.canonicalize(root_path.as_path()).await;
        let root_canonical_path = match &root_canonical_path {
            Ok(path) => SanitizedPath::new(path),
            Err(error) => {
                let new_path = self
                    .state
                    .lock()
                    .await
                    .snapshot
                    .root_file_handle
                    .clone()
                    .and_then(|handle| match handle.current_path(&self.fs) {
                        Ok(new_path) => Some(new_path),
                        Err(current_path_error) => {
                            log::error!(
                                "Failed to refresh worktree root path: {current_path_error:#}"
                            );
                            None
                        }
                    })
                    .map(|path| SanitizedPath::new_arc(&path))
                    .filter(|new_path| *new_path != root_path);

                if let Some(new_path) = new_path {
                    log::info!(
                        "Root renamed from {:?} to {:?}",
                        root_path.as_path(),
                        new_path.as_path(),
                    );
                    self.status_updates_tx
                        .unbounded_send(ScanState::RootUpdated { new_path })
                        .ok();
                } else {
                    log::error!("Root path could not be canonicalized: {error:#}");
                }
                return;
            }
        };

        let mut relative_paths = Vec::with_capacity(events.len());
        events.sort_unstable_by(|left, right| left.path.cmp(&right.path));
        events.dedup_by(|left, right| {
            if left.path == right.path {
                if matches!(left.kind, Some(PathEventKind::Rescan)) {
                    right.kind = left.kind;
                }
                true
            } else if left.path.starts_with(&right.path) {
                if matches!(left.kind, Some(PathEventKind::Rescan)) {
                    right.kind = left.kind;
                }
                true
            } else {
                false
            }
        });
        {
            let snapshot = &self.state.lock().await.snapshot;
            let mut ranges_to_drop = SmallVec::<[Range<usize>; 4]>::new();

            fn skip_idx(ranges: &mut SmallVec<[Range<usize>; 4]>, idx: usize) {
                if let Some(last_range) = ranges.last_mut()
                    && last_range.end == idx
                {
                    last_range.end += 1;
                } else {
                    ranges.push(idx..idx + 1);
                }
            }

            for (idx, event) in events.iter().enumerate() {
                let abs_path = SanitizedPath::new(&event.path);
                let relative_path = if let Ok(path) = abs_path
                    .as_path()
                    .strip_prefix(root_canonical_path.as_path())
                    && let Ok(path) = RelPath::new(path, PathStyle::local())
                {
                    path
                } else {
                    log::error!(
                        "Ignoring event {abs_path:?} outside of root path {root_canonical_path:?}",
                    );
                    skip_idx(&mut ranges_to_drop, idx);
                    continue;
                };

                let parent_dir_is_loaded = relative_path.parent().is_none_or(|parent| {
                    snapshot
                        .entry_for_path(parent)
                        .is_some_and(|entry| entry.kind == EntryKind::Dir)
                });
                if !parent_dir_is_loaded {
                    log::debug!("Ignoring event {relative_path:?} within unloaded directory");
                    skip_idx(&mut ranges_to_drop, idx);
                    continue;
                }

                relative_paths.push(EventRoot {
                    path: relative_path.into_owned().into(),
                    was_rescanned: matches!(event.kind, Some(PathEventKind::Rescan)),
                });
            }

            for range_to_drop in ranges_to_drop.into_iter().rev() {
                events.drain(range_to_drop);
            }
        }

        if relative_paths.is_empty() {
            return;
        }

        self.state.lock().await.snapshot.scan_id += 1;

        let (scan_job_tx, scan_job_rx) = channel::unbounded();
        log::debug!(
            "Received fs events {:?}",
            relative_paths
                .iter()
                .map(|event_root| &event_root.path)
                .collect::<Vec<_>>()
        );
        self.reload_entries_for_paths(
            root_path.as_ref(),
            root_canonical_path,
            &relative_paths
                .iter()
                .map(|event_root| event_root.path.clone())
                .collect::<Vec<_>>(),
            events
                .into_iter()
                .map(|event| event.path)
                .collect::<Vec<_>>(),
            Some(scan_job_tx.clone()),
        )
        .await;
        drop(scan_job_tx);
        self.scan_dirs(false, scan_job_rx).await;

        {
            let mut state = self.state.lock().await;
            state.snapshot.completed_scan_id = state.snapshot.scan_id;
            for (_, removed_entry) in std::mem::take(&mut state.removed_entries) {
                state.scanned_dirs.remove(&removed_entry.id);
            }
        }

        self.send_status_update(false, SmallVec::new(), &relative_paths)
            .await;
    }

    async fn send_status_update(
        &self,
        scanning: bool,
        completion_senders: SmallVec<[oneshot::Sender<()>; 1]>,
        event_roots: &[EventRoot],
    ) -> bool {
        let mut state = self.state.lock().await;
        if state.changed_paths.is_empty() && event_roots.is_empty() && scanning {
            for completion_sender in completion_senders {
                if completion_sender.send(()).is_err() {
                    log::trace!("Worktree scan completion receiver dropped");
                }
            }
            return true;
        }

        let merged_event_roots = merge_event_roots(&state.changed_paths, event_roots);
        let new_snapshot = state.snapshot.clone();
        let old_snapshot =
            std::mem::replace(&mut state.prev_snapshot, new_snapshot.snapshot.clone());
        let changes = build_diff(
            self.phase,
            &old_snapshot,
            &new_snapshot,
            &merged_event_roots,
        );
        state.changed_paths.clear();

        match self.status_updates_tx.unbounded_send(ScanState::Updated {
            snapshot: new_snapshot,
            changes,
            completion_senders,
            scanning,
        }) {
            Ok(()) => true,
            Err(error) => {
                match error.into_inner() {
                    ScanState::Updated {
                        completion_senders, ..
                    } => {
                        for completion_sender in completion_senders {
                            if completion_sender.send(()).is_err() {
                                log::trace!("Worktree scan completion receiver dropped");
                            }
                        }
                    }
                    ScanState::Started | ScanState::RootUpdated { .. } => {}
                }
                false
            }
        }
    }

    async fn scan_dirs(
        &self,
        enable_progress_updates: bool,
        scan_jobs_rx: channel::Receiver<ScanJob>,
    ) {
        if self
            .status_updates_tx
            .unbounded_send(ScanState::Started)
            .is_err()
        {
            return;
        }

        let progress_update_count = AtomicUsize::new(0);
        self.executor
            .scoped_priority(Priority::Low, |scope| {
                for _ in 0..self.executor.num_cpus() {
                    scope.spawn(async {
                        let mut last_progress_update_count = 0;
                        let progress_update_timer =
                            self.progress_timer(enable_progress_updates).fuse();
                        futures::pin_mut!(progress_update_timer);

                        loop {
                            select_biased! {
                                request = self.next_scan_request().fuse() => {
                                    let Ok(request) = request else { break };
                                    if !self.process_scan_request(request, true).await {
                                        return;
                                    }
                                }
                                () = progress_update_timer => {
                                    match progress_update_count.compare_exchange(
                                        last_progress_update_count,
                                        last_progress_update_count + 1,
                                        SeqCst,
                                        SeqCst
                                    ) {
                                        Ok(_) => {
                                            last_progress_update_count += 1;
                                            self.send_status_update(true, SmallVec::new(), &[])
                                                .await;
                                        }
                                        Err(count) => {
                                            last_progress_update_count = count;
                                        }
                                    }
                                    progress_update_timer
                                        .set(self.progress_timer(enable_progress_updates).fuse());
                                }
                                job = scan_jobs_rx.recv().fuse() => {
                                    let Ok(job) = job else { break };
                                    if let Err(error) = self.scan_dir(&job).await
                                        && job.path.is_empty() {
                                            log::error!("Error scanning directory {:?}: {error:#}", job.abs_path);
                                        }
                                }
                            }
                        }
                    });
                }
            })
            .await;
    }

    async fn scan_dir(&self, job: &ScanJob) -> anyhow::Result<()> {
        let root_abs_path = self.state.lock().await.snapshot.abs_path().clone();
        log::trace!("Scanning directory {:?}", job.path);

        let next_entry_id = self.next_entry_id.clone();
        let mut root_canonical_path = None;
        let mut new_entries: Vec<Entry> = Vec::new();
        let mut new_jobs: Vec<Option<ScanJob>> = Vec::new();
        let child_paths = self
            .fs
            .read_dir(&job.abs_path)
            .await?
            .filter_map(|entry| async {
                match entry {
                    Ok(entry) => Some(entry),
                    Err(error) => {
                        log::error!("Failed to read directory entry: {error:#}");
                        None
                    }
                }
            })
            .collect::<Vec<_>>()
            .await;

        for child_abs_path in child_paths {
            let child_abs_path: Arc<Path> = child_abs_path.into();
            let Some(child_name) = child_abs_path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let Some(child_name) = RelPath::unix(child_name).ok() else {
                continue;
            };
            let child_path = job.path.join(child_name);

            let child_metadata = match self.fs.metadata(child_abs_path.as_ref()).await {
                Ok(Some(metadata)) => metadata,
                Ok(None) => continue,
                Err(error) => {
                    log::error!("Failed to stat {}: {error:#}", child_abs_path.display());
                    continue;
                }
            };

            let mut child_entry = Entry::new(
                child_path.clone(),
                &child_metadata,
                ProjectEntryId::new(&next_entry_id),
                None,
            );

            if job.is_external {
                child_entry.is_external = true;
            } else if child_metadata.is_symlink {
                let canonical_path = match self.fs.canonicalize(&child_abs_path).await {
                    Ok(path) => path,
                    Err(error) => {
                        log::error!(
                            "Failed to canonicalize symbolic link {}: {error:#}",
                            child_abs_path.display(),
                        );
                        continue;
                    }
                };
                let root_canonical_path = match root_canonical_path.as_ref() {
                    Some(path) => path,
                    None => match self.fs.canonicalize(&root_abs_path).await {
                        Ok(path) => root_canonical_path.insert(path),
                        Err(error) => {
                            log::error!(
                                "Failed to canonicalize worktree root {}: {error:#}",
                                root_abs_path.display(),
                            );
                            continue;
                        }
                    },
                };

                if !canonical_path.starts_with(root_canonical_path) {
                    child_entry.is_external = true;
                }
                child_entry.canonical_path = Some(canonical_path.into());
            }

            if child_entry.is_dir() {
                if job.ancestor_inodes.contains(&child_entry.inode) {
                    new_jobs.push(None);
                } else {
                    let mut ancestor_inodes = job.ancestor_inodes.clone();
                    ancestor_inodes.insert(child_entry.inode);
                    new_jobs.push(Some(ScanJob {
                        abs_path: child_abs_path.clone(),
                        path: child_path,
                        scan_queue: job.scan_queue.clone(),
                        ancestor_inodes,
                        is_external: child_entry.is_external,
                    }));
                }

                new_entries.push(child_entry);
                continue;
            }

            if child_metadata.is_fifo || child_abs_path.extension() != Some(OsStr::new("toml")) {
                continue;
            }

            child_entry.request = Some(match self.fs.load(child_abs_path.as_ref()).await {
                Ok(contents) => parse_request_file(&contents),
                Err(error) => RequestFileState::Invalid(error.to_string()),
            });
            new_entries.push(child_entry);
        }

        {
            let mut state = self.state.lock().await;
            let mut job_index = 0;
            for entry in &mut new_entries {
                state.reuse_entry_id(entry);
                if entry.is_dir() {
                    if state.should_scan_directory(entry) {
                        job_index += 1;
                    } else {
                        log::debug!("Deferring scan for directory {:?}", entry.path);
                        entry.kind = EntryKind::UnloadedDir;
                        new_jobs.remove(job_index);
                    }
                }
            }
            state.populate_dir(job.path.clone(), new_entries);
        }

        self.watcher.add(job.abs_path.as_ref()).log_err();

        for new_job in new_jobs.into_iter().flatten() {
            let scan_queue = new_job.scan_queue.clone();
            scan_queue
                .try_send(new_job)
                .expect("scan job channel is unbounded");
        }

        Ok(())
    }

    async fn reload_entries_for_paths(
        &self,
        root_abs_path: &SanitizedPath,
        root_canonical_path: &SanitizedPath,
        relative_paths: &[Arc<RelPath>],
        abs_paths: Vec<PathBuf>,
        scan_queue_tx: Option<channel::Sender<ScanJob>>,
    ) {
        let metadata = futures::future::join_all(
            abs_paths
                .iter()
                .map(|abs_path| async move {
                    let metadata = self.fs.metadata(abs_path).await?;
                    if let Some(metadata) = metadata {
                        let canonical_path = self.fs.canonicalize(abs_path).await?;
                        if !self.fs_case_sensitive && !metadata.is_symlink {
                            let canonical_file_name = canonical_path.file_name();
                            let file_name = abs_path.file_name();
                            if canonical_file_name != file_name {
                                return Ok(None);
                            }
                        }
                        anyhow::Ok(Some((metadata, SanitizedPath::new_arc(&canonical_path))))
                    } else {
                        Ok(None)
                    }
                })
                .collect::<Vec<_>>(),
        )
        .await;

        let requests = futures::future::join_all(
            abs_paths
                .iter()
                .zip(metadata.iter())
                .map(|(abs_path, metadata)| async move {
                    let Ok(Some((metadata, _))) = metadata.as_ref() else {
                        return None;
                    };
                    if metadata.is_dir
                        || metadata.is_fifo
                        || abs_path.extension() != Some(OsStr::new("toml"))
                    {
                        return None;
                    }

                    Some(match self.fs.load(abs_path.as_path()).await {
                        Ok(contents) => parse_request_file(&contents),
                        Err(error) => RequestFileState::Invalid(error.to_string()),
                    })
                })
                .collect::<Vec<_>>(),
        )
        .await;

        let mut state = self.state.lock().await;
        let doing_recursive_update = scan_queue_tx.is_some();

        for (path, metadata) in relative_paths.iter().zip(metadata.iter()) {
            if matches!(metadata, Ok(None)) || doing_recursive_update {
                state.remove_path(path.as_ref(), self.watcher.as_ref());
            }
        }

        for ((path, metadata), request) in relative_paths.iter().zip(metadata).zip(requests) {
            let abs_path: Arc<Path> = root_abs_path.as_path().join(path.as_std_path()).into();
            match metadata {
                Ok(Some((metadata, canonical_path))) => {
                    if metadata.is_dir {
                        let entry_id = state.entry_id_for(
                            self.next_entry_id.as_ref(),
                            path.as_ref(),
                            &metadata,
                        );
                        let mut entry = Entry::new(
                            path.clone(),
                            &metadata,
                            entry_id,
                            metadata
                                .is_symlink
                                .then(|| canonical_path.as_path().to_path_buf().into()),
                        );
                        entry.is_external = !canonical_path.starts_with(root_canonical_path);
                        entry.kind =
                            if doing_recursive_update && state.should_scan_directory(&entry) {
                                EntryKind::PendingDir
                            } else if doing_recursive_update {
                                EntryKind::UnloadedDir
                            } else {
                                EntryKind::Dir
                            };
                        state.insert_entry(entry.clone());
                        if let Some(scan_queue_tx) = scan_queue_tx.as_ref()
                            && entry.kind == EntryKind::PendingDir
                        {
                            state.enqueue_scan_dir(abs_path.clone(), &entry, scan_queue_tx);
                        }
                    } else if let Some(request) = request {
                        let entry_id = state.entry_id_for(
                            self.next_entry_id.as_ref(),
                            path.as_ref(),
                            &metadata,
                        );
                        let mut entry = Entry::new(
                            path.clone(),
                            &metadata,
                            entry_id,
                            metadata
                                .is_symlink
                                .then(|| canonical_path.as_path().to_path_buf().into()),
                        );
                        entry.is_external = !canonical_path.starts_with(root_canonical_path);
                        entry.request = Some(request);
                        state.insert_entry(entry);
                    } else {
                        state.remove_path(path.as_ref(), self.watcher.as_ref());
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    log::error!("Failed to reload {}: {error:#}", abs_path.display());
                }
            }
        }

        util::extend_sorted(
            &mut state.changed_paths,
            relative_paths.iter().cloned(),
            usize::MAX,
            Ord::cmp,
        );
    }

    async fn progress_timer(&self, running: bool) {
        if !running {
            return futures::future::pending().await;
        }

        self.executor.timer(FS_WATCH_LATENCY).await;
    }
}

struct BackgroundScannerState {
    snapshot: LocalSnapshot,
    scanned_dirs: HashSet<ProjectEntryId>,
    path_prefixes_to_scan: HashSet<Arc<RelPath>>,
    paths_to_scan: HashSet<Arc<RelPath>>,
    removed_entries: HashMap<u64, Entry>,
    changed_paths: Vec<Arc<RelPath>>,
    scanning_enabled: bool,
    prev_snapshot: Snapshot,
}

impl BackgroundScannerState {
    fn reuse_entry_id(&mut self, entry: &mut Entry) {
        if let Some(mtime) = entry.mtime {
            if let Some(removed_entry) = self.removed_entries.remove(&entry.inode) {
                if removed_entry.mtime == Some(mtime) || removed_entry.path == entry.path {
                    entry.id = removed_entry.id;
                }
            } else if let Some(existing_entry) = self.snapshot.entry_for_path(&entry.path) {
                entry.id = existing_entry.id;
            }
        }
    }

    fn entry_id_for(
        &mut self,
        next_entry_id: &AtomicUsize,
        path: &RelPath,
        metadata: &FsMetadata,
    ) -> ProjectEntryId {
        if let Some(removed_entry) = self.removed_entries.remove(&metadata.inode) {
            if removed_entry.mtime == Some(metadata.mtime) || *removed_entry.path == *path {
                return removed_entry.id;
            }
        } else if let Some(existing_entry) = self.snapshot.entry_for_path(path) {
            return existing_entry.id;
        }

        ProjectEntryId::new(next_entry_id)
    }

    fn should_scan_directory(&self, entry: &Entry) -> bool {
        (self.scanning_enabled && !entry.is_external)
            || self.scanned_dirs.contains(&entry.id)
            || self
                .paths_to_scan
                .iter()
                .any(|path| path.starts_with(entry.path.as_ref()))
            || self
                .path_prefixes_to_scan
                .iter()
                .any(|path_prefix| entry.path.as_ref().starts_with(path_prefix.as_ref()))
    }

    fn enqueue_scan_dir(
        &self,
        abs_path: Arc<Path>,
        entry: &Entry,
        scan_job_tx: &channel::Sender<ScanJob>,
    ) {
        let path = entry.path.clone();
        let mut ancestor_inodes = self.snapshot.ancestor_inodes_for_path(path.as_ref());

        if !ancestor_inodes.contains(&entry.inode) {
            ancestor_inodes.insert(entry.inode);
            scan_job_tx
                .try_send(ScanJob {
                    abs_path,
                    path,
                    scan_queue: scan_job_tx.clone(),
                    ancestor_inodes,
                    is_external: entry.is_external,
                })
                .expect("scan job channel is unbounded");
        }
    }

    fn populate_dir(
        &mut self,
        parent_path: Arc<RelPath>,
        entries: impl IntoIterator<Item = Entry>,
    ) {
        let mut parent_entry = if let Some(parent_entry) = self
            .snapshot
            .entries_by_path
            .get(&PathKey(parent_path.clone()), ())
        {
            parent_entry.clone()
        } else {
            log::warn!("Populating a directory {parent_path:?} that has been removed");
            return;
        };

        match parent_entry.kind {
            EntryKind::PendingDir | EntryKind::UnloadedDir => parent_entry.kind = EntryKind::Dir,
            EntryKind::Dir => {}
            _ => return,
        }

        let parent_entry_id = parent_entry.id;
        self.scanned_dirs.insert(parent_entry_id);

        let mut entries_by_path_edits = vec![Edit::Insert(parent_entry)];
        let mut entries_by_id_edits = Vec::new();
        for entry in entries {
            self.removed_entries.remove(&entry.inode);
            entries_by_id_edits.push(Edit::Insert(entry.to_path_entry()));
            entries_by_path_edits.push(Edit::Insert(entry));
        }

        self.snapshot
            .entries_by_path
            .edit(entries_by_path_edits, ());
        self.snapshot.entries_by_id.edit(entries_by_id_edits, ());

        if let Err(index) = self.changed_paths.binary_search(&parent_path) {
            self.changed_paths.insert(index, parent_path.clone());
        }

        #[cfg(feature = "test-support")]
        self.snapshot.check_invariants();
    }

    fn insert_entry(&mut self, entry: Entry) {
        self.removed_entries.remove(&entry.inode);
        self.snapshot.insert_entry(entry);

        #[cfg(feature = "test-support")]
        self.snapshot.check_invariants();
    }

    fn remove_path(&mut self, path: &RelPath, watcher: &dyn FsWatcher) {
        let mut new_entries;
        let removed_entries;
        {
            let mut cursor = self
                .snapshot
                .entries_by_path
                .cursor::<TraversalProgress>(());
            new_entries = cursor.slice(&TraversalTarget::path(path), Bias::Left);
            removed_entries = cursor.slice(&TraversalTarget::successor(path), Bias::Left);
            new_entries.append(cursor.suffix(), ());
        }
        self.snapshot.entries_by_path = new_entries;

        let mut removed_ids = Vec::with_capacity(removed_entries.summary().count);
        let mut removed_dir_abs_paths = Vec::new();
        for entry in removed_entries.cursor::<()>(()) {
            if entry.is_dir() {
                removed_dir_abs_paths.push(self.snapshot.absolutize(&entry.path));
            }
            if let Some(previous_entry) = self.removed_entries.get_mut(&entry.inode) {
                if entry.id > previous_entry.id {
                    *previous_entry = entry.clone();
                }
            } else {
                self.removed_entries.insert(entry.inode, entry.clone());
            }
            if let Err(index) = removed_ids.binary_search(&entry.id) {
                removed_ids.insert(index, entry.id);
            }
        }

        self.snapshot.entries_by_id.edit(
            removed_ids
                .iter()
                .map(|entry_id| Edit::Remove(*entry_id))
                .collect(),
            (),
        );

        for removed_dir_abs_path in removed_dir_abs_paths {
            watcher.remove(removed_dir_abs_path.as_path()).log_err();
        }

        #[cfg(feature = "test-support")]
        self.snapshot.check_invariants();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EventRoot {
    path: Arc<RelPath>,
    was_rescanned: bool,
}

struct ScanJob {
    abs_path: Arc<Path>,
    path: Arc<RelPath>,
    scan_queue: channel::Sender<ScanJob>,
    ancestor_inodes: HashSet<u64>,
    is_external: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BackgroundScannerPhase {
    InitialScan,
    EventsReceivedDuringInitialScan,
    Events,
}

struct NullWatcher;

impl fs::Watcher for NullWatcher {
    fn add(&self, _path: &Path) -> anyhow::Result<()> {
        Ok(())
    }

    fn remove(&self, _path: &Path) -> anyhow::Result<()> {
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct TraversalProgress<'a> {
    max_path: &'a RelPath,
    count: usize,
    file_count: usize,
}

impl TraversalProgress<'_> {
    fn count(&self, include_files: bool, include_dirs: bool) -> usize {
        match (include_files, include_dirs) {
            (true, true) => self.count,
            (true, false) => self.file_count,
            (false, true) => self.count - self.file_count,
            (false, false) => 0,
        }
    }
}

impl<'a> Dimension<'a, EntrySummary> for TraversalProgress<'a> {
    fn zero((): ()) -> Self {
        Self::default()
    }

    fn add_summary(&mut self, summary: &'a EntrySummary, (): ()) {
        self.max_path = summary.max_path.as_ref();
        self.count += summary.count;
        self.file_count += summary.file_count;
    }
}

impl Default for TraversalProgress<'_> {
    fn default() -> Self {
        Self {
            max_path: RelPath::empty(),
            count: 0,
            file_count: 0,
        }
    }
}

#[derive(Debug)]
pub struct Traversal<'a> {
    snapshot: &'a Snapshot,
    cursor: sum_tree::Cursor<'a, 'static, Entry, TraversalProgress<'a>>,
    include_files: bool,
    include_dirs: bool,
}

impl<'a> Traversal<'a> {
    fn new(
        snapshot: &'a Snapshot,
        include_files: bool,
        include_dirs: bool,
        start_path: &RelPath,
    ) -> Self {
        let mut cursor = snapshot.entries_by_path.cursor(());
        cursor.seek(&TraversalTarget::path(start_path), Bias::Left);
        let mut traversal = Self {
            snapshot,
            cursor,
            include_files,
            include_dirs,
        };
        if traversal.end_offset() == traversal.start_offset() {
            traversal.next();
        }
        traversal
    }

    pub fn advance(&mut self) -> bool {
        self.advance_by(1)
    }

    pub fn advance_by(&mut self, count: usize) -> bool {
        self.cursor.seek_forward(
            &TraversalTarget::Count {
                count: self.end_offset() + count,
                include_files: self.include_files,
                include_dirs: self.include_dirs,
            },
            Bias::Left,
        )
    }

    pub fn advance_to_sibling(&mut self) -> bool {
        while let Some(entry) = self.cursor.item() {
            self.cursor
                .seek_forward(&TraversalTarget::successor(entry.path.as_ref()), Bias::Left);
            if let Some(entry) = self.cursor.item()
                && (self.include_files || !entry.is_file())
                && (self.include_dirs || !entry.is_dir())
            {
                return true;
            }
        }
        false
    }

    pub fn back_to_parent(&mut self) -> bool {
        let Some(parent_path) = self.cursor.item().and_then(|entry| entry.path.parent()) else {
            return false;
        };
        self.cursor
            .seek(&TraversalTarget::path(parent_path), Bias::Left)
    }

    pub fn entry(&self) -> Option<&'a Entry> {
        self.cursor.item()
    }

    pub fn snapshot(&self) -> &'a Snapshot {
        self.snapshot
    }

    pub fn start_offset(&self) -> usize {
        self.cursor
            .start()
            .count(self.include_files, self.include_dirs)
    }

    pub fn end_offset(&self) -> usize {
        self.cursor
            .end()
            .count(self.include_files, self.include_dirs)
    }
}

impl<'a> Iterator for Traversal<'a> {
    type Item = &'a Entry;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(item) = self.entry() {
            self.advance();
            Some(item)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum PathTarget<'a> {
    Path(&'a RelPath),
    Successor(&'a RelPath),
}

impl PathTarget<'_> {
    fn cmp_path(&self, other: &RelPath) -> Ordering {
        match self {
            Self::Path(path) => path.cmp(&other),
            Self::Successor(path) => {
                if other.starts_with(path) {
                    Ordering::Greater
                } else {
                    Ordering::Equal
                }
            }
        }
    }
}

impl<'a> SeekTarget<'a, EntrySummary, TraversalProgress<'a>> for PathTarget<'_> {
    fn cmp(&self, cursor_location: &TraversalProgress<'a>, (): ()) -> Ordering {
        self.cmp_path(cursor_location.max_path)
    }
}

#[derive(Debug, Clone, Copy)]
enum TraversalTarget<'a> {
    Path(PathTarget<'a>),
    Count {
        count: usize,
        include_files: bool,
        include_dirs: bool,
    },
}

impl<'a> TraversalTarget<'a> {
    fn path(path: &'a RelPath) -> Self {
        Self::Path(PathTarget::Path(path))
    }

    fn successor(path: &'a RelPath) -> Self {
        Self::Path(PathTarget::Successor(path))
    }

    fn cmp_progress(&self, progress: &TraversalProgress) -> Ordering {
        match self {
            Self::Path(path) => path.cmp_path(progress.max_path),
            Self::Count {
                count,
                include_files,
                include_dirs,
            } => Ord::cmp(count, &progress.count(*include_files, *include_dirs)),
        }
    }
}

impl<'a> SeekTarget<'a, EntrySummary, TraversalProgress<'a>> for TraversalTarget<'_> {
    fn cmp(&self, cursor_location: &TraversalProgress<'a>, (): ()) -> Ordering {
        self.cmp_progress(cursor_location)
    }
}

pub struct ChildEntriesOptions {
    pub include_files: bool,
    pub include_dirs: bool,
}

pub struct ChildEntriesIter<'a> {
    parent_path: &'a RelPath,
    traversal: Traversal<'a>,
}

impl<'a> Iterator for ChildEntriesIter<'a> {
    type Item = &'a Entry;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(item) = self.traversal.entry()
            && item.path.starts_with(self.parent_path)
        {
            self.traversal.advance_to_sibling();
            return Some(item);
        }
        None
    }
}

fn merge_event_roots(changed_paths: &[Arc<RelPath>], event_roots: &[EventRoot]) -> Vec<EventRoot> {
    let mut merged_event_roots = Vec::with_capacity(changed_paths.len() + event_roots.len());
    let mut changed_paths = changed_paths.iter().peekable();
    let mut event_roots = event_roots.iter().peekable();
    while let (Some(path), Some(event_root)) = (changed_paths.peek(), event_roots.peek()) {
        match path.cmp(&&event_root.path) {
            Ordering::Less => {
                merged_event_roots.push(EventRoot {
                    path: (*changed_paths.next().expect("peeked changed path")).clone(),
                    was_rescanned: false,
                });
            }
            Ordering::Equal => {
                merged_event_roots.push((*event_roots.next().expect("peeked event root")).clone());
                changed_paths.next();
            }
            Ordering::Greater => {
                merged_event_roots.push((*event_roots.next().expect("peeked event root")).clone());
            }
        }
    }
    merged_event_roots.extend(changed_paths.map(|path| EventRoot {
        path: path.clone(),
        was_rescanned: false,
    }));
    merged_event_roots.extend(event_roots.cloned());
    merged_event_roots
}

fn build_diff(
    phase: BackgroundScannerPhase,
    old_snapshot: &Snapshot,
    new_snapshot: &Snapshot,
    event_roots: &[EventRoot],
) -> UpdatedEntriesSet {
    use BackgroundScannerPhase::{EventsReceivedDuringInitialScan, InitialScan};
    use PathChange::{Added, AddedOrUpdated, Loaded, Removed, Updated};

    let mut changes = Vec::new();
    let mut old_paths = old_snapshot.entries_by_path.cursor::<PathKey>(());
    let mut new_paths = new_snapshot.entries_by_path.cursor::<PathKey>(());
    let mut last_newly_loaded_dir_path = None;
    old_paths.next();
    new_paths.next();
    for event_root in event_roots {
        let path = PathKey(event_root.path.clone());
        if old_paths.item().is_some_and(|entry| entry.path < path.0) {
            old_paths.seek_forward(&path, Bias::Left);
        }
        if new_paths.item().is_some_and(|entry| entry.path < path.0) {
            new_paths.seek_forward(&path, Bias::Left);
        }
        loop {
            match (old_paths.item(), new_paths.item()) {
                (Some(old_entry), Some(new_entry)) => {
                    if old_entry.path > path.0
                        && new_entry.path > path.0
                        && !old_entry.path.starts_with(&path.0)
                        && !new_entry.path.starts_with(&path.0)
                    {
                        break;
                    }

                    match Ord::cmp(&old_entry.path, &new_entry.path) {
                        Ordering::Less => {
                            changes.push((old_entry.path.clone(), old_entry.id, Removed));
                            old_paths.next();
                        }
                        Ordering::Equal => {
                            if phase == EventsReceivedDuringInitialScan {
                                if old_entry.id != new_entry.id {
                                    changes.push((old_entry.path.clone(), old_entry.id, Removed));
                                }
                                changes.push((
                                    new_entry.path.clone(),
                                    new_entry.id,
                                    AddedOrUpdated,
                                ));
                            } else if old_entry.id != new_entry.id {
                                changes.push((old_entry.path.clone(), old_entry.id, Removed));
                                changes.push((new_entry.path.clone(), new_entry.id, Added));
                            } else if old_entry != new_entry {
                                if old_entry.kind.is_unloaded() {
                                    last_newly_loaded_dir_path = Some(&new_entry.path);
                                    changes.push((new_entry.path.clone(), new_entry.id, Loaded));
                                } else {
                                    changes.push((new_entry.path.clone(), new_entry.id, Updated));
                                }
                            } else if event_root.was_rescanned {
                                changes.push((new_entry.path.clone(), new_entry.id, Updated));
                            }
                            old_paths.next();
                            new_paths.next();
                        }
                        Ordering::Greater => {
                            let is_newly_loaded = phase == InitialScan
                                || last_newly_loaded_dir_path
                                    .as_ref()
                                    .is_some_and(|dir| new_entry.path.starts_with(dir));
                            changes.push((
                                new_entry.path.clone(),
                                new_entry.id,
                                if is_newly_loaded { Loaded } else { Added },
                            ));
                            new_paths.next();
                        }
                    }
                }
                (Some(old_entry), None) => {
                    changes.push((old_entry.path.clone(), old_entry.id, Removed));
                    old_paths.next();
                }
                (None, Some(new_entry)) => {
                    let is_newly_loaded = phase == InitialScan
                        || last_newly_loaded_dir_path
                            .as_ref()
                            .is_some_and(|dir| new_entry.path.starts_with(dir));
                    changes.push((
                        new_entry.path.clone(),
                        new_entry.id,
                        if is_newly_loaded { Loaded } else { Added },
                    ));
                    new_paths.next();
                }
                (None, None) => break,
            }
        }
    }

    changes.into()
}
