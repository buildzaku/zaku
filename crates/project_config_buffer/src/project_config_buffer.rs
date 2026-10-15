use gpui::{AppContext, Context, EventEmitter, Task};
use std::sync::Arc;

use fs::MTime;
use worktree::{ConfigFileState, ConfigFileType, DiskState, File};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectConfigBufferEvent {
    DirtyChanged,
    Saved,
    FileHandleChanged,
    Reloaded,
    ReloadNeeded,
}

pub struct ProjectConfigBuffer {
    file: Arc<File>,
    file_type: ConfigFileType,
    config_file: ConfigFileState,
    version: usize,
    saved_mtime: Option<MTime>,
    is_dirty: bool,
    has_conflict: bool,
}

impl ProjectConfigBuffer {
    pub fn new(file: Arc<File>, file_type: ConfigFileType, config_file: ConfigFileState) -> Self {
        let saved_mtime = file.disk_state.mtime();
        Self {
            file,
            file_type,
            config_file,
            version: 0,
            saved_mtime,
            is_dirty: false,
            has_conflict: false,
        }
    }

    pub fn file(&self) -> &Arc<File> {
        &self.file
    }

    pub fn file_type(&self) -> ConfigFileType {
        self.file_type
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
                cx.emit(ProjectConfigBufferEvent::ReloadNeeded);
            }
        }

        self.file = new_file;
        if file_changed {
            cx.emit(ProjectConfigBufferEvent::FileHandleChanged);
            cx.notify();
        }
    }

    pub fn config_file(&self) -> &ConfigFileState {
        &self.config_file
    }

    pub fn set_config_file(&mut self, config_file: ConfigFileState, cx: &mut Context<Self>) {
        if self.config_file == config_file {
            return;
        }

        self.config_file = config_file;
        self.version += 1;
        cx.notify();
    }

    pub fn version(&self) -> usize {
        self.version
    }

    pub fn reload(&mut self, cx: &Context<Self>) -> Task<anyhow::Result<()>> {
        let version = self.version;
        let mtime = self.file.disk_state.mtime();
        let file_type = self.file_type;
        let load_task = language::File::load(self.file.as_ref(), cx);

        cx.spawn(async move |this, cx| {
            let contents = load_task.await?;
            let parse_task = cx.background_spawn(async move { file_type.parse(&contents) });
            let config_file = parse_task.await;
            this.update(cx, |this, cx| {
                if this.version == version {
                    this.did_reload(config_file, mtime, cx);
                } else {
                    let was_dirty = this.is_dirty();
                    this.has_conflict = true;
                    if !was_dirty {
                        cx.emit(ProjectConfigBufferEvent::DirtyChanged);
                    }
                    cx.notify();
                }
            })?;
            anyhow::Ok(())
        })
    }

    pub fn is_dirty(&self) -> bool {
        self.is_dirty || self.has_conflict
    }

    pub fn has_conflict(&self) -> bool {
        if self.has_conflict {
            return true;
        }
        match self.file.disk_state {
            DiskState::New | DiskState::Deleted => false,
            DiskState::Present { mtime, .. } => match self.saved_mtime {
                Some(saved_mtime) => mtime.bad_is_greater_than(saved_mtime) && self.is_dirty,
                None => true,
            },
        }
    }

    pub fn set_dirty(&mut self, is_dirty: bool, cx: &mut Context<Self>) -> bool {
        let was_dirty = self.is_dirty();
        self.is_dirty = is_dirty;
        let dirty_changed = was_dirty != self.is_dirty();
        if dirty_changed {
            cx.emit(ProjectConfigBufferEvent::DirtyChanged);
            cx.notify();
        }
        dirty_changed
    }

    pub fn did_save(&mut self, version: usize, mtime: Option<MTime>, cx: &mut Context<Self>) {
        let was_dirty = self.is_dirty();
        if self.version == version {
            self.is_dirty = false;
        }
        self.has_conflict = false;
        self.saved_mtime = mtime;
        if was_dirty != self.is_dirty() {
            cx.emit(ProjectConfigBufferEvent::DirtyChanged);
        }
        cx.emit(ProjectConfigBufferEvent::Saved);
        cx.notify();
    }

    pub fn did_reload(
        &mut self,
        config_file: ConfigFileState,
        mtime: Option<MTime>,
        cx: &mut Context<Self>,
    ) {
        let was_dirty = self.is_dirty();
        self.config_file = config_file;
        self.version += 1;
        self.saved_mtime = mtime;
        self.is_dirty = false;
        self.has_conflict = false;
        if was_dirty != self.is_dirty() {
            cx.emit(ProjectConfigBufferEvent::DirtyChanged);
        }
        cx.emit(ProjectConfigBufferEvent::Reloaded);
        cx.notify();
    }
}

impl EventEmitter<ProjectConfigBufferEvent> for ProjectConfigBuffer {}
