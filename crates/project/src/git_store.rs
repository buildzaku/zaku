pub mod git_traversal;

use anyhow::Context as AnyhowContext;
use futures::{FutureExt, StreamExt, channel::mpsc, future, stream::FuturesOrdered};
use gpui::{
    App, AppContext, AsyncApp, Context, Entity, EventEmitter, SharedString, Subscription, Task,
};
use std::{ops, path::Path, sync::Arc};
use sum_tree::{Bias, Edit, SumTree};

use collections::{BTreeSet, HashMap, HashSet, VecDeque};
use git::{
    repository::{
        Branch, BranchesScanResult, CommitDetails, GitRepository, RepoPath, SystemGitRepository,
    },
    status::{FileStatus, GitStatus, GitSummary},
};
use path::{PathStyle, RelPath};
use util::ResultExt;
use worktree::{
    PathChange, PathKey, PathProgress, PathSummary, PathTarget, ProjectEntryId,
    UpdatedGitRepositoriesSet, UpdatedGitRepository, Worktree, WorktreeId,
};

use crate::{
    ProjectPath,
    worktree_store::{WorktreeStore, WorktreeStoreEvent},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusEntry {
    pub repo_path: RepoPath,
    pub status: FileStatus,
}

impl sum_tree::Item for StatusEntry {
    type Summary = PathSummary<GitSummary>;

    fn summary(&self, (): ()) -> Self::Summary {
        PathSummary {
            max_path: self.repo_path.as_ref().clone(),
            item_summary: self.status.summary(),
        }
    }
}

impl sum_tree::KeyedItem for StatusEntry {
    type Key = PathKey;

    fn key(&self) -> Self::Key {
        PathKey(self.repo_path.as_ref().clone())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RepositoryId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositorySnapshot {
    pub id: RepositoryId,
    pub statuses_by_path: SumTree<StatusEntry>,
    pub work_directory_abs_path: Arc<Path>,
    pub dot_git_abs_path: Arc<Path>,
    pub repository_dir_abs_path: Arc<Path>,
    pub common_dir_abs_path: Arc<Path>,
    pub path_style: PathStyle,
    pub branch: Option<Branch>,
    pub branch_list: Arc<[Branch]>,
    pub branch_list_error: Option<SharedString>,
    pub head_commit: Option<CommitDetails>,
    pub scan_id: u64,
}

impl RepositorySnapshot {
    fn empty(
        id: RepositoryId,
        work_directory_abs_path: Arc<Path>,
        repository_dir_abs_path: Option<Arc<Path>>,
        dot_git_abs_path: Option<Arc<Path>>,
        common_dir_abs_path: Option<Arc<Path>>,
        path_style: PathStyle,
    ) -> Self {
        let repository_dir_abs_path =
            repository_dir_abs_path.unwrap_or_else(|| work_directory_abs_path.join(".git").into());
        let dot_git_abs_path =
            dot_git_abs_path.unwrap_or_else(|| work_directory_abs_path.join(".git").into());
        let common_dir_abs_path =
            common_dir_abs_path.unwrap_or_else(|| repository_dir_abs_path.clone());

        Self {
            id,
            statuses_by_path: SumTree::new(()),
            repository_dir_abs_path,
            dot_git_abs_path,
            common_dir_abs_path,
            work_directory_abs_path,
            branch: None,
            branch_list: Arc::from([]),
            branch_list_error: None,
            head_commit: None,
            path_style,
            scan_id: 0,
        }
    }

    pub fn abs_path_to_repo_path(&self, abs_path: &Path) -> Option<RepoPath> {
        Self::abs_path_to_repo_path_inner(&self.work_directory_abs_path, abs_path, self.path_style)
    }

    #[inline]
    fn abs_path_to_repo_path_inner(
        work_directory_abs_path: &Path,
        abs_path: &Path,
        path_style: PathStyle,
    ) -> Option<RepoPath> {
        let rel_path = path_style.strip_prefix(abs_path, work_directory_abs_path)?;
        Some(RepoPath::from_rel_path(&rel_path))
    }

    pub fn status(&self) -> impl Iterator<Item = StatusEntry> + '_ {
        self.statuses_by_path.iter().cloned()
    }

    pub fn status_summary(&self) -> GitSummary {
        self.statuses_by_path.summary().item_summary
    }

    pub fn status_for_path(&self, path: &RepoPath) -> Option<StatusEntry> {
        self.statuses_by_path
            .get(&PathKey(path.as_ref().clone()), ())
            .cloned()
    }
}

#[derive(Clone)]
pub struct RepositoryState {
    pub backend: Arc<dyn GitRepository>,
}

impl RepositoryState {
    async fn new(
        work_directory_abs_path: Arc<Path>,
        dot_git_abs_path: Arc<Path>,
        cx: &mut AsyncApp,
    ) -> anyhow::Result<Self> {
        let search_paths = std::env::var_os("PATH");
        let executor = cx.background_executor().clone();
        let backend = cx
            .background_spawn({
                let executor = executor.clone();
                async move {
                    let system_git_binary_path = search_paths
                        .and_then(|search_paths| {
                            which::which_in(
                                "git",
                                Some(search_paths),
                                work_directory_abs_path.as_ref(),
                            )
                            .ok()
                        })
                        .or_else(|| which::which("git").ok());
                    SystemGitRepository::new(&dot_git_abs_path, system_git_binary_path, executor)
                        .with_context(|| {
                            format!("opening repository at {}", dot_git_abs_path.display())
                        })
                }
            })
            .await?;

        Ok(Self {
            backend: Arc::new(backend),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryEvent {
    StatusesChanged,
    HeadChanged,
    BranchListChanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitStoreEvent {
    ActiveRepositoryChanged(Option<RepositoryId>),
    RepositoryUpdated(RepositoryId, RepositoryEvent, bool),
    RepositoryAdded,
    RepositoryRemoved(RepositoryId),
}

pub struct GitJob {
    job: Box<dyn FnOnce(RepositoryState, &mut AsyncApp) -> Task<()>>,
    key: Option<GitJobKey>,
}

#[derive(PartialEq, Eq)]
enum GitJobKey {
    RefreshStatuses,
    ReloadGitState,
}

pub struct GitStore {
    worktree_store: Entity<WorktreeStore>,
    repositories: HashMap<RepositoryId, Entity<Repository>>,
    worktree_ids: HashMap<RepositoryId, HashSet<WorktreeId>>,
    active_repo_id: Option<RepositoryId>,
    next_repository_id: u64,
    repository_subscriptions: HashMap<RepositoryId, Subscription>,
    _worktree_store_subscription: Subscription,
}

impl GitStore {
    pub fn new(worktree_store: Entity<WorktreeStore>, cx: &mut Context<Self>) -> Self {
        let worktree_store_subscription =
            cx.subscribe(&worktree_store, |this, worktree_store, event, cx| {
                this.on_worktree_store_event(&worktree_store, event, cx);
            });

        Self {
            worktree_store,
            repositories: HashMap::default(),
            worktree_ids: HashMap::default(),
            active_repo_id: None,
            next_repository_id: 1,
            repository_subscriptions: HashMap::default(),
            _worktree_store_subscription: worktree_store_subscription,
        }
    }

    pub fn repositories(&self) -> &HashMap<RepositoryId, Entity<Repository>> {
        &self.repositories
    }

    fn set_active_repo_id(&mut self, repo_id: RepositoryId, cx: &mut Context<Self>) {
        if self.active_repo_id != Some(repo_id) {
            self.active_repo_id = Some(repo_id);
            cx.emit(GitStoreEvent::ActiveRepositoryChanged(Some(repo_id)));
        }
    }

    pub fn set_active_repo_for_path(&mut self, project_path: &ProjectPath, cx: &mut Context<Self>) {
        if let Some((repo, _)) = self.repository_and_path_for_project_path(project_path, cx) {
            self.set_active_repo_id(repo.read(cx).id, cx);
        }
    }

    pub fn set_active_repo_for_worktree(
        &mut self,
        worktree_id: WorktreeId,
        cx: &mut Context<Self>,
    ) {
        let Some(worktree) = self
            .worktree_store
            .read(cx)
            .worktree_for_id(worktree_id, cx)
        else {
            return;
        };
        let worktree_abs_path = worktree.read(cx).abs_path();
        let Some(repo_id) = self
            .repositories
            .values()
            .filter(|repo| {
                let repo_path = &repo.read(cx).work_directory_abs_path;
                worktree_abs_path.starts_with(repo_path.as_ref())
            })
            .max_by_key(|repo| repo.read(cx).work_directory_abs_path.as_os_str().len())
            .map(|repo| repo.read(cx).id)
        else {
            return;
        };

        self.set_active_repo_id(repo_id, cx);
    }

    pub fn active_repository(&self) -> Option<Entity<Repository>> {
        self.active_repo_id
            .as_ref()
            .and_then(|id| self.repositories.get(id).cloned())
    }

    pub fn project_path_git_status(
        &self,
        project_path: &ProjectPath,
        cx: &App,
    ) -> Option<FileStatus> {
        let (repo, repo_path) = self.repository_and_path_for_project_path(project_path, cx)?;
        Some(repo.read(cx).status_for_path(&repo_path)?.status)
    }

    pub fn repository_and_path_for_project_path(
        &self,
        path: &ProjectPath,
        cx: &App,
    ) -> Option<(Entity<Repository>, RepoPath)> {
        let abs_path = self.worktree_store.read(cx).absolutize(path, cx)?;
        self.repositories
            .values()
            .filter_map(|repo| {
                let repo_path = repo.read(cx).abs_path_to_repo_path(&abs_path)?;
                Some((repo.clone(), repo_path))
            })
            .max_by_key(|(repo, _)| repo.read(cx).work_directory_abs_path.clone())
    }

    fn on_worktree_store_event(
        &mut self,
        worktree_store: &Entity<WorktreeStore>,
        event: &WorktreeStoreEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            WorktreeStoreEvent::WorktreeUpdatedEntries(worktree_id, updated_entries) => {
                if let Some(worktree) = worktree_store.read(cx).worktree_for_id(*worktree_id, cx) {
                    let paths_by_git_repo =
                        self.process_updated_entries(&worktree, updated_entries, cx);
                    cx.spawn(async move |_, cx| {
                        let paths_by_git_repo = paths_by_git_repo.await;
                        for (repo, paths) in paths_by_git_repo {
                            repo.update(cx, |repo, cx| {
                                repo.paths_changed(paths, cx);
                            });
                        }
                    })
                    .detach();
                }
            }
            WorktreeStoreEvent::WorktreeUpdatedGitRepositories(worktree_id, changed_repos) => {
                self.update_repositories_from_worktree(*worktree_id, changed_repos, cx);
            }
            WorktreeStoreEvent::WorktreeRemoved(worktree_id) => {
                let removed_ids: Vec<RepositoryId> = self
                    .worktree_ids
                    .iter_mut()
                    .filter_map(|(repo_id, worktree_ids)| {
                        worktree_ids.remove(worktree_id);
                        if worktree_ids.is_empty() {
                            Some(*repo_id)
                        } else {
                            None
                        }
                    })
                    .collect();
                let is_active_repo_removed = removed_ids
                    .iter()
                    .any(|repo_id| self.active_repo_id == Some(*repo_id));

                for id in removed_ids {
                    self.repositories.remove(&id);
                    self.worktree_ids.remove(&id);
                    self.repository_subscriptions.remove(&id);
                    cx.emit(GitStoreEvent::RepositoryRemoved(id));
                }

                if is_active_repo_removed {
                    if let Some((&repo_id, _)) = self.repositories.iter().next() {
                        self.active_repo_id = Some(repo_id);
                        cx.emit(GitStoreEvent::ActiveRepositoryChanged(Some(repo_id)));
                    } else {
                        self.active_repo_id = None;
                        cx.emit(GitStoreEvent::ActiveRepositoryChanged(None));
                    }
                }
            }
            WorktreeStoreEvent::WorktreeAdded(_)
            | WorktreeStoreEvent::WorktreeDeletedEntry(_, _) => {}
        }
    }

    fn update_repositories_from_worktree(
        &mut self,
        worktree_id: WorktreeId,
        updated_git_repositories: &UpdatedGitRepositoriesSet,
        cx: &mut Context<Self>,
    ) {
        let mut removed_ids = Vec::new();
        for update in updated_git_repositories.iter() {
            if let Some((id, existing)) = self.repositories.iter().find(|(_, repo)| {
                let existing_work_directory_abs_path =
                    repo.read(cx).work_directory_abs_path.clone();
                Some(&existing_work_directory_abs_path)
                    == update.old_work_directory_abs_path.as_ref()
                    || Some(&existing_work_directory_abs_path)
                        == update.new_work_directory_abs_path.as_ref()
            }) {
                let repo_id = *id;
                if let Some(new_work_directory_abs_path) =
                    update.new_work_directory_abs_path.clone()
                {
                    self.worktree_ids
                        .entry(repo_id)
                        .or_default()
                        .insert(worktree_id);
                    let path_changed = update.old_work_directory_abs_path.as_ref()
                        != update.new_work_directory_abs_path.as_ref();
                    if path_changed
                        && let Some(dot_git_abs_path) = update.dot_git_abs_path.clone()
                        && let Some(repository_dir_abs_path) =
                            update.repository_dir_abs_path.clone()
                        && let Some(common_dir_abs_path) = update.common_dir_abs_path.clone()
                    {
                        existing.update(cx, |existing, cx| {
                            existing.reinitialize_backend(
                                new_work_directory_abs_path,
                                dot_git_abs_path,
                                repository_dir_abs_path,
                                common_dir_abs_path,
                                cx,
                            );
                            existing.schedule_scan(cx);
                        });
                    } else {
                        existing.update(cx, |existing, cx| {
                            existing.snapshot.work_directory_abs_path = new_work_directory_abs_path;
                            existing.schedule_scan(cx);
                        });
                    }
                } else if let Some(worktree_ids) = self.worktree_ids.get_mut(&repo_id) {
                    worktree_ids.remove(&worktree_id);
                    if worktree_ids.is_empty() {
                        removed_ids.push(repo_id);
                    }
                }
            } else if let UpdatedGitRepository {
                new_work_directory_abs_path: Some(work_directory_abs_path),
                dot_git_abs_path: Some(dot_git_abs_path),
                repository_dir_abs_path: Some(repository_dir_abs_path),
                common_dir_abs_path: Some(common_dir_abs_path),
                ..
            } = update
            {
                let work_directory_abs_path = work_directory_abs_path.clone();
                let dot_git_abs_path = dot_git_abs_path.clone();
                let repository_dir_abs_path = repository_dir_abs_path.clone();
                let common_dir_abs_path = common_dir_abs_path.clone();
                let id = RepositoryId(self.next_repository_id);
                self.next_repository_id += 1;
                let repo = cx.new(move |cx| {
                    let mut repo = Repository::new(
                        id,
                        work_directory_abs_path.clone(),
                        repository_dir_abs_path.clone(),
                        common_dir_abs_path.clone(),
                        dot_git_abs_path.clone(),
                        cx,
                    );
                    repo.schedule_scan(cx);
                    repo
                });
                let repository_subscription = cx.subscribe(&repo, |this, repo, event, cx| {
                    this.on_repository_event(&repo, event, cx);
                });
                self.repositories.insert(id, repo);
                self.repository_subscriptions
                    .insert(id, repository_subscription);
                let mut repository_worktree_ids = HashSet::default();
                repository_worktree_ids.insert(worktree_id);
                self.worktree_ids.insert(id, repository_worktree_ids);
                cx.emit(GitStoreEvent::RepositoryAdded);
                self.active_repo_id.get_or_insert_with(|| {
                    cx.emit(GitStoreEvent::ActiveRepositoryChanged(Some(id)));
                    id
                });
            }
        }

        for id in removed_ids {
            if self.active_repo_id == Some(id) {
                self.active_repo_id = None;
                cx.emit(GitStoreEvent::ActiveRepositoryChanged(None));
            }
            self.repositories.remove(&id);
            self.worktree_ids.remove(&id);
            self.repository_subscriptions.remove(&id);
            cx.emit(GitStoreEvent::RepositoryRemoved(id));
        }
    }

    fn on_repository_event(
        &mut self,
        repo: &Entity<Repository>,
        event: &RepositoryEvent,
        cx: &mut Context<Self>,
    ) {
        let id = repo.read(cx).id;
        cx.emit(GitStoreEvent::RepositoryUpdated(
            id,
            event.clone(),
            self.active_repo_id == Some(id),
        ));
    }

    pub fn repo_snapshots(&self, cx: &App) -> HashMap<RepositoryId, RepositorySnapshot> {
        self.repositories
            .iter()
            .map(|(id, repo)| (*id, repo.read(cx).snapshot.clone()))
            .collect()
    }

    fn coalesce_repo_paths(mut paths: Vec<RepoPath>) -> Vec<RepoPath> {
        paths.sort();

        let mut coalesced = Vec::with_capacity(paths.len());
        for path in paths {
            if coalesced
                .last()
                .is_some_and(|ancestor: &RepoPath| path.starts_with(ancestor))
            {
                continue;
            }
            coalesced.push(path);
        }

        coalesced
    }

    fn process_updated_entries(
        &self,
        worktree: &Entity<Worktree>,
        updated_entries: &[(Arc<RelPath>, ProjectEntryId, PathChange)],
        cx: &mut App,
    ) -> Task<HashMap<Entity<Repository>, Vec<RepoPath>>> {
        let path_style = worktree.read(cx).path_style();
        let mut repo_paths = self
            .repositories
            .values()
            .map(|repo| (repo.read(cx).work_directory_abs_path.clone(), repo.clone()))
            .collect::<Vec<_>>();
        let mut entries: Vec<_> = updated_entries
            .iter()
            .map(|(path, _, _)| path.clone())
            .collect();
        entries.sort();
        let worktree = worktree.read(cx);

        let entries = entries
            .into_iter()
            .map(|path| worktree.absolutize(&path))
            .collect::<Arc<[_]>>();

        let executor = cx.background_executor().clone();
        cx.background_executor().spawn(async move {
            repo_paths.sort_by(|lhs, rhs| lhs.0.cmp(&rhs.0));
            let mut paths_by_git_repo = HashMap::<_, Vec<_>>::default();
            let mut tasks = FuturesOrdered::new();
            for (repo_path, repo) in repo_paths.into_iter().rev() {
                let entries = entries.clone();
                let task = executor.spawn(async move {
                    let mut entry_index = entries.partition_point(|path| path < &*repo_path);
                    if entry_index == entries.len() {
                        return None;
                    }

                    let mut paths = Vec::new();
                    while let Some(path) = entries.get(entry_index)
                        && let Some(repo_path) = RepositorySnapshot::abs_path_to_repo_path_inner(
                            &repo_path, path, path_style,
                        )
                    {
                        paths.push((repo_path, entry_index));
                        entry_index += 1;
                    }
                    if paths.is_empty() {
                        None
                    } else {
                        Some((repo, paths))
                    }
                });
                tasks.push_back(task);
            }

            let mut path_was_used = vec![false; entries.len()];
            let tasks = tasks.collect::<Vec<_>>().await;
            for task in tasks {
                let Some((repo, paths)) = task else {
                    continue;
                };
                let entry = paths_by_git_repo.entry(repo).or_default();
                for (repo_path, entry_index) in paths {
                    let Some(was_used) = path_was_used.get_mut(entry_index) else {
                        continue;
                    };
                    if *was_used {
                        continue;
                    }
                    *was_used = true;
                    entry.push(repo_path);
                }
            }

            for paths in paths_by_git_repo.values_mut() {
                *paths = Self::coalesce_repo_paths(std::mem::take(paths));
            }

            paths_by_git_repo
        })
    }
}

impl EventEmitter<GitStoreEvent> for GitStore {}

pub struct Repository {
    snapshot: RepositorySnapshot,
    repository_state: future::Shared<Task<Result<RepositoryState, String>>>,
    paths_needing_status_update: Vec<Vec<RepoPath>>,
    job_sender: mpsc::UnboundedSender<GitJob>,
    worker_task: Task<()>,
}

impl Repository {
    fn new(
        id: RepositoryId,
        work_directory_abs_path: Arc<Path>,
        repository_dir_abs_path: Arc<Path>,
        common_dir_abs_path: Arc<Path>,
        dot_git_abs_path: Arc<Path>,
        cx: &mut Context<Self>,
    ) -> Self {
        let snapshot = RepositorySnapshot::empty(
            id,
            work_directory_abs_path,
            Some(repository_dir_abs_path),
            Some(dot_git_abs_path),
            Some(common_dir_abs_path),
            PathStyle::local(),
        );

        let mut repo = Repository {
            snapshot,
            repository_state: Task::ready(Err("not yet initialized".into())).shared(),
            paths_needing_status_update: Vec::new(),
            job_sender: mpsc::unbounded().0,
            worker_task: Task::ready(()),
        };
        repo.respawn_worker(cx);
        repo
    }

    pub fn snapshot(&self) -> RepositorySnapshot {
        self.snapshot.clone()
    }

    fn respawn_worker(&mut self, cx: &mut Context<Self>) {
        let work_directory_abs_path = self.snapshot.work_directory_abs_path.clone();
        let dot_git_abs_path = self.snapshot.dot_git_abs_path.clone();
        let state = cx
            .spawn(async move |_, cx| {
                RepositoryState::new(work_directory_abs_path, dot_git_abs_path, cx)
                    .await
                    .map_err(|error| error.to_string())
            })
            .shared();

        self.job_sender.close_channel();

        self.repository_state = state.clone();
        let (job_sender, worker_task) =
            Repository::spawn_git_worker(self.repository_state.clone(), cx);
        self.job_sender = job_sender;
        self.worker_task = worker_task;
    }

    fn reinitialize_backend(
        &mut self,
        work_directory_abs_path: Arc<Path>,
        dot_git_abs_path: Arc<Path>,
        repository_dir_abs_path: Arc<Path>,
        common_dir_abs_path: Arc<Path>,
        cx: &mut Context<Self>,
    ) {
        self.snapshot.work_directory_abs_path = work_directory_abs_path;
        self.snapshot.dot_git_abs_path = dot_git_abs_path;
        self.snapshot.repository_dir_abs_path = repository_dir_abs_path;
        self.snapshot.common_dir_abs_path = common_dir_abs_path;
        self.respawn_worker(cx);
    }

    fn schedule_scan(&mut self, cx: &mut Context<Self>) {
        let this = cx.weak_entity();
        self.send_keyed_job(Some(GitJobKey::ReloadGitState), move |state, cx| {
            cx.spawn(async move |cx| {
                log::debug!("Run scheduled Git status scan");

                let Some(this) = this.upgrade() else {
                    return;
                };
                let RepositoryState { backend } = state;
                compute_snapshot(this, backend, cx).await;
            })
        });
    }

    fn spawn_git_worker(
        state: future::Shared<Task<Result<RepositoryState, String>>>,
        cx: &mut Context<Self>,
    ) -> (mpsc::UnboundedSender<GitJob>, Task<()>) {
        let (job_tx, mut job_rx) = mpsc::unbounded::<GitJob>();

        let worker_task = cx.spawn(async move |_, cx| {
            let Some(state) = state.await.log_err() else {
                return;
            };
            let mut jobs = VecDeque::new();
            loop {
                while let Ok(next_job) = job_rx.try_recv() {
                    jobs.push_back(next_job);
                }

                if let Some(job) = jobs.pop_front() {
                    if let Some(current_key) = &job.key
                        && jobs
                            .iter()
                            .any(|other_job| other_job.key.as_ref() == Some(current_key))
                    {
                        continue;
                    }
                    (job.job)(state.clone(), cx).await;
                } else if let Some(job) = job_rx.next().await {
                    jobs.push_back(job);
                } else {
                    break;
                }
            }
        });

        (job_tx, worker_task)
    }

    fn send_keyed_job<F>(&mut self, key: Option<GitJobKey>, job: F)
    where
        F: FnOnce(RepositoryState, &mut AsyncApp) -> Task<()> + 'static,
    {
        if self
            .job_sender
            .unbounded_send(GitJob {
                key,
                job: Box::new(job),
            })
            .is_err()
        {
            log::error!("Failed to queue Git job");
        }
    }

    fn paths_changed(&mut self, paths: Vec<RepoPath>, cx: &mut Context<Self>) {
        if !paths.is_empty() {
            self.paths_needing_status_update.push(paths);
        }

        let this = cx.weak_entity();
        self.send_keyed_job(Some(GitJobKey::RefreshStatuses), move |state, cx| {
            cx.spawn(async move |cx| {
                let Some(this) = this.upgrade() else {
                    return;
                };
                let (prev_statuses, changed_paths) = this.update(cx, |this, _| {
                    (
                        this.snapshot.statuses_by_path.clone(),
                        std::mem::take(&mut this.paths_needing_status_update),
                    )
                });

                if changed_paths.is_empty() {
                    return;
                }

                let RepositoryState { backend } = state;
                match cx
                    .background_spawn(async move {
                        let changed_paths = GitStore::coalesce_repo_paths(
                            changed_paths
                                .into_iter()
                                .flatten()
                                .collect::<BTreeSet<_>>()
                                .into_iter()
                                .collect(),
                        );

                        let statuses = backend.status(&changed_paths).await?;
                        let current_status_paths = statuses
                            .entries
                            .iter()
                            .map(|(repo_path, _)| repo_path.clone())
                            .collect::<BTreeSet<_>>();
                        let mut changed_path_statuses = Vec::new();

                        for path in &changed_paths {
                            let mut cursor = prev_statuses.cursor::<PathProgress>(());
                            cursor.seek_forward(&PathTarget::Path(path), Bias::Left);
                            while let Some(entry) = cursor.item() {
                                if !entry.repo_path.starts_with(path) {
                                    break;
                                }

                                if !current_status_paths.contains(&entry.repo_path) {
                                    changed_path_statuses.push(Edit::Remove(PathKey(
                                        entry.repo_path.as_ref().clone(),
                                    )));
                                }
                                cursor.next();
                            }
                        }

                        let mut cursor = prev_statuses.cursor::<PathProgress>(());
                        for (repo_path, status) in statuses.entries.iter() {
                            if cursor.seek_forward(&PathTarget::Path(repo_path), Bias::Left)
                                && cursor
                                    .item()
                                    .is_some_and(|prev_entry| prev_entry.status == *status)
                            {
                                continue;
                            }

                            changed_path_statuses.push(Edit::Insert(StatusEntry {
                                repo_path: repo_path.clone(),
                                status: *status,
                            }));
                        }

                        anyhow::Ok(changed_path_statuses)
                    })
                    .await
                {
                    Ok(changed_path_statuses) => {
                        this.update(cx, |this, cx| {
                            if !changed_path_statuses.is_empty() {
                                cx.emit(RepositoryEvent::StatusesChanged);
                                this.snapshot
                                    .statuses_by_path
                                    .edit(changed_path_statuses, ());
                                this.snapshot.scan_id += 1;
                            }
                        });
                    }
                    Err(error) => {
                        log::error!("Failed to scan Git status: {error:#}");
                    }
                }
            })
        });
    }
}

impl ops::Deref for Repository {
    type Target = RepositorySnapshot;

    fn deref(&self) -> &Self::Target {
        &self.snapshot
    }
}

impl EventEmitter<RepositoryEvent> for Repository {}

async fn compute_snapshot(
    this: Entity<Repository>,
    backend: Arc<dyn GitRepository>,
    cx: &mut AsyncApp,
) -> RepositorySnapshot {
    log::debug!("Starting compute snapshot");

    this.update(cx, |this, _| {
        this.paths_needing_status_update.clear();
    });

    let branches_future = {
        let backend = backend.clone();
        async move { backend.branches().await.log_err().unwrap_or_default() }
    };
    let head_commit_future = {
        let backend = backend.clone();
        async move { backend.show("HEAD".to_string()).await.ok() }
    };
    let (branches, head_commit) = future::join(branches_future, head_commit_future).await;
    log::debug!("Fetched branches and HEAD commit");

    let BranchesScanResult {
        branches,
        error: branch_list_error,
    } = branches;
    let branch = branches.iter().find(|branch| branch.is_head).cloned();
    let branch_list: Arc<[Branch]> = branches.into();

    let statuses = match RelPath::new(".".as_ref(), PathStyle::local()) {
        Ok(path) => backend
            .status(&[RepoPath::from_rel_path(&path)])
            .await
            .log_err()
            .unwrap_or_default(),
        Err(error) => {
            log::error!("Failed to create Git status root path: {error:#}");
            GitStatus::default()
        }
    };
    log::debug!("Fetched statuses");

    let statuses_by_path = SumTree::from_iter(
        statuses
            .entries
            .iter()
            .map(|(repo_path, status)| StatusEntry {
                repo_path: repo_path.clone(),
                status: *status,
            }),
        (),
    );

    this.update(cx, |this, cx| {
        let head_changed =
            branch != this.snapshot.branch || head_commit != this.snapshot.head_commit;
        let branch_list_changed = *branch_list != *this.snapshot.branch_list;
        let branch_list_error_changed = branch_list_error != this.snapshot.branch_list_error;

        if head_changed {
            cx.emit(RepositoryEvent::HeadChanged);
        }

        if branch_list_changed || branch_list_error_changed {
            cx.emit(RepositoryEvent::BranchListChanged);
        }

        if statuses_by_path != this.snapshot.statuses_by_path {
            cx.emit(RepositoryEvent::StatusesChanged);
        }

        this.snapshot.branch = branch;
        this.snapshot.branch_list = branch_list;
        this.snapshot.branch_list_error = branch_list_error;
        this.snapshot.head_commit = head_commit;
        this.snapshot.scan_id += 1;
        this.snapshot.statuses_by_path = statuses_by_path;
        this.snapshot.clone()
    })
}

pub fn repo_identity_path(common_dir: &Path) -> &Path {
    let is_dot_entry = common_dir
        .file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with('.'));
    if is_dot_entry {
        common_dir.parent().unwrap_or(common_dir)
    } else {
        common_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coalesce_repo_paths_keeps_root_only() {
        let coalesced = GitStore::coalesce_repo_paths(vec![
            RepoPath::new("").unwrap(),
            RepoPath::new("foo").unwrap(),
            RepoPath::new("foo/first.toml").unwrap(),
        ]);

        assert_eq!(coalesced, vec![RepoPath::new("").unwrap()]);
    }

    #[test]
    fn test_coalesce_repo_paths_keeps_existing_ancestors() {
        let coalesced = GitStore::coalesce_repo_paths(vec![
            RepoPath::new("bar").unwrap(),
            RepoPath::new("bar/first.toml").unwrap(),
            RepoPath::new("bar/baz/second.toml").unwrap(),
            RepoPath::new("foo/third.toml").unwrap(),
        ]);

        assert_eq!(
            coalesced,
            vec![
                RepoPath::new("bar").unwrap(),
                RepoPath::new("foo/third.toml").unwrap(),
            ]
        );
    }

    #[test]
    fn test_coalesce_repo_paths_does_not_invent_missing_parents() {
        let coalesced = GitStore::coalesce_repo_paths(vec![
            RepoPath::new("foo/first.toml").unwrap(),
            RepoPath::new("foo/bar/second.toml").unwrap(),
            RepoPath::new("foo/bar/baz/third.toml").unwrap(),
        ]);

        assert_eq!(
            coalesced,
            vec![
                RepoPath::new("foo/bar/baz/third.toml").unwrap(),
                RepoPath::new("foo/bar/second.toml").unwrap(),
                RepoPath::new("foo/first.toml").unwrap(),
            ]
        );
    }

    #[test]
    fn test_repo_identity_path() {
        let examples = [
            ("/home/me/zaku/.git", "/home/me/zaku"),
            ("/repos/project/.bare", "/repos/project"),
            ("/repos/zaku.git", "/repos/zaku.git"),
            ("/repos/project", "/repos/project"),
        ];

        for (common_dir, expected) in examples {
            assert_eq!(
                repo_identity_path(Path::new(common_dir)),
                Path::new(expected)
            );
        }
    }
}
