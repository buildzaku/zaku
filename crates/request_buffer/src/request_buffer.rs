use gpui::Context;
use std::sync::Arc;

use util::rel_path::RelPath;
use worktree::{ProjectEntryId, RequestFileState, WorktreeId};

pub struct RequestBuffer {
    entry_id: ProjectEntryId,
    worktree_id: WorktreeId,
    path: Arc<RelPath>,
    request_file: RequestFileState,
    is_dirty: bool,
}

impl RequestBuffer {
    pub fn new(
        entry_id: ProjectEntryId,
        worktree_id: WorktreeId,
        path: Arc<RelPath>,
        request_file: RequestFileState,
    ) -> Self {
        Self {
            entry_id,
            worktree_id,
            path,
            request_file,
            is_dirty: false,
        }
    }

    pub fn entry_id(&self) -> ProjectEntryId {
        self.entry_id
    }

    pub fn worktree_id(&self) -> WorktreeId {
        self.worktree_id
    }

    pub fn path(&self) -> Arc<RelPath> {
        self.path.clone()
    }

    pub fn request_file(&self) -> &RequestFileState {
        &self.request_file
    }

    pub fn set_request_file(&mut self, request_file: RequestFileState, cx: &mut Context<Self>) {
        self.request_file = request_file;
        cx.notify();
    }

    pub fn is_dirty(&self) -> bool {
        self.is_dirty
    }

    pub fn set_dirty(&mut self, is_dirty: bool, cx: &mut Context<Self>) -> bool {
        let dirty_changed = self.is_dirty != is_dirty;
        if dirty_changed {
            self.is_dirty = is_dirty;
            cx.notify();
        }
        dirty_changed
    }

    pub fn did_save(&mut self, cx: &mut Context<Self>) {
        self.is_dirty = false;
        cx.notify();
    }
}
