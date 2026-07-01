mod ignore;
mod request;

pub use language::DiskState;
pub use request::{
    REQUEST_FILE_VERSION, RequestFile, RequestFileBody, RequestFileBodyType, RequestFileHeader,
    RequestFileHttp, RequestFileMeta, RequestFileParam, RequestFileState, parse_request_file,
    request_method_label, serialize_request_file,
};
pub use settings::WorktreeId;

use ::ignore::gitignore::{Gitignore, GitignoreBuilder};
use anyhow::{Context as AnyhowContext, anyhow};
use async_lock::Mutex;
#[cfg(feature = "test")]
use futures::future::LocalBoxFuture;
use futures::{FutureExt, Stream, StreamExt};
#[cfg(feature = "test")]
use gpui::TestAppContext;
use gpui::{
    App, AppContext, AsyncApp, BackgroundExecutor, Context, Entity, EventEmitter, Priority, Task,
};
use smallvec::{SmallVec, smallvec};
use smol::channel;
use std::{
    cmp::Ordering,
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
use sum_tree::{
    Bias, ContextLessSummary, Dimension, Dimensions, Edit, SeekTarget, SumTree, TreeMap,
};
use tokio::sync::{oneshot, watch};

#[cfg(feature = "test")]
use collections::BTreeSet;
use collections::{HashMap, HashSet};
use fs::{
    FileHandle, Fs, MTime, Metadata as FsMetadata, PathEvent, PathEventKind, RemoveOptions,
    Watcher as FsWatcher,
};
use git::{
    BISECT_LOG, COMMIT_MESSAGE, DOT_GIT, FETCH_HEAD, FSMONITOR_DAEMON, GC_PID, GITIGNORE,
    HOOKS_DIR, INFO_DIR, LFS_DIR, LOGS_DIR, LOGS_REF_STASH, OBJECTS_DIR, ORIG_HEAD,
    REBASE_APPLY_DIR, REBASE_MERGE_DIR, REPO_EXCLUDE, SEQUENCER_DIR, status::GitSummary,
};
use language::{LineEnding, Rope};
use path::{PathStyle, RelPath, SanitizedPath};
use util::ResultExt;

use crate::ignore::{IgnoreKind, IgnoreStack};

pub const FS_WATCH_LATENCY: Duration = Duration::from_millis(100);

pub struct Worktree {
    snapshot: WorktreeSnapshot,
    scan_requests_tx: channel::Sender<ScanRequest>,
    path_prefixes_to_scan_tx: channel::Sender<PathPrefixScanRequest>,
    is_scanning: (watch::Sender<bool>, watch::Receiver<bool>),
    background_scanner_tasks: Vec<Task<()>>,
    fs: Arc<dyn Fs>,
    fs_case_sensitive: bool,
    visible: bool,
    next_entry_id: Arc<AtomicUsize>,
    scanning_enabled: bool,
}

impl Worktree {
    pub async fn new(
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

        let path_style = PathStyle::local();
        let fs_case_sensitive = fs.is_case_sensitive().await;
        let root_name = match opened_abs_path.file_name() {
            Some(file_name) => {
                let file_name = file_name
                    .to_str()
                    .context("worktree root name should be valid utf-8")?;
                RelPath::unix(file_name)
                    .context("failed to parse worktree root name")
                    .map(Arc::<RelPath>::from)?
            }
            None => RelPath::empty().into(),
        };
        let root_file_handle = if metadata.as_ref().is_some() {
            fs.open_handle(opened_abs_path.as_ref())
                .await
                .with_context(|| {
                    format!(
                        "failed to open worktree root at {}",
                        opened_abs_path.display()
                    )
                })
                .log_err()
        } else {
            None
        };

        Ok(cx.new(move |cx: &mut Context<Self>| {
            let mut snapshot = WorktreeSnapshot {
                snapshot: Snapshot::new(
                    worktree_id,
                    root_name,
                    opened_abs_path.clone(),
                    path_style,
                ),
                ignores_by_parent_abs_path: HashMap::default(),
                git_repositories: TreeMap::default(),
                root_file_handle,
            };
            if let Some(metadata) = metadata {
                let mut root_entry = Entry::new(
                    Arc::from(RelPath::empty()),
                    &metadata,
                    ProjectEntryId::new(next_entry_id.as_ref()),
                    None,
                );
                if metadata.is_dir {
                    root_entry.kind = if scanning_enabled {
                        EntryKind::PendingDir
                    } else {
                        EntryKind::UnloadedDir
                    };
                }
                cx.foreground_executor()
                    .block_on(snapshot.insert_entry(root_entry, fs.as_ref()));
            }

            let (scan_requests_tx, scan_requests_rx) = channel::unbounded();
            let (path_prefixes_to_scan_tx, path_prefixes_to_scan_rx) = channel::unbounded();
            let mut worktree = Worktree {
                snapshot,
                scan_requests_tx,
                path_prefixes_to_scan_tx,
                is_scanning: watch::channel(true),
                background_scanner_tasks: Vec::new(),
                fs,
                fs_case_sensitive,
                visible,
                next_entry_id,
                scanning_enabled,
            };
            worktree.start_background_scanner(scan_requests_rx, path_prefixes_to_scan_rx, cx);
            worktree
        }))
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    fn is_file_worktree(&self) -> bool {
        self.root_dir().is_none()
    }

    pub fn full_path(&self, worktree_relative_path: &RelPath) -> PathBuf {
        self.root_name()
            .join(worktree_relative_path)
            .display(self.path_style())
            .to_string()
            .into()
    }

    pub fn write_request_file(
        &self,
        path: Arc<RelPath>,
        request_file: RequestFile,
        cx: &Context<Self>,
    ) -> Task<anyhow::Result<Arc<File>>> {
        let fs = self.fs().clone();
        let abs_path = self.absolutize(&path);
        let write_task = cx.background_spawn(async move {
            let contents = request::serialize_request_file(&request_file)?;
            fs.write(&abs_path, contents.as_bytes()).await
        });

        cx.spawn(async move |this, cx| {
            write_task.await?;
            let entry = this
                .update(cx, |worktree, cx| {
                    worktree.refresh_entry(path.clone(), None, cx)
                })?
                .await?;
            let worktree = this.upgrade().context("worktree dropped")?;
            Ok(File::for_entry(&entry, worktree))
        })
    }

    fn descendant_entry_ids(&self, path: &RelPath) -> Vec<ProjectEntryId> {
        fn inner(worktree: &Worktree, path: &RelPath, entry_ids: &mut Vec<ProjectEntryId>) {
            for entry in worktree.child_entries(path) {
                entry_ids.push(entry.id);
                inner(worktree, entry.path.as_ref(), entry_ids);
            }
        }

        let mut entry_ids = Vec::new();
        inner(self, path, &mut entry_ids);
        entry_ids
    }

    pub fn fs(&self) -> &Arc<dyn Fs> {
        &self.fs
    }

    pub fn fs_is_case_sensitive(&self) -> bool {
        self.fs_case_sensitive
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot::clone(&self.snapshot)
    }

    pub fn refresh_entries_for_paths(
        &self,
        relative_paths: Vec<Arc<RelPath>>,
    ) -> oneshot::Receiver<()> {
        let (tx, rx) = oneshot::channel();
        let request = ScanRequest {
            relative_paths,
            completion_senders: smallvec![tx],
        };
        if self.scan_requests_tx.try_send(request).is_err() {
            log::trace!("Worktree scan request receiver dropped");
        }
        rx
    }

    pub fn add_path_prefix_to_scan(&self, path_prefix: Arc<RelPath>) -> oneshot::Receiver<()> {
        let (tx, rx) = oneshot::channel();
        let request = PathPrefixScanRequest {
            path: path_prefix,
            completion_senders: smallvec![tx],
        };
        if self.path_prefixes_to_scan_tx.try_send(request).is_err() {
            log::trace!("Worktree path prefix scan request receiver dropped");
        }
        rx
    }

    fn lowest_ancestor(&self, path: &RelPath) -> Arc<RelPath> {
        let mut lowest_ancestor = None;
        for path in path.ancestors() {
            if self.entry_for_path(path).is_some() {
                lowest_ancestor = Some(path.into());
                break;
            }
        }

        lowest_ancestor.unwrap_or_else(|| RelPath::empty().into())
    }

    pub fn refresh_entry(
        &self,
        path: Arc<RelPath>,
        old_path: Option<&Arc<RelPath>>,
        cx: &Context<Worktree>,
    ) -> Task<anyhow::Result<Entry>> {
        let paths = if let Some(old_path) = old_path {
            vec![old_path.clone(), path.clone()]
        } else {
            vec![path.clone()]
        };
        let refresh_task = self.refresh_entries_for_paths(paths);
        cx.spawn(async move |this, cx| {
            refresh_task.await.context("Failed to refresh entry")?;
            let new_entry = this.read_with(cx, |this, _| {
                this.entry_for_path(&path).cloned().ok_or_else(|| {
                    anyhow!("Could not find entry in worktree for {path:?} after refresh")
                })
            })??;
            Ok(new_entry)
        })
    }

    pub fn load_file(
        &self,
        path: &RelPath,
        cx: &Context<Worktree>,
    ) -> Task<anyhow::Result<LoadedFile>> {
        let path: Arc<RelPath> = Arc::from(path);
        let abs_path = self.absolutize(path.as_ref());
        let fs = self.fs.clone();
        let refresh_task = self.refresh_entry(path.clone(), None, cx);
        let worktree = cx.weak_entity();

        cx.background_spawn(async move {
            let text = fs.load(abs_path.as_path()).await?;
            let worktree = worktree.upgrade().context("worktree was dropped")?;
            let entry = refresh_task.await?;
            let file = File::for_entry(&entry, worktree);
            Ok(LoadedFile { file, text })
        })
    }

    pub fn write_file(
        &self,
        path: Arc<RelPath>,
        text: Rope,
        line_ending: LineEnding,
        cx: &Context<Worktree>,
    ) -> Task<anyhow::Result<Arc<File>>> {
        let fs = self.fs.clone();
        let abs_path = self.absolutize(path.as_ref());
        let write_task = cx.background_spawn({
            let fs = fs.clone();
            let abs_path = abs_path.clone();
            async move {
                let text = match line_ending {
                    LineEnding::Unix => text.to_string(),
                    LineEnding::Windows => text.to_string().replace('\n', "\r\n"),
                };
                fs.write(abs_path.as_path(), text.as_bytes()).await
            }
        });

        cx.spawn(async move |this, cx| {
            write_task.await?;
            let entry = this
                .update(cx, |worktree, cx| {
                    worktree.refresh_entry(path.clone(), None, cx)
                })?
                .await?;
            let worktree = this.upgrade().context("worktree dropped")?;
            Ok(File::for_entry(&entry, worktree))
        })
    }

    pub fn expand_entry(
        &self,
        entry_id: ProjectEntryId,
        cx: &Context<Worktree>,
    ) -> Option<Task<anyhow::Result<()>>> {
        let path = self.entry_for_id(entry_id)?.path.clone();
        let refresh_task = self.refresh_entries_for_paths(vec![path]);
        Some(cx.background_spawn(async move {
            refresh_task.await.context("Failed to expand entry")?;
            Ok(())
        }))
    }

    pub fn create_entry(
        &self,
        path: Arc<RelPath>,
        is_dir: bool,
        content: Option<Vec<u8>>,
        cx: &Context<Worktree>,
    ) -> Task<anyhow::Result<Entry>> {
        let abs_path = self.absolutize(&path);
        let fs = self.fs.clone();
        let task_abs_path = abs_path.clone();
        let write_task = cx.background_spawn(async move {
            if is_dir {
                fs.create_dir(&task_abs_path)
                    .await
                    .with_context(|| format!("creating directory {}", task_abs_path.display()))
            } else {
                fs.write(&task_abs_path, content.as_deref().unwrap_or(&[]))
                    .await
                    .with_context(|| format!("creating file {}", task_abs_path.display()))
            }
        });

        let lowest_ancestor = self.lowest_ancestor(&path);
        cx.spawn(async move |this, cx| {
            write_task.await?;
            let (result_task, refresh_tasks) = this.update(cx, |worktree, cx| {
                let mut refresh_tasks = Vec::new();
                let Ok(refresh_paths) = path.strip_prefix(&lowest_ancestor) else {
                    return (
                        Task::ready(Err(anyhow!(
                            "Could not refresh created entry at {}",
                            abs_path.display()
                        ))),
                        Vec::new(),
                    );
                };
                for refresh_path in refresh_paths.ancestors() {
                    if refresh_path == RelPath::empty() {
                        continue;
                    }
                    let refresh_full_path = lowest_ancestor.join(refresh_path);

                    refresh_tasks.push(worktree.refresh_entry(refresh_full_path, None, cx));
                }

                (
                    worktree.refresh_entry(path.clone(), None, cx),
                    refresh_tasks,
                )
            })?;
            for task in refresh_tasks {
                task.await.log_err();
            }

            result_task.await
        })
    }

    pub fn delete_entry(
        &mut self,
        entry_id: ProjectEntryId,
        trash: bool,
        cx: &mut Context<Worktree>,
    ) -> Option<Task<anyhow::Result<()>>> {
        let entry = self.entry_for_id(entry_id)?.clone();
        let abs_path = self.absolutize(&entry.path);
        let fs = self.fs.clone();
        let path = entry.path.clone();

        let delete_task = cx.background_spawn(async move {
            match (entry.is_file(), trash) {
                (true, true) => fs.trash(&abs_path, RemoveOptions::default()).await?,
                (false, true) => {
                    fs.trash(
                        &abs_path,
                        RemoveOptions {
                            recursive: true,
                            ignore_if_not_exists: false,
                        },
                    )
                    .await?;
                }
                (true, false) => {
                    fs.remove_file(&abs_path, RemoveOptions::default()).await?;
                }
                (false, false) => {
                    fs.remove_dir(
                        &abs_path,
                        RemoveOptions {
                            recursive: true,
                            ignore_if_not_exists: false,
                        },
                    )
                    .await?;
                }
            }

            anyhow::Ok(entry.path)
        });

        for entry_id in std::iter::once(entry_id).chain(self.descendant_entry_ids(path.as_ref())) {
            cx.emit(WorktreeEvent::DeletedEntry(entry_id));
        }

        Some(cx.spawn(async move |this, cx| {
            let path = delete_task.await?;
            let refresh_task =
                this.update(cx, |this, _| this.refresh_entries_for_paths(vec![path]))?;
            refresh_task
                .await
                .context("Failed to refresh deleted entry")?;

            Ok(())
        }))
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

    pub fn update_abs_path_and_refresh(
        &mut self,
        new_path: Arc<SanitizedPath>,
        cx: &Context<Worktree>,
    ) {
        self.snapshot.ignores_by_parent_abs_path = HashMap::default();
        let root_name = match new_path.as_path().file_name() {
            Some(file_name) => {
                let file_name = file_name
                    .to_str()
                    .expect("worktree root name should be valid utf-8");
                RelPath::unix(file_name)
                    .expect("worktree root name should be a valid relative path")
                    .into()
            }
            None => RelPath::empty().into(),
        };

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
        let is_file_worktree = self.is_file_worktree();
        let fs = self.fs.clone();
        let next_entry_id = self.next_entry_id.clone();
        let scanning_enabled = self.scanning_enabled;
        let executor = cx.background_executor().clone();
        let (scan_states_tx, mut scan_states_rx) = futures::channel::mpsc::unbounded();

        let background_scanner_task = cx.background_spawn(async move {
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
                    scanned_dirs: HashSet::default(),
                    path_prefixes_to_scan: HashSet::default(),
                    paths_to_scan: HashSet::default(),
                    removed_entries: HashMap::default(),
                    changed_paths: Vec::default(),
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
                is_file_worktree,
            };
            background_scanner.run(events).await;
        });

        let scan_state_updater_task = cx.spawn(async move |this, cx| {
            while let Some((state, this)) = scan_states_rx.next().await.zip(this.upgrade()) {
                this.update(cx, |this, cx| match state {
                    ScanState::Started => {
                        this.is_scanning.0.send_replace(true);
                    }
                    ScanState::Updated {
                        snapshot,
                        changes,
                        completion_senders,
                        scanning,
                    } => {
                        this.is_scanning.0.send_replace(scanning);
                        this.set_snapshot(snapshot, changes, cx);
                        for completion_sender in completion_senders {
                            if completion_sender.send(()).is_err() {
                                log::trace!("Worktree scan completion receiver dropped");
                            }
                        }
                    }
                    ScanState::RootUpdated { new_path } => {
                        this.update_abs_path_and_refresh(new_path, cx);
                    }
                    ScanState::RootDeleted => {
                        log::info!(
                            "Worktree root {} no longer exists, closing worktree",
                            this.abs_path().display()
                        );
                        cx.emit(WorktreeEvent::Deleted);
                    }
                });
            }
        });

        self.background_scanner_tasks = vec![background_scanner_task, scan_state_updater_task];
        self.is_scanning.0.send_replace(true);
    }

    fn set_snapshot(
        &mut self,
        snapshot: WorktreeSnapshot,
        changes: UpdatedEntriesSet,
        cx: &mut Context<Worktree>,
    ) {
        let repo_changes = self.changed_repos(&self.snapshot, &snapshot);
        self.snapshot = snapshot;
        if !changes.is_empty() {
            cx.emit(WorktreeEvent::UpdatedEntries(changes));
        }
        if !repo_changes.is_empty() {
            cx.emit(WorktreeEvent::UpdatedGitRepositories(repo_changes));
        }
    }

    fn changed_repos(
        &self,
        old_snapshot: &WorktreeSnapshot,
        new_snapshot: &WorktreeSnapshot,
    ) -> UpdatedGitRepositoriesSet {
        let clone_entry = |entry: &(&ProjectEntryId, &RepositoryEntry)| (*entry.0, entry.1.clone());
        let mut changes = Vec::new();
        let mut old_repos = old_snapshot.git_repositories.iter().peekable();
        let new_repos = new_snapshot.git_repositories.clone();
        let mut new_repos = new_repos.iter().peekable();

        loop {
            match (
                new_repos.peek().map(clone_entry),
                old_repos.peek().map(clone_entry),
            ) {
                (Some((new_entry_id, new_repo)), Some((old_entry_id, old_repo))) => {
                    match Ord::cmp(&new_entry_id, &old_entry_id) {
                        Ordering::Less => {
                            changes.push(UpdatedGitRepository {
                                work_directory_id: new_entry_id,
                                old_work_directory_abs_path: None,
                                new_work_directory_abs_path: Some(
                                    new_repo.work_directory_abs_path.clone(),
                                ),
                                dot_git_abs_path: Some(new_repo.dot_git_abs_path.clone()),
                                repository_dir_abs_path: Some(
                                    new_repo.repository_dir_abs_path.clone(),
                                ),
                                common_dir_abs_path: Some(new_repo.common_dir_abs_path.clone()),
                            });
                            new_repos.next();
                        }
                        Ordering::Equal => {
                            if new_repo.git_dir_scan_id != old_repo.git_dir_scan_id
                                || new_repo.work_directory_abs_path
                                    != old_repo.work_directory_abs_path
                            {
                                changes.push(UpdatedGitRepository {
                                    work_directory_id: new_entry_id,
                                    old_work_directory_abs_path: Some(
                                        old_repo.work_directory_abs_path.clone(),
                                    ),
                                    new_work_directory_abs_path: Some(
                                        new_repo.work_directory_abs_path.clone(),
                                    ),
                                    dot_git_abs_path: Some(new_repo.dot_git_abs_path.clone()),
                                    repository_dir_abs_path: Some(
                                        new_repo.repository_dir_abs_path.clone(),
                                    ),
                                    common_dir_abs_path: Some(new_repo.common_dir_abs_path.clone()),
                                });
                            }
                            new_repos.next();
                            old_repos.next();
                        }
                        Ordering::Greater => {
                            changes.push(UpdatedGitRepository {
                                work_directory_id: old_entry_id,
                                old_work_directory_abs_path: Some(
                                    old_repo.work_directory_abs_path.clone(),
                                ),
                                new_work_directory_abs_path: None,
                                dot_git_abs_path: None,
                                repository_dir_abs_path: None,
                                common_dir_abs_path: None,
                            });
                            old_repos.next();
                        }
                    }
                }
                (Some((entry_id, repository)), None) => {
                    changes.push(UpdatedGitRepository {
                        work_directory_id: entry_id,
                        old_work_directory_abs_path: None,
                        new_work_directory_abs_path: Some(
                            repository.work_directory_abs_path.clone(),
                        ),
                        dot_git_abs_path: Some(repository.dot_git_abs_path.clone()),
                        repository_dir_abs_path: Some(repository.repository_dir_abs_path.clone()),
                        common_dir_abs_path: Some(repository.common_dir_abs_path.clone()),
                    });
                    new_repos.next();
                }
                (None, Some((entry_id, repository))) => {
                    changes.push(UpdatedGitRepository {
                        work_directory_id: entry_id,
                        old_work_directory_abs_path: Some(
                            repository.work_directory_abs_path.clone(),
                        ),
                        new_work_directory_abs_path: None,
                        dot_git_abs_path: Some(repository.dot_git_abs_path.clone()),
                        repository_dir_abs_path: None,
                        common_dir_abs_path: None,
                    });
                    old_repos.next();
                }
                (None, None) => break,
            }
        }

        changes.into()
    }
}

impl Deref for Worktree {
    type Target = Snapshot;

    fn deref(&self) -> &Self::Target {
        &self.snapshot
    }
}

impl EventEmitter<WorktreeEvent> for Worktree {}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectEntryId(usize);

impl ProjectEntryId {
    pub const MAX: Self = Self(usize::MAX);

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

impl<'a> Dimension<'a, PathEntrySummary> for ProjectEntryId {
    fn zero((): ()) -> Self {
        Self::default()
    }

    fn add_summary(&mut self, summary: &'a PathEntrySummary, (): ()) {
        *self = summary.max_id;
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
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        struct EntriesById<'a>(&'a SumTree<PathEntry>);
        struct EntriesByPath<'a>(&'a SumTree<Entry>);

        impl fmt::Debug for EntriesByPath<'_> {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter
                    .debug_map()
                    .entries(self.0.iter().map(|entry| (&entry.path, entry.id)))
                    .finish()
            }
        }

        impl fmt::Debug for EntriesById<'_> {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.debug_list().entries(self.0.iter()).finish()
            }
        }

        formatter
            .debug_struct("Snapshot")
            .field("id", &self.id)
            .field("root_name", &self.root_name)
            .field("entries_by_path", &EntriesByPath(&self.entries_by_path))
            .field("entries_by_id", &EntriesById(&self.entries_by_id))
            .finish_non_exhaustive()
    }
}

pub struct LoadedFile {
    pub file: Arc<File>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct File {
    pub worktree: Entity<Worktree>,
    pub path: Arc<RelPath>,
    pub disk_state: DiskState,
    pub entry_id: Option<ProjectEntryId>,
}

impl File {
    pub fn for_entry(entry: &Entry, worktree: Entity<Worktree>) -> Arc<Self> {
        Arc::new(Self {
            worktree,
            path: entry.path.clone(),
            disk_state: if let Some(mtime) = entry.mtime {
                DiskState::Present {
                    mtime,
                    size: entry.size,
                }
            } else {
                DiskState::New
            },
            entry_id: Some(entry.id),
        })
    }

    pub fn from_dyn(file: Option<&Arc<dyn language::File>>) -> Option<&Self> {
        file.and_then(|file| {
            let file: &dyn language::File = file.as_ref();
            let file: &dyn std::any::Any = file;
            file.downcast_ref()
        })
    }

    pub fn worktree_id(&self, cx: &App) -> WorktreeId {
        self.worktree.read(cx).id()
    }

    pub fn project_entry_id(&self) -> Option<ProjectEntryId> {
        match self.disk_state {
            DiskState::Deleted => None,
            _ => self.entry_id,
        }
    }
}

impl language::File for File {
    fn disk_state(&self) -> DiskState {
        self.disk_state
    }

    fn path(&self) -> &Arc<RelPath> {
        &self.path
    }

    fn abs_path(&self, cx: &App) -> PathBuf {
        self.worktree.read(cx).absolutize(self.path.as_ref())
    }

    fn load(&self, cx: &App) -> Task<anyhow::Result<String>> {
        let worktree = self.worktree.read(cx);
        let abs_path = worktree.absolutize(self.path.as_ref());
        let fs = worktree.fs.clone();
        cx.background_spawn(async move { fs.load(abs_path.as_path()).await })
    }

    fn load_bytes(&self, cx: &App) -> Task<anyhow::Result<Vec<u8>>> {
        let worktree = self.worktree.read(cx);
        let abs_path = worktree.absolutize(self.path.as_ref());
        let fs = worktree.fs.clone();
        cx.background_spawn(async move { fs.load_bytes(abs_path.as_path()).await })
    }

    fn full_path(&self, cx: &App) -> PathBuf {
        self.worktree.read(cx).full_path(self.path.as_ref())
    }

    fn path_style(&self, cx: &App) -> PathStyle {
        self.worktree.read(cx).path_style()
    }

    fn file_name<'a>(&'a self, cx: &'a App) -> &'a str {
        self.path
            .file_name()
            .unwrap_or_else(|| self.worktree.read(cx).root_name_str())
    }

    fn worktree_id(&self, cx: &App) -> WorktreeId {
        self.worktree.read(cx).id()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: ProjectEntryId,
    pub kind: EntryKind,
    pub path: Arc<RelPath>,
    pub inode: u64,
    pub mtime: Option<MTime>,
    pub canonical_path: Option<Arc<Path>>,
    pub is_ignored: bool,
    pub is_external: bool,
    pub is_fifo: bool,
    pub size: u64,
    pub is_request: bool,
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
            is_ignored: false,
            is_external: false,
            is_fifo: metadata.is_fifo,
            size: metadata.len,
            is_request: false,
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

impl sum_tree::Item for Entry {
    type Summary = EntrySummary;

    fn summary(&self, (): ()) -> Self::Summary {
        EntrySummary {
            count: 1,
            file_count: usize::from(self.is_file()),
            max_path: self.path.clone(),
        }
    }
}

impl sum_tree::KeyedItem for Entry {
    type Key = PathKey;

    fn key(&self) -> Self::Key {
        PathKey(self.path.clone())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathChange {
    Added,
    Removed,
    Updated,
    AddedOrUpdated,
    Loaded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdatedGitRepository {
    pub work_directory_id: ProjectEntryId,
    pub old_work_directory_abs_path: Option<Arc<Path>>,
    pub new_work_directory_abs_path: Option<Arc<Path>>,
    pub dot_git_abs_path: Option<Arc<Path>>,
    pub repository_dir_abs_path: Option<Arc<Path>>,
    pub common_dir_abs_path: Option<Arc<Path>>,
}

pub type UpdatedEntriesSet = Arc<[(Arc<RelPath>, ProjectEntryId, PathChange)]>;
pub type UpdatedGitRepositoriesSet = Arc<[UpdatedGitRepository]>;

#[derive(Debug, Clone)]
pub struct PathProgress<'a> {
    pub max_path: &'a RelPath,
}

#[derive(Debug, Clone)]
pub struct PathSummary<S> {
    pub max_path: Arc<RelPath>,
    pub item_summary: S,
}

impl<S: sum_tree::Summary> sum_tree::Summary for PathSummary<S> {
    type Context<'a> = S::Context<'a>;

    fn zero(cx: Self::Context<'_>) -> Self {
        Self {
            max_path: Arc::from(RelPath::empty()),
            item_summary: S::zero(cx),
        }
    }

    fn add_summary(&mut self, rhs: &Self, cx: Self::Context<'_>) {
        self.max_path = rhs.max_path.clone();
        self.item_summary.add_summary(&rhs.item_summary, cx);
    }
}

impl<'a, S: sum_tree::Summary> Dimension<'a, PathSummary<S>> for PathProgress<'a> {
    fn zero(_: S::Context<'_>) -> Self {
        Self {
            max_path: RelPath::empty(),
        }
    }

    fn add_summary(&mut self, summary: &'a PathSummary<S>, _: S::Context<'_>) {
        self.max_path = summary.max_path.as_ref();
    }
}

impl<'a> Dimension<'a, PathSummary<GitSummary>> for GitSummary {
    fn zero((): ()) -> Self {
        Self::default()
    }

    fn add_summary(&mut self, summary: &'a PathSummary<GitSummary>, (): ()) {
        *self += summary.item_summary;
    }
}

#[derive(Debug, Clone)]
pub enum WorktreeEvent {
    UpdatedEntries(UpdatedEntriesSet),
    UpdatedGitRepositories(UpdatedGitRepositoriesSet),
    DeletedEntry(ProjectEntryId),
    Deleted,
}

pub trait WorktreeModelHandle {
    #[cfg(feature = "test")]
    fn flush_fs_events<'a>(&self, cx: &'a mut TestAppContext) -> LocalBoxFuture<'a, ()>;
}

impl WorktreeModelHandle for Entity<Worktree> {
    #[cfg(feature = "test")]
    fn flush_fs_events<'a>(&self, cx: &'a mut TestAppContext) -> LocalBoxFuture<'a, ()> {
        let file_name = "fs-event-sentinel";
        let worktree = self.clone();
        let (fs, root_path) = self.read_with(cx, |worktree, _| {
            (worktree.fs().clone(), worktree.abs_path().to_path_buf())
        });

        async move {
            let mut events = cx.events(&worktree);

            fs.write(&root_path.join(file_name), &[])
                .await
                .expect("failed to write filesystem event sentinel");

            let file_exists = || {
                worktree.read_with(cx, |worktree, _| {
                    worktree
                        .entry_for_path(
                            RelPath::unix(file_name)
                                .expect("test file name should be a valid relative path"),
                        )
                        .is_some()
                })
            };

            while !file_exists() {
                futures::select_biased! {
                    _ = events.next() => {}
                    () = cx.background_executor.timer(Duration::from_millis(10)).fuse() => {}
                }
            }

            fs.remove_file(&root_path.join(file_name), RemoveOptions::default())
                .await
                .expect("failed to remove filesystem event sentinel");

            let file_gone = || {
                worktree.read_with(cx, |worktree, _| {
                    worktree
                        .entry_for_path(
                            RelPath::unix(file_name)
                                .expect("test file name should be a valid relative path"),
                        )
                        .is_none()
                })
            };

            while !file_gone() {
                futures::select_biased! {
                    _ = events.next() => {}
                    () = cx.background_executor.timer(Duration::from_millis(10)).fuse() => {}
                }
            }

            cx.update(|cx| worktree.read(cx).scan_complete()).await;
        }
        .boxed_local()
    }
}

#[derive(Clone)]
struct WorktreeSnapshot {
    snapshot: Snapshot,
    ignores_by_parent_abs_path: HashMap<Arc<Path>, (Arc<Gitignore>, bool)>,
    git_repositories: TreeMap<ProjectEntryId, RepositoryEntry>,
    root_file_handle: Option<Arc<dyn FileHandle>>,
}

impl WorktreeSnapshot {
    async fn insert_entry(&mut self, mut entry: Entry, fs: &dyn Fs) -> Entry {
        if entry.is_file() && entry.path.file_name() == Some(GITIGNORE) {
            let abs_path = self.absolutize(&entry.path);
            match build_gitignore(&abs_path, fs).await {
                Ok(ignore) => {
                    self.ignores_by_parent_abs_path.insert(
                        abs_path
                            .parent()
                            .expect("gitignore path should have a parent")
                            .into(),
                        (Arc::new(ignore), true),
                    );
                }
                Err(error) => {
                    log::error!("Failed to load .gitignore file: {error:#}");
                }
            }
        }
        if entry.kind == EntryKind::PendingDir
            && let Some(existing_entry) = self
                .snapshot
                .entries_by_path
                .get(&PathKey(entry.path.clone()), ())
        {
            entry.kind = existing_entry.kind;
        }

        let removed = self
            .snapshot
            .entries_by_path
            .insert_or_replace(entry.clone(), ());
        if let Some(removed) = removed
            && removed.id != entry.id
        {
            self.snapshot.entries_by_id.remove(&removed.id, ());
        }
        self.snapshot
            .entries_by_id
            .insert_or_replace(entry.to_path_entry(), ());

        entry
    }

    async fn ignore_stack_for_abs_path(
        &self,
        abs_path: &Path,
        is_dir: bool,
        fs: &dyn Fs,
    ) -> IgnoreStack {
        let mut new_ignores = Vec::new();
        let mut repo_root = None;
        for (index, ancestor) in abs_path.ancestors().enumerate() {
            if index > 0 {
                if let Some((ignore, _)) = self.ignores_by_parent_abs_path.get(ancestor) {
                    new_ignores.push((ancestor, Some(ignore.clone())));
                } else {
                    new_ignores.push((ancestor, None));
                }
            }

            let metadata = fs.metadata(&ancestor.join(DOT_GIT)).await.ok().flatten();
            if metadata.is_some() {
                repo_root = Some(Arc::from(ancestor));
                break;
            }
        }

        let mut ignore_stack = IgnoreStack::none();
        ignore_stack.repo_root = repo_root;
        for (parent_abs_path, ignore) in new_ignores.into_iter().rev() {
            if ignore_stack.is_abs_path_ignored(parent_abs_path, true) {
                ignore_stack = IgnoreStack::all();
                break;
            } else if let Some(ignore) = ignore {
                ignore_stack =
                    ignore_stack.append(IgnoreKind::Gitignore(parent_abs_path.into()), ignore);
            }
        }

        if ignore_stack.is_abs_path_ignored(abs_path, is_dir) {
            ignore_stack = IgnoreStack::all();
        }

        ignore_stack
    }

    #[cfg(feature = "test")]
    fn check_invariants(&self) {
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
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>(),
            "entries_by_path and entries_by_id are inconsistent"
        );

        let mut file_entries = self.snapshot.files(0);
        for entry in self.snapshot.entries_by_path.cursor::<()>(()) {
            if entry.is_file() {
                assert_eq!(
                    file_entries.next().expect("file entry should exist").inode,
                    entry.inode
                );
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

impl Deref for WorktreeSnapshot {
    type Target = Snapshot;

    fn deref(&self) -> &Self::Target {
        &self.snapshot
    }
}

impl DerefMut for WorktreeSnapshot {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.snapshot
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RepositoryEntry {
    work_directory_id: ProjectEntryId,
    work_directory_abs_path: Arc<Path>,
    git_dir_scan_id: usize,
    dot_git_abs_path: Arc<Path>,
    common_dir_abs_path: Arc<Path>,
    repository_dir_abs_path: Arc<Path>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PathKey(pub Arc<RelPath>);

impl Default for PathKey {
    fn default() -> Self {
        Self(RelPath::empty().into())
    }
}

impl<'a> Dimension<'a, EntrySummary> for PathKey {
    fn zero((): ()) -> Self {
        PathKey::default()
    }

    fn add_summary(&mut self, summary: &'a EntrySummary, (): ()) {
        self.0 = summary.max_path.clone();
    }
}

impl<'a, S: sum_tree::Summary> Dimension<'a, PathSummary<S>> for PathKey {
    fn zero(_: S::Context<'_>) -> Self {
        PathKey::default()
    }

    fn add_summary(&mut self, summary: &'a PathSummary<S>, _: S::Context<'_>) {
        self.0 = summary.max_path.clone();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PathEntry {
    id: ProjectEntryId,
    path: Arc<RelPath>,
}

impl sum_tree::Item for PathEntry {
    type Summary = PathEntrySummary;

    fn summary(&self, (): ()) -> Self::Summary {
        PathEntrySummary { max_id: self.id }
    }
}

impl sum_tree::KeyedItem for PathEntry {
    type Key = ProjectEntryId;

    fn key(&self) -> Self::Key {
        self.id
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
        snapshot: WorktreeSnapshot,
        changes: UpdatedEntriesSet,
        completion_senders: SmallVec<[oneshot::Sender<()>; 1]>,
        scanning: bool,
    },
    RootUpdated {
        new_path: Arc<SanitizedPath>,
    },
    RootDeleted,
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
    is_file_worktree: bool,
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
                            ignore_stack: IgnoreStack::none(),
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
            futures::select_biased! {
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
            let root_path = state.snapshot.abs_path.clone();
            for path in paths {
                for ancestor in path.ancestors() {
                    if let Some(entry) = state.snapshot.entry_for_path(ancestor)
                        && entry.kind == EntryKind::UnloadedDir
                    {
                        let abs_path = if entry.is_external {
                            entry.canonical_path.as_ref().map_or_else(
                                || root_path.as_path().join(ancestor.as_std_path()),
                                |path| path.as_ref().to_path_buf(),
                            )
                        } else {
                            root_path.as_path().join(ancestor.as_std_path())
                        };
                        state
                            .enqueue_scan_dir(
                                abs_path.into(),
                                entry,
                                &scan_job_tx,
                                self.fs.as_ref(),
                            )
                            .await;
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
        let skip_index = |ranges: &mut SmallVec<[Range<usize>; 4]>, index: usize| {
            if let Some(last_range) = ranges.last_mut()
                && last_range.end == index
            {
                last_range.end += 1;
            } else {
                ranges.push(index..index + 1);
            }
        };

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
                        "Root renamed from {} to {}",
                        root_path.as_path().display(),
                        new_path.as_path().display(),
                    );
                    if self
                        .status_updates_tx
                        .unbounded_send(ScanState::RootUpdated { new_path })
                        .is_err()
                    {
                        log::trace!("Worktree root update receiver dropped");
                    }
                } else {
                    log::error!("Root path could not be canonicalized: {error:#}");
                    if self.is_file_worktree {
                        log::info!(
                            "File worktree root {} no longer exists",
                            root_path.as_path().display()
                        );
                        let event_roots = [EventRoot {
                            path: Arc::from(RelPath::empty()),
                            was_rescanned: false,
                        }];
                        {
                            let mut state = self.state.lock().await;
                            state.snapshot.scan_id += 1;
                            state.remove_path_from_snapshot_and_unwatch(
                                RelPath::empty(),
                                self.watcher.as_ref(),
                                false,
                            );
                            state.snapshot.completed_scan_id = state.snapshot.scan_id;
                            for (_, removed_entry) in std::mem::take(&mut state.removed_entries) {
                                state.scanned_dirs.remove(&removed_entry.id);
                            }
                        }
                        self.send_status_update(false, SmallVec::new(), &event_roots)
                            .await;
                        if self
                            .status_updates_tx
                            .unbounded_send(ScanState::RootDeleted)
                            .is_err()
                        {
                            log::trace!("Worktree root delete receiver dropped");
                        }
                    }
                }
                return;
            }
        };

        let skipped_file_names_in_dot_git =
            [COMMIT_MESSAGE, FETCH_HEAD, ORIG_HEAD, BISECT_LOG, GC_PID];
        let skipped_dirs_in_dot_git = [
            FSMONITOR_DAEMON,
            LFS_DIR,
            OBJECTS_DIR,
            HOOKS_DIR,
            REBASE_MERGE_DIR,
            REBASE_APPLY_DIR,
            SEQUENCER_DIR,
        ];

        let mut dot_git_abs_paths = Vec::new();
        {
            let mut ranges_to_drop = SmallVec::<[Range<usize>; 4]>::new();

            for (index, event) in events.iter().enumerate() {
                let abs_path = SanitizedPath::new(&event.path);
                let mut dot_git_paths = None;

                for ancestor in abs_path.as_path().ancestors() {
                    if is_dot_git(ancestor, self.fs.as_ref()).await {
                        let path_in_git_dir = abs_path
                            .as_path()
                            .strip_prefix(ancestor)
                            .expect("stripping off the ancestor");
                        dot_git_paths = Some((ancestor.to_owned(), path_in_git_dir.to_owned()));
                        break;
                    }
                }

                if let Some((dot_git_abs_path, path_in_git_dir)) = dot_git_paths {
                    let is_ignored = skipped_file_names_in_dot_git.iter().any(|skipped| {
                        path_in_git_dir
                            .file_name()
                            .is_some_and(|file_name| file_name == OsStr::new(skipped))
                    }) || (path_in_git_dir.starts_with(LOGS_DIR)
                        && path_in_git_dir != Path::new(LOGS_REF_STASH))
                        || (path_in_git_dir.starts_with(INFO_DIR)
                            && path_in_git_dir != Path::new(REPO_EXCLUDE))
                        || skipped_dirs_in_dot_git.iter().any(|skipped_git_subdir| {
                            path_in_git_dir.starts_with(skipped_git_subdir)
                        })
                        || path_in_git_dir
                            .extension()
                            .is_some_and(|extension| extension == "lock")
                        || (path_in_git_dir.components().count() == 1
                            && path_in_git_dir
                                .extension()
                                .is_some_and(|extension| extension == "new" || extension == "tmp"));
                    let is_dot_git = path_in_git_dir == Path::new("")
                        && matches!(event.kind, Some(PathEventKind::Changed))
                        && matches!(
                            self.fs.metadata(&dot_git_abs_path).await,
                            Ok(Some(metadata)) if metadata.is_dir
                        );

                    if is_ignored {
                        log::debug!(
                            "Ignoring event {} as it's in the .git directory among skipped files or directories",
                            abs_path.as_path().display()
                        );
                        skip_index(&mut ranges_to_drop, index);
                        continue;
                    }
                    if is_dot_git {
                        log::debug!(
                            "Ignoring event {} for .git directory itself (kind: {:?})",
                            abs_path.as_path().display(),
                            event.kind
                        );
                        skip_index(&mut ranges_to_drop, index);
                        continue;
                    }

                    if !dot_git_abs_paths.contains(&dot_git_abs_path) {
                        log::debug!(
                            "Detected update within Git repo at {}: {}",
                            dot_git_abs_path.display(),
                            abs_path.as_path().display()
                        );
                        dot_git_abs_paths.push(dot_git_abs_path);
                    }
                }
            }

            for range_to_drop in ranges_to_drop.into_iter().rev() {
                events.drain(range_to_drop);
            }
        }

        events.sort_unstable_by(|left, right| left.path.cmp(&right.path));
        events.dedup_by(|left, right| {
            if left.path == right.path || left.path.starts_with(&right.path) {
                if matches!(left.kind, Some(PathEventKind::Rescan)) {
                    right.kind = left.kind;
                }
                true
            } else {
                false
            }
        });

        let mut relative_paths = Vec::with_capacity(events.len());
        {
            let snapshot = &self.state.lock().await.snapshot;
            let mut ranges_to_drop = SmallVec::<[Range<usize>; 4]>::new();

            for (index, event) in events.iter().enumerate() {
                let abs_path = SanitizedPath::new(&event.path);
                let relative_path = if let Ok(path) = abs_path
                    .as_path()
                    .strip_prefix(root_canonical_path.as_path())
                    && let Ok(path) = RelPath::new(path, PathStyle::local())
                {
                    path
                } else {
                    skip_index(&mut ranges_to_drop, index);
                    continue;
                };

                if abs_path.as_path().file_name() == Some(OsStr::new(GITIGNORE)) {
                    for (_, repository) in snapshot.git_repositories.iter() {
                        let Some(work_directory_entry) =
                            snapshot.entry_for_id(repository.work_directory_id)
                        else {
                            continue;
                        };

                        if relative_path
                            .as_ref()
                            .starts_with(work_directory_entry.path.as_ref())
                            && !dot_git_abs_paths.iter().any(|dot_git_abs_path| {
                                dot_git_abs_path == repository.common_dir_abs_path.as_ref()
                            })
                        {
                            dot_git_abs_paths.push(repository.common_dir_abs_path.to_path_buf());
                        }
                    }
                }

                let parent_dir_is_loaded = relative_path.parent().is_none_or(|parent| {
                    snapshot
                        .entry_for_path(parent)
                        .is_some_and(|entry| entry.kind == EntryKind::Dir)
                });
                if !parent_dir_is_loaded {
                    log::debug!("Ignoring event {relative_path:?} within unloaded directory");
                    skip_index(&mut ranges_to_drop, index);
                    continue;
                }

                if is_path_excluded(relative_path.as_ref()) {
                    skip_index(&mut ranges_to_drop, index);
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

        if relative_paths.is_empty() && dot_git_abs_paths.is_empty() {
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
        if !dot_git_abs_paths.is_empty() {
            self.update_git_repositories(dot_git_abs_paths).await;
        }
        {
            let ignores_to_update = self.ignores_needing_update().await;
            let ignores_to_update = self.order_ignores(ignores_to_update).await;
            let snapshot = self.state.lock().await.snapshot.clone();
            self.update_ignore_statuses_for_paths(scan_job_tx, snapshot, ignores_to_update)
                .await;
            self.scan_dirs(false, scan_job_rx).await;
        }

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

    async fn update_git_repositories(&self, dot_git_paths: Vec<PathBuf>) {
        log::trace!("Reloading repositories: {dot_git_paths:?}");
        let mut state = self.state.lock().await;
        let scan_id = state.snapshot.scan_id;

        for dot_git_dir in dot_git_paths {
            let sanitized_dot_git_dir = SanitizedPath::new(&dot_git_dir);
            let existing_repository_entry =
                state
                    .snapshot
                    .git_repositories
                    .iter()
                    .find_map(|(_, repository)| {
                        if SanitizedPath::new(repository.common_dir_abs_path.as_ref())
                            == sanitized_dot_git_dir
                            || SanitizedPath::new(repository.repository_dir_abs_path.as_ref())
                                == sanitized_dot_git_dir
                            || SanitizedPath::new(repository.dot_git_abs_path.as_ref())
                                == sanitized_dot_git_dir
                        {
                            Some(repository.clone())
                        } else {
                            None
                        }
                    });

            match existing_repository_entry {
                None => {
                    let Ok(relative_path) = dot_git_dir.strip_prefix(state.snapshot.abs_path())
                    else {
                        continue;
                    };
                    let Some(relative_path) =
                        RelPath::new(relative_path, PathStyle::local()).log_err()
                    else {
                        continue;
                    };
                    state
                        .insert_git_repository(
                            relative_path.into_owned().into(),
                            self.fs.as_ref(),
                            self.watcher.as_ref(),
                        )
                        .await;
                }
                Some(mut repository) => {
                    repository.git_dir_scan_id = scan_id;
                    state
                        .snapshot
                        .git_repositories
                        .insert(repository.work_directory_id, repository);
                }
            }
        }

        let repositories = state
            .snapshot
            .git_repositories
            .iter()
            .map(|(work_directory_id, repository)| {
                (*work_directory_id, repository.dot_git_abs_path.clone())
            })
            .collect::<Vec<_>>();
        let mut ids_to_preserve = HashSet::default();
        for (work_directory_id, dot_git_abs_path) in repositories {
            let dot_git_present = !matches!(self.fs.metadata(&dot_git_abs_path).await, Ok(None));
            if dot_git_present {
                ids_to_preserve.insert(work_directory_id);
            }
        }

        state
            .snapshot
            .git_repositories
            .retain(|work_directory_id, _| ids_to_preserve.contains(work_directory_id));
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
                    ScanState::Started | ScanState::RootUpdated { .. } | ScanState::RootDeleted => {
                    }
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
                            futures::select_biased! {
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
                                            log::error!(
                                                "Error scanning directory {}: {error:#}",
                                                job.abs_path.display()
                                            );
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
        if is_path_excluded(job.path.as_ref()) {
            log::error!("Skipping excluded directory {:?}", job.path);
            return Ok(());
        }

        let root_abs_path = self.state.lock().await.snapshot.abs_path().clone();
        log::trace!("Scanning directory {:?}", job.path);

        let next_entry_id = self.next_entry_id.clone();
        let mut ignore_stack = job.ignore_stack.clone();
        let mut new_ignore = None;
        let mut root_canonical_path = None;
        let mut new_entries: Vec<Entry> = Vec::new();
        let mut new_jobs: Vec<Option<ScanJob>> = Vec::new();
        let mut child_paths = self
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
        swap_to_front(&mut child_paths, GITIGNORE);
        swap_to_front(&mut child_paths, DOT_GIT);

        if let Some(path) = child_paths.first()
            && path.ends_with(DOT_GIT)
        {
            ignore_stack.repo_root = Some(job.abs_path.clone());
        }

        for child_abs_path in child_paths {
            let child_abs_path: Arc<Path> = child_abs_path.into();
            let Some(child_name) = child_abs_path.file_name() else {
                continue;
            };
            let Some(child_path) = child_name
                .to_str()
                .and_then(|name| Some(job.path.join(RelPath::unix(name).ok()?)))
            else {
                continue;
            };

            if child_name == GITIGNORE {
                match build_gitignore(&child_abs_path, self.fs.as_ref()).await {
                    Ok(ignore) => {
                        let ignore = Arc::new(ignore);
                        ignore_stack = ignore_stack
                            .append(IgnoreKind::Gitignore(job.abs_path.clone()), ignore.clone());
                        new_ignore = Some(ignore);
                    }
                    Err(error) => {
                        log::error!("Failed to load .gitignore file: {error:#}");
                    }
                }
            }

            let child_metadata = match self.fs.metadata(child_abs_path.as_ref()).await {
                Ok(Some(metadata)) => metadata,
                Ok(None) => continue,
                Err(error) => {
                    log::error!("Failed to stat {}: {error:#}", child_abs_path.display());
                    continue;
                }
            };

            if child_name == DOT_GIT && !child_metadata.is_fifo {
                self.state
                    .lock()
                    .await
                    .insert_git_repository(
                        child_path.clone(),
                        self.fs.as_ref(),
                        self.watcher.as_ref(),
                    )
                    .await;
            }

            if is_path_excluded(child_path.as_ref()) {
                self.state
                    .lock()
                    .await
                    .remove_path_from_snapshot_and_unwatch(
                        child_path.as_ref(),
                        self.watcher.as_ref(),
                        true,
                    );
                continue;
            }

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
                child_entry.is_ignored = ignore_stack.is_abs_path_ignored(&child_abs_path, true);

                if job.ancestor_inodes.contains(&child_entry.inode) {
                    new_jobs.push(None);
                } else {
                    let mut ancestor_inodes = job.ancestor_inodes.clone();
                    ancestor_inodes.insert(child_entry.inode);
                    new_jobs.push(Some(ScanJob {
                        abs_path: child_abs_path.clone(),
                        path: child_path,
                        ignore_stack: if child_entry.is_ignored {
                            IgnoreStack::all()
                        } else {
                            ignore_stack.clone()
                        },
                        scan_queue: job.scan_queue.clone(),
                        ancestor_inodes,
                        is_external: child_entry.is_external,
                    }));
                }
            } else {
                if child_metadata.is_fifo {
                    continue;
                }

                child_entry.is_ignored = ignore_stack.is_abs_path_ignored(&child_abs_path, false);
                child_entry.is_request = child_abs_path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"));
            }

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
            state.populate_dir(job.path.clone(), new_entries, new_ignore);
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

        let mut state = self.state.lock().await;
        let doing_recursive_update = scan_queue_tx.is_some();

        let mut paths_to_process = Vec::with_capacity(relative_paths.len());
        for (path, metadata) in relative_paths.iter().zip(metadata.iter()) {
            let path_is_excluded = is_path_excluded(path.as_ref());
            let path_was_removed = matches!(metadata, Ok(None));
            let prune_repositories = path_was_removed && !path_is_excluded;
            let removed_descendant_paths =
                if path_was_removed || doing_recursive_update || path_is_excluded {
                    state.remove_path_from_snapshot(path, prune_repositories)
                } else {
                    Vec::new()
                };
            paths_to_process.push((path, metadata, removed_descendant_paths));
        }

        for (path, metadata, mut removed_descendant_abs_paths) in paths_to_process {
            let abs_path: Arc<Path> = root_abs_path.as_path().join(path.as_std_path()).into();
            if is_path_excluded(path.as_ref()) {
                state.unwatch_path(self.watcher.as_ref(), removed_descendant_abs_paths, true);
                continue;
            }

            match metadata {
                Ok(Some((metadata, canonical_path))) => {
                    if metadata.is_fifo {
                        if !doing_recursive_update {
                            removed_descendant_abs_paths =
                                state.remove_path_from_snapshot(path, true);
                        }
                        state.unwatch_path(
                            self.watcher.as_ref(),
                            removed_descendant_abs_paths,
                            false,
                        );
                        continue;
                    }

                    let ignore_stack = state
                        .snapshot
                        .ignore_stack_for_abs_path(&abs_path, metadata.is_dir, self.fs.as_ref())
                        .await;
                    let is_external = !canonical_path.starts_with(root_canonical_path);
                    let entry_id = state.entry_id_for(self.next_entry_id.as_ref(), path, metadata);
                    let mut fs_entry = Entry::new(
                        path.clone(),
                        metadata,
                        entry_id,
                        metadata
                            .is_symlink
                            .then(|| canonical_path.as_path().to_path_buf().into()),
                    );

                    let is_dir = fs_entry.is_dir();
                    fs_entry.is_ignored = ignore_stack.is_abs_path_ignored(&abs_path, is_dir);
                    fs_entry.is_external = is_external;
                    if !is_dir {
                        fs_entry.is_request = abs_path
                            .extension()
                            .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"));
                    }

                    if let (Some(scan_queue_tx), true) = (&scan_queue_tx, is_dir) {
                        if state.should_scan_directory(&fs_entry) {
                            state
                                .enqueue_scan_dir(
                                    abs_path.clone(),
                                    &fs_entry,
                                    scan_queue_tx,
                                    self.fs.as_ref(),
                                )
                                .await;
                        } else {
                            fs_entry.kind = EntryKind::UnloadedDir;
                        }
                    }

                    state
                        .insert_entry(fs_entry, self.fs.as_ref(), self.watcher.as_ref())
                        .await;
                }
                Ok(None) => {
                    state.unwatch_path(self.watcher.as_ref(), removed_descendant_abs_paths, false);
                }
                Err(error) => {
                    log::error!("Failed to reload {}: {error:#}", abs_path.display());
                    state.unwatch_path(self.watcher.as_ref(), removed_descendant_abs_paths, false);
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

    async fn update_ignore_statuses_for_paths(
        &self,
        scan_job_tx: channel::Sender<ScanJob>,
        prev_snapshot: WorktreeSnapshot,
        ignores_to_update: Vec<(Arc<Path>, IgnoreStack)>,
    ) {
        let (ignore_queue_tx, ignore_queue_rx) = channel::unbounded();
        for (parent_abs_path, ignore_stack) in ignores_to_update {
            ignore_queue_tx
                .send_blocking(UpdateIgnoreStatusJob {
                    abs_path: parent_abs_path,
                    ignore_stack,
                    ignore_queue: ignore_queue_tx.clone(),
                    scan_queue: scan_job_tx.clone(),
                })
                .expect("ignore status job channel should be open");
        }
        drop(ignore_queue_tx);

        self.executor
            .scoped(|scope| {
                for _ in 0..self.executor.num_cpus() {
                    scope.spawn(async {
                        loop {
                            futures::select_biased! {
                                request = self.next_scan_request().fuse() => {
                                    let Ok(request) = request else {
                                        break;
                                    };
                                    if !self.process_scan_request(request, true).await {
                                        return;
                                    }
                                }
                                job = ignore_queue_rx.recv().fuse() => {
                                    let Ok(job) = job else {
                                        break;
                                    };
                                    self.update_ignore_status(job, &prev_snapshot).await;
                                }
                            }
                        }
                    });
                }
            })
            .await;
    }

    async fn ignores_needing_update(&self) -> Vec<Arc<Path>> {
        let mut ignores_to_update = Vec::new();
        let snapshot = &mut self.state.lock().await.snapshot;
        let abs_path = snapshot.abs_path.clone();

        snapshot
            .ignores_by_parent_abs_path
            .retain(|parent_abs_path, (_, needs_update)| {
                if let Ok(parent_path) = parent_abs_path.strip_prefix(abs_path.as_path())
                    && let Some(parent_path) =
                        RelPath::new(parent_path, PathStyle::local()).log_err()
                {
                    if *needs_update {
                        *needs_update = false;
                        if snapshot.snapshot.entry_for_path(&parent_path).is_some() {
                            ignores_to_update.push(parent_abs_path.clone());
                        }
                    }

                    let ignore_path = parent_path.join(
                        RelPath::unix(GITIGNORE)
                            .expect("gitignore path should be a valid relative path"),
                    );
                    if snapshot.snapshot.entry_for_path(&ignore_path).is_none() {
                        return false;
                    }
                }
                true
            });

        ignores_to_update
    }

    async fn order_ignores(&self, mut ignores: Vec<Arc<Path>>) -> Vec<(Arc<Path>, IgnoreStack)> {
        let snapshot = self.state.lock().await.snapshot.clone();
        ignores.sort_unstable();
        let mut ignores_to_update = ignores.into_iter().peekable();

        let mut result = Vec::new();
        while let Some(parent_abs_path) = ignores_to_update.next() {
            while ignores_to_update
                .peek()
                .is_some_and(|path| path.starts_with(&parent_abs_path))
            {
                ignores_to_update.next();
            }
            let ignore_stack = snapshot
                .ignore_stack_for_abs_path(&parent_abs_path, true, self.fs.as_ref())
                .await;
            result.push((parent_abs_path, ignore_stack));
        }

        result
    }

    async fn update_ignore_status(&self, job: UpdateIgnoreStatusJob, snapshot: &WorktreeSnapshot) {
        log::trace!("Updating ignore status {}", job.abs_path.display());

        let mut ignore_stack = job.ignore_stack;
        if let Some((ignore, _)) = snapshot.ignores_by_parent_abs_path.get(&job.abs_path) {
            ignore_stack =
                ignore_stack.append(IgnoreKind::Gitignore(job.abs_path.clone()), ignore.clone());
        }

        let mut entries_by_path_edits = Vec::new();
        let Some(path) = job
            .abs_path
            .strip_prefix(snapshot.abs_path.as_path())
            .with_context(|| {
                format!(
                    "Failed to strip prefix '{}' from path '{}'",
                    snapshot.abs_path.as_path().display(),
                    job.abs_path.display()
                )
            })
            .log_err()
        else {
            return;
        };

        let Some(path) = RelPath::new(path, PathStyle::local()).log_err() else {
            return;
        };

        if let Ok(Some(metadata)) = self.fs.metadata(&job.abs_path.join(DOT_GIT)).await
            && metadata.is_dir
        {
            ignore_stack.repo_root = Some(job.abs_path.clone());
        }

        for mut entry in snapshot.child_entries(&path).cloned() {
            let was_ignored = entry.is_ignored;
            let abs_path: Arc<Path> = snapshot.absolutize(&entry.path).into();
            entry.is_ignored = ignore_stack.is_abs_path_ignored(&abs_path, entry.is_dir());

            if entry.is_dir() {
                let child_ignore_stack = if entry.is_ignored {
                    IgnoreStack::all()
                } else {
                    ignore_stack.clone()
                };

                if was_ignored && !entry.is_ignored && entry.kind.is_unloaded() {
                    let state = self.state.lock().await;
                    if state.should_scan_directory(&entry) {
                        state
                            .enqueue_scan_dir(
                                abs_path.clone(),
                                &entry,
                                &job.scan_queue,
                                self.fs.as_ref(),
                            )
                            .await;
                    }
                }

                job.ignore_queue
                    .send(UpdateIgnoreStatusJob {
                        abs_path: abs_path.clone(),
                        ignore_stack: child_ignore_stack,
                        ignore_queue: job.ignore_queue.clone(),
                        scan_queue: job.scan_queue.clone(),
                    })
                    .await
                    .expect("ignore status job channel is unbounded");
            }

            if entry.is_ignored != was_ignored {
                entries_by_path_edits.push(Edit::Insert(entry));
            }
        }

        let state = &mut self.state.lock().await;
        for edit in &entries_by_path_edits {
            if let Edit::Insert(entry) = edit
                && let Err(index) = state.changed_paths.binary_search(&entry.path)
            {
                state.changed_paths.insert(index, entry.path.clone());
            }
        }

        state
            .snapshot
            .entries_by_path
            .edit(entries_by_path_edits, ());
    }

    async fn progress_timer(&self, running: bool) {
        if !running {
            return futures::future::pending().await;
        }

        self.executor.timer(FS_WATCH_LATENCY).await;
    }
}

struct BackgroundScannerState {
    snapshot: WorktreeSnapshot,
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
        (self.scanning_enabled && !entry.is_external && !entry.is_ignored)
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

    async fn enqueue_scan_dir(
        &self,
        abs_path: Arc<Path>,
        entry: &Entry,
        scan_job_tx: &channel::Sender<ScanJob>,
        fs: &dyn Fs,
    ) {
        let path = entry.path.clone();
        let ignore_stack = self
            .snapshot
            .ignore_stack_for_abs_path(&abs_path, true, fs)
            .await;
        let mut ancestor_inodes = self.snapshot.ancestor_inodes_for_path(path.as_ref());

        if !ancestor_inodes.contains(&entry.inode) {
            ancestor_inodes.insert(entry.inode);
            scan_job_tx
                .try_send(ScanJob {
                    abs_path,
                    path,
                    ignore_stack,
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
        ignore: Option<Arc<Gitignore>>,
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
            EntryKind::File => return,
        }

        if let Some(ignore) = ignore {
            let abs_parent_path: Arc<Path> = self.snapshot.absolutize(&parent_path).into();
            self.snapshot
                .ignores_by_parent_abs_path
                .insert(abs_parent_path, (ignore, false));
        }

        let parent_entry_id = parent_entry.id;
        self.scanned_dirs.insert(parent_entry_id);

        let mut entries_by_path_edits = vec![Edit::Insert(parent_entry)];
        let mut entries_by_id_edits = Vec::new();
        for entry in entries {
            entries_by_id_edits.push(Edit::Insert(entry.to_path_entry()));
            entries_by_path_edits.push(Edit::Insert(entry));
        }

        self.snapshot
            .entries_by_path
            .edit(entries_by_path_edits, ());
        self.snapshot.entries_by_id.edit(entries_by_id_edits, ());

        if let Err(index) = self.changed_paths.binary_search(&parent_path) {
            self.changed_paths.insert(index, parent_path);
        }

        #[cfg(feature = "test")]
        self.snapshot.check_invariants();
    }

    async fn insert_entry(&mut self, entry: Entry, fs: &dyn Fs, watcher: &dyn FsWatcher) -> Entry {
        self.removed_entries.remove(&entry.inode);
        let entry = self.snapshot.insert_entry(entry, fs).await;
        if entry.path.file_name() == Some(DOT_GIT) {
            self.insert_git_repository(entry.path.clone(), fs, watcher)
                .await;
        }

        #[cfg(feature = "test")]
        self.snapshot.check_invariants();

        entry
    }

    async fn insert_git_repository(
        &mut self,
        dot_git_path: Arc<RelPath>,
        fs: &dyn Fs,
        watcher: &dyn FsWatcher,
    ) {
        let Some(work_directory_path) = dot_git_path.parent() else {
            log::debug!("Not building Git repository for the worktree itself: {dot_git_path:?}");
            return;
        };

        if work_directory_path
            .components()
            .any(|component| component == DOT_GIT)
        {
            log::debug!(
                "Not building Git repository for nested `.git` directory: {dot_git_path:?}"
            );
            return;
        }

        let Some(work_directory_entry) = self.snapshot.entry_for_path(work_directory_path) else {
            log::debug!("Working directory {work_directory_path:?} not indexed");
            return;
        };
        let work_directory_id = work_directory_entry.id;

        let dot_git_abs_path: Arc<Path> = self.snapshot.absolutize(&dot_git_path).into();
        let work_directory_abs_path: Arc<Path> =
            self.snapshot.absolutize(work_directory_path).into();
        let (repository_dir_abs_path, common_dir_abs_path) =
            discover_git_paths(&dot_git_abs_path, fs).await;

        watcher.add(common_dir_abs_path.as_ref()).log_err();
        watcher.add(repository_dir_abs_path.as_ref()).log_err();

        let reftable_path = common_dir_abs_path.join("reftable");
        if let Ok(Some(metadata)) = fs.metadata(&reftable_path).await
            && metadata.is_dir
        {
            watcher.add(&reftable_path).log_err();
        }

        self.snapshot.git_repositories.insert(
            work_directory_id,
            RepositoryEntry {
                work_directory_id,
                work_directory_abs_path,
                git_dir_scan_id: 0,
                dot_git_abs_path,
                common_dir_abs_path,
                repository_dir_abs_path,
            },
        );
    }

    fn remove_path_from_snapshot_and_unwatch(
        &mut self,
        path: &RelPath,
        watcher: &dyn FsWatcher,
        preserve_repository_watches: bool,
    ) {
        let removed_descendant_abs_paths =
            self.remove_path_from_snapshot(path, !preserve_repository_watches);
        self.unwatch_path(
            watcher,
            removed_descendant_abs_paths,
            preserve_repository_watches,
        );
    }

    fn unwatch_path(
        &mut self,
        watcher: &dyn FsWatcher,
        removed_descendant_abs_paths: Vec<PathBuf>,
        preserve_repository_watches: bool,
    ) {
        let mut repository_watches_to_preserve = HashSet::<Arc<Path>>::default();
        if preserve_repository_watches {
            for repository in self.snapshot.git_repositories.values() {
                repository_watches_to_preserve.insert(repository.common_dir_abs_path.clone());
                repository_watches_to_preserve.insert(repository.repository_dir_abs_path.clone());
            }
        }

        for removed_dir_abs_path in removed_descendant_abs_paths {
            if repository_watches_to_preserve.contains(removed_dir_abs_path.as_path()) {
                continue;
            }
            watcher.remove(removed_dir_abs_path.as_path()).log_err();
        }
    }

    fn remove_path_from_snapshot(
        &mut self,
        path: &RelPath,
        prune_repositories: bool,
    ) -> Vec<PathBuf> {
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

            if entry.path.file_name() == Some(GITIGNORE)
                && let Some(parent) = entry.path.parent()
            {
                let abs_parent_path = self.snapshot.absolutize(parent);
                if let Some((_, needs_update)) = self
                    .snapshot
                    .ignores_by_parent_abs_path
                    .get_mut(abs_parent_path.as_path())
                {
                    *needs_update = true;
                }
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
        if prune_repositories {
            self.snapshot
                .git_repositories
                .retain(|work_directory_id, _| {
                    removed_ids.binary_search(work_directory_id).is_err()
                });
        }

        #[cfg(feature = "test")]
        self.snapshot.check_invariants();

        removed_dir_abs_paths
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EventRoot {
    path: Arc<RelPath>,
    was_rescanned: bool,
}

struct ScanJob {
    abs_path: Arc<Path>,
    path: Arc<RelPath>,
    ignore_stack: IgnoreStack,
    scan_queue: channel::Sender<ScanJob>,
    ancestor_inodes: HashSet<u64>,
    is_external: bool,
}

struct UpdateIgnoreStatusJob {
    abs_path: Arc<Path>,
    ignore_stack: IgnoreStack,
    ignore_queue: channel::Sender<UpdateIgnoreStatusJob>,
    scan_queue: channel::Sender<ScanJob>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone)]
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

impl<'a, S: sum_tree::Summary> Dimension<'a, PathSummary<S>> for TraversalProgress<'a> {
    fn zero(_: S::Context<'_>) -> Self {
        Self::default()
    }

    fn add_summary(&mut self, summary: &'a PathSummary<S>, _: S::Context<'_>) {
        self.max_path = summary.max_path.as_ref();
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
pub enum PathTarget<'a> {
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

impl<'a, S: sum_tree::Summary> SeekTarget<'a, PathSummary<S>, PathProgress<'a>> for PathTarget<'_> {
    fn cmp(&self, cursor_location: &PathProgress<'a>, _: S::Context<'_>) -> Ordering {
        self.cmp_path(cursor_location.max_path)
    }
}

impl<'a, S: sum_tree::Summary> SeekTarget<'a, PathSummary<S>, TraversalProgress<'a>>
    for PathTarget<'_>
{
    fn cmp(&self, cursor_location: &TraversalProgress<'a>, _: S::Context<'_>) -> Ordering {
        self.cmp_path(cursor_location.max_path)
    }
}

impl<'a> SeekTarget<'a, PathSummary<GitSummary>, Dimensions<TraversalProgress<'a>, GitSummary>>
    for PathTarget<'_>
{
    fn cmp(
        &self,
        cursor_location: &Dimensions<TraversalProgress<'a>, GitSummary>,
        (): (),
    ) -> Ordering {
        self.cmp_path(cursor_location.0.max_path)
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

#[derive(Clone, Copy)]
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

async fn build_gitignore(abs_path: &Path, fs: &dyn Fs) -> anyhow::Result<Gitignore> {
    let parent = abs_path.parent().unwrap_or_else(|| Path::new("/"));
    build_gitignore_with_root(abs_path, parent, fs).await
}

async fn build_gitignore_with_root(
    abs_path: &Path,
    root: &Path,
    fs: &dyn Fs,
) -> anyhow::Result<Gitignore> {
    let contents = fs
        .load(abs_path)
        .await
        .with_context(|| format!("failed to load gitignore file at {}", abs_path.display()))?;
    let mut builder = GitignoreBuilder::new(root);
    for line in contents.lines() {
        builder.add_line(Some(abs_path.into()), line)?;
    }
    Ok(builder.build()?)
}

async fn is_dot_git(path: &Path, fs: &dyn Fs) -> bool {
    if let Some(file_name) = path.file_name()
        && file_name == DOT_GIT
    {
        return true;
    }

    let head_metadata = fs.metadata(&path.join("HEAD")).await;
    if !matches!(head_metadata, Ok(Some(_))) {
        return false;
    }

    let config_metadata = fs.metadata(&path.join("config")).await;
    matches!(config_metadata, Ok(Some(_)))
}

fn is_path_excluded(path: &RelPath) -> bool {
    path.components().any(|component| component == DOT_GIT)
}

fn parse_gitfile(content: &str) -> anyhow::Result<&Path> {
    let path = content
        .strip_prefix("gitdir:")
        .with_context(|| format!("parsing gitfile content {content:?}"))?;
    Ok(Path::new(path.trim()))
}

fn resolve_gitfile_path(dot_git_abs_path: &Path, gitfile_path: &Path) -> PathBuf {
    if gitfile_path.is_absolute() {
        gitfile_path.into()
    } else {
        dot_git_abs_path
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(gitfile_path)
    }
}

fn resolve_commondir_path(repository_dir_abs_path: &Path, commondir_path: &str) -> PathBuf {
    let commondir_path = Path::new(commondir_path.trim());
    if commondir_path.is_absolute() {
        commondir_path.into()
    } else {
        repository_dir_abs_path.join(commondir_path)
    }
}

async fn discover_git_paths(dot_git_abs_path: &Arc<Path>, fs: &dyn Fs) -> (Arc<Path>, Arc<Path>) {
    let mut repository_dir_abs_path = dot_git_abs_path.clone();
    let mut common_dir_abs_path = dot_git_abs_path.clone();

    if let Some(path) = fs
        .load(dot_git_abs_path)
        .await
        .ok()
        .as_ref()
        .and_then(|contents| parse_gitfile(contents).log_err())
    {
        let path = resolve_gitfile_path(dot_git_abs_path, path);
        if let Some(path) = fs.canonicalize(&path).await.log_err() {
            repository_dir_abs_path = path.as_path().into();
            common_dir_abs_path = repository_dir_abs_path.clone();

            if let Some(commondir_contents) = fs.load(&path.join("commondir")).await.ok()
                && let Some(commondir_path) = fs
                    .canonicalize(&resolve_commondir_path(&path, &commondir_contents))
                    .await
                    .log_err()
            {
                common_dir_abs_path = commondir_path.as_path().into();
            }
        }
    }

    (repository_dir_abs_path, common_dir_abs_path)
}

fn swap_to_front(child_paths: &mut Vec<PathBuf>, file: &str) {
    let position = child_paths
        .iter()
        .position(|path| path.file_name() == Some(OsStr::new(file)));
    if let Some(position) = position {
        let path = child_paths.remove(position);
        child_paths.insert(0, path);
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
