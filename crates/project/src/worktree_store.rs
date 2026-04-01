use anyhow::anyhow;
use futures::{FutureExt, future};
use gpui::{App, Context, Entity, EventEmitter, Global, Subscription, Task};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use fs::Fs;
use util::{
    path::{PathStyle, SanitizedPath},
    rel_path::RelPath,
};
use worktree::{Snapshot, UpdatedEntriesSet, Worktree, WorktreeEvent, WorktreeId};

#[derive(Clone)]
pub struct WorktreeIdCounter(Arc<AtomicUsize>);

impl Default for WorktreeIdCounter {
    fn default() -> Self {
        Self(Arc::new(AtomicUsize::new(1)))
    }
}

impl WorktreeIdCounter {
    pub fn get(cx: &mut App) -> Self {
        cx.default_global::<Self>().clone()
    }

    fn next(&self) -> usize {
        self.0.fetch_add(1, Ordering::Relaxed)
    }
}

impl Global for WorktreeIdCounter {}

pub struct WorktreeStore {
    next_entry_id: Arc<AtomicUsize>,
    next_worktree_id: WorktreeIdCounter,
    worktree: Option<Entity<Worktree>>,
    worktree_subscription: Option<Subscription>,
    worktree_path_to_open: Option<Arc<SanitizedPath>>,
    worktree_open_epoch: usize,
    scanning_enabled: bool,
    pending_worktree_tasks: HashMap<
        Arc<SanitizedPath>,
        future::Shared<Task<std::result::Result<Entity<Worktree>, Arc<anyhow::Error>>>>,
    >,
    fs: Arc<dyn Fs>,
}

#[derive(Debug)]
pub enum WorktreeStoreEvent {
    WorktreeAdded,
    WorktreeRemoved,
    WorktreeUpdatedEntries(UpdatedEntriesSet),
}

impl EventEmitter<WorktreeStoreEvent> for WorktreeStore {}

impl WorktreeStore {
    pub fn new(fs: Arc<dyn Fs>, next_worktree_id: WorktreeIdCounter) -> Self {
        Self {
            next_entry_id: Default::default(),
            next_worktree_id,
            worktree: None,
            worktree_subscription: None,
            worktree_path_to_open: None,
            worktree_open_epoch: 0,
            scanning_enabled: true,
            pending_worktree_tasks: Default::default(),
            fs,
        }
    }

    fn next_worktree_id(&self) -> WorktreeId {
        WorktreeId::from_usize(self.next_worktree_id.next())
    }

    pub fn disable_scanner(&mut self) {
        self.scanning_enabled = false;
    }

    pub fn worktree(&self) -> Option<Entity<Worktree>> {
        self.worktree.clone()
    }

    pub fn snapshot(&self, cx: &App) -> Option<Snapshot> {
        self.worktree
            .as_ref()
            .map(|worktree| worktree.read(cx).snapshot())
    }

    pub fn root(&self, cx: &App) -> Option<PathBuf> {
        self.worktree
            .as_ref()
            .map(|worktree| worktree.read(cx).abs_path().as_ref().to_path_buf())
    }

    fn find_worktree(
        &self,
        abs_path: impl AsRef<Path>,
        cx: &App,
    ) -> Option<(Entity<Worktree>, Arc<RelPath>)> {
        let abs_path = SanitizedPath::new(abs_path.as_ref());
        let worktree = self.worktree.as_ref()?;
        let path_style = worktree.read(cx).path_style();
        if let Some(relative_path) =
            path_style.strip_prefix(abs_path.as_path(), worktree.read(cx).abs_path().as_ref())
        {
            return Some((worktree.clone(), Arc::from(relative_path.as_ref())));
        }
        None
    }

    pub fn absolutize(&self, path: &RelPath, cx: &App) -> Option<PathBuf> {
        let worktree = self.worktree.as_ref()?;
        Some(worktree.read(cx).absolutize(path))
    }

    pub fn path_style(&self) -> PathStyle {
        PathStyle::local()
    }

    pub fn find_or_create_worktree(
        &mut self,
        abs_path: impl AsRef<Path>,
        visible: bool,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Entity<Worktree>>> {
        let requested_abs_path = SanitizedPath::new_arc(abs_path.as_ref());
        self.worktree_path_to_open = Some(requested_abs_path.clone());

        if let Some((worktree, relative_path)) =
            self.find_worktree(requested_abs_path.as_path(), cx)
            && relative_path.is_empty()
        {
            Task::ready(Ok(worktree))
        } else {
            self.create_worktree(requested_abs_path.as_path(), visible, cx)
        }
    }

    pub fn create_worktree(
        &mut self,
        abs_path: impl AsRef<Path>,
        visible: bool,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Entity<Worktree>>> {
        let requested_abs_path = SanitizedPath::new_arc(abs_path.as_ref());
        let worktree_open_epoch = self.worktree_open_epoch;
        self.worktree_path_to_open = Some(requested_abs_path.clone());
        let fs = self.fs.clone();

        cx.spawn(async move |worktree_store, cx| {
            let canonical_abs_path = match fs.canonicalize(requested_abs_path.as_path()).await {
                Ok(path) => SanitizedPath::new_arc(path.as_path()),
                Err(_) => requested_abs_path.clone(),
            };

            let task = worktree_store.update(cx, |this, cx| {
                let is_current_open_epoch = this.worktree_open_epoch == worktree_open_epoch;
                let is_latest_request = this.worktree_path_to_open.as_deref()
                    == Some(requested_abs_path.as_ref())
                    || this.worktree_path_to_open.as_deref() == Some(canonical_abs_path.as_ref());
                if !is_current_open_epoch || !is_latest_request {
                    return Err(anyhow!("Worktree open was superseded by a newer request"));
                }

                this.worktree_path_to_open = Some(canonical_abs_path.clone());

                if let Some(worktree) = this.worktree.as_ref()
                    && worktree.read(cx).abs_path().as_ref() == canonical_abs_path.as_path()
                {
                    return Ok(Task::ready(Ok(worktree.clone())).shared());
                }

                if !this
                    .pending_worktree_tasks
                    .contains_key(&canonical_abs_path)
                {
                    let fs = this.fs.clone();
                    let task =
                        this.create_local_worktree(fs, canonical_abs_path.clone(), visible, cx);
                    this.pending_worktree_tasks
                        .insert(canonical_abs_path.clone(), task.shared());
                }

                let Some(task) = this
                    .pending_worktree_tasks
                    .get(&canonical_abs_path)
                    .cloned()
                else {
                    return Err(anyhow!("Missing pending worktree task"));
                };

                Ok(task)
            })??;

            let worktree = match task.await {
                Ok(worktree) => worktree,
                Err(error) => {
                    worktree_store.update(cx, |this, _| {
                        this.pending_worktree_tasks.remove(&canonical_abs_path);
                    })?;

                    return Err(anyhow!("{error:#}"));
                }
            };

            let worktree_opened = worktree_store.update(cx, |this, cx| {
                this.pending_worktree_tasks.remove(&canonical_abs_path);

                let is_current_open_epoch = this.worktree_open_epoch == worktree_open_epoch;
                let is_latest_request = this.worktree_path_to_open.as_deref()
                    == Some(requested_abs_path.as_ref())
                    || this.worktree_path_to_open.as_deref() == Some(canonical_abs_path.as_ref());
                let is_current_worktree = this.worktree.as_ref().is_some_and(|current_worktree| {
                    current_worktree.entity_id() == worktree.entity_id()
                });

                if is_current_open_epoch && is_latest_request && !is_current_worktree {
                    this.add(&worktree, cx);
                    true
                } else if is_current_open_epoch && is_latest_request {
                    is_current_worktree
                } else {
                    false
                }
            })?;

            if worktree_opened {
                Ok(worktree)
            } else {
                Err(anyhow!("Worktree open was superseded by a newer request"))
            }
        })
    }

    fn create_local_worktree(
        &mut self,
        fs: Arc<dyn Fs>,
        abs_path: Arc<SanitizedPath>,
        visible: bool,
        cx: &mut Context<Self>,
    ) -> Task<std::result::Result<Entity<Worktree>, Arc<anyhow::Error>>> {
        let next_entry_id = self.next_entry_id.clone();
        let scanning_enabled = self.scanning_enabled;
        let worktree_id = self.next_worktree_id();

        cx.spawn(async move |_, cx| {
            Worktree::local(
                SanitizedPath::cast_arc(abs_path),
                visible,
                fs,
                next_entry_id,
                scanning_enabled,
                worktree_id,
                cx,
            )
            .await
            .map_err(Arc::new)
        })
    }

    fn add(&mut self, worktree: &Entity<Worktree>, cx: &mut Context<Self>) {
        let worktree_id = worktree.read(cx).id();
        if let Some(current_worktree) = self.worktree.as_ref() {
            debug_assert_ne!(current_worktree.read(cx).id(), worktree_id);
        }

        if self.worktree.replace(worktree.clone()).is_some() {
            self.worktree_subscription.take();
            cx.emit(WorktreeStoreEvent::WorktreeRemoved);
        }

        self.worktree_subscription = Some(cx.subscribe(
            worktree,
            |_, _, event: &WorktreeEvent, cx| match event {
                WorktreeEvent::UpdatedEntries(changes) => {
                    cx.emit(WorktreeStoreEvent::WorktreeUpdatedEntries(changes.clone()));
                }
            },
        ));

        cx.emit(WorktreeStoreEvent::WorktreeAdded);
    }

    pub fn remove_worktree(&mut self, cx: &mut Context<Self>) {
        self.worktree_subscription.take();
        let removed_worktree = self.worktree.take();
        self.worktree_path_to_open = None;
        self.worktree_open_epoch = self.worktree_open_epoch.wrapping_add(1);

        if removed_worktree.is_some() {
            cx.emit(WorktreeStoreEvent::WorktreeRemoved);
        }
    }
}
