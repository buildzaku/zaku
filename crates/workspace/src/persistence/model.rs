use gpui::WindowId;
use std::path::{Path, PathBuf};

use crate::WorkspaceId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SerializedWorkspaceLocation {
    Local(PathBuf),
}

impl SerializedWorkspaceLocation {
    pub fn path(&self) -> &Path {
        match self {
            Self::Local(path) => path.as_path(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SerializedWorkspace {
    pub id: WorkspaceId,
    pub location: SerializedWorkspaceLocation,
    pub session_id: Option<String>,
    pub window_id: Option<u64>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct SessionWorkspace {
    pub workspace_id: WorkspaceId,
    pub location: SerializedWorkspaceLocation,
    pub window_id: Option<WindowId>,
}
