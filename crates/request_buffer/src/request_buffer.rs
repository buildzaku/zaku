use gpui::{AppContext, Context, EventEmitter, Task};
use std::sync::Arc;

use worktree::{DiskState, File, RequestFileState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestBufferEvent {
    DirtyChanged,
    Saved,
    FileHandleChanged,
    Reloaded,
    ReloadNeeded,
}

pub struct RequestBuffer {
    file: Arc<File>,
    request_file: RequestFileState,
    is_dirty: bool,
}

impl RequestBuffer {
    pub fn new(file: Arc<File>, request_file: RequestFileState) -> Self {
        Self {
            file,
            request_file,
            is_dirty: false,
        }
    }

    pub fn file(&self) -> &Arc<File> {
        &self.file
    }

    pub fn file_updated(&mut self, new_file: Arc<File>, cx: &mut Context<Self>) {
        let was_dirty = self.is_dirty();
        let mut file_changed = false;

        if new_file.path.as_ref() != self.file.path.as_ref() {
            file_changed = true;
        }

        let old_state = self.file.disk_state;
        let new_state = new_file.disk_state;
        if new_state != old_state {
            file_changed = true;
            if !was_dirty && matches!(new_state, DiskState::Present { .. }) {
                cx.emit(RequestBufferEvent::ReloadNeeded);
            }
        }

        self.file = new_file;
        if file_changed {
            cx.emit(RequestBufferEvent::FileHandleChanged);
            cx.notify();
        }
    }

    pub fn request_file(&self) -> &RequestFileState {
        &self.request_file
    }

    pub fn set_request_file(&mut self, request_file: RequestFileState, cx: &mut Context<Self>) {
        if self.request_file == request_file {
            return;
        }

        self.request_file = request_file;
        cx.notify();
    }

    pub fn reload(&mut self, cx: &Context<Self>) -> Task<anyhow::Result<()>> {
        let load_task = language::File::load(self.file.as_ref(), cx);

        cx.spawn(async move |this, cx| {
            let contents = load_task.await?;
            let parse_task =
                cx.background_spawn(async move { worktree::parse_request_file(&contents) });
            let request_file = parse_task.await;
            this.update(cx, |this, cx| {
                this.did_reload(request_file, cx);
            })?;
            anyhow::Ok(())
        })
    }

    pub fn is_dirty(&self) -> bool {
        self.is_dirty
    }

    pub fn set_dirty(&mut self, is_dirty: bool, cx: &mut Context<Self>) -> bool {
        let dirty_changed = self.is_dirty != is_dirty;
        if dirty_changed {
            self.is_dirty = is_dirty;
            cx.emit(RequestBufferEvent::DirtyChanged);
            cx.notify();
        }
        dirty_changed
    }

    pub fn did_save(&mut self, cx: &mut Context<Self>) {
        let dirty_changed = self.is_dirty;
        self.is_dirty = false;
        if dirty_changed {
            cx.emit(RequestBufferEvent::DirtyChanged);
        }
        cx.emit(RequestBufferEvent::Saved);
        cx.notify();
    }

    pub fn did_reload(&mut self, request_file: RequestFileState, cx: &mut Context<Self>) {
        self.request_file = request_file;
        let dirty_changed = self.is_dirty;
        self.is_dirty = false;
        if dirty_changed {
            cx.emit(RequestBufferEvent::DirtyChanged);
        }
        cx.emit(RequestBufferEvent::Reloaded);
        cx.notify();
    }
}

impl EventEmitter<RequestBufferEvent> for RequestBuffer {}
