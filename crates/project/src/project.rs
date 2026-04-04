pub mod worktree_store;

use anyhow;

#[cfg(feature = "test-support")]
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

pub use worktree::{Entry, EntryKind, ProjectEntryId, Snapshot, Worktree};

use crate::worktree_store::{WorktreeIdCounter, WorktreeStore, WorktreeStoreEvent};

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
        let worktree_store = cx.new({
            let fs = fs.clone();
            move |cx| WorktreeStore::new(fs.clone(), WorktreeIdCounter::get(cx))
        });
        cx.subscribe(&worktree_store, Self::on_worktree_store_event)
            .detach();

        Self { worktree_store }
    }

    pub fn open_local(
        fs: Arc<dyn Fs>,
        abs_path: PathBuf,
        cx: &mut App,
    ) -> Task<anyhow::Result<Entity<Self>>> {
        let project = cx.new({
            let fs = fs.clone();
            move |cx| Self::new(fs.clone(), cx)
        });
        let open_task = project.update(cx, |project, cx| {
            project.find_or_create_worktree(abs_path, true, cx)
        });

        cx.spawn(async move |_| {
            open_task.await?;
            Ok(project)
        })
    }

    #[cfg(feature = "test-support")]
    pub async fn test(
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

    fn on_worktree_store_event(
        &mut self,
        _: Entity<WorktreeStore>,
        event: &WorktreeStoreEvent,
        cx: &mut Context<Self>,
    ) {
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

    pub fn absolutize(&self, path: &RelPath, cx: &App) -> Option<PathBuf> {
        self.worktree_store.read(cx).absolutize(path, cx)
    }
}
