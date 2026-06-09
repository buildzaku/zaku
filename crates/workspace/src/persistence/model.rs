use gpui::WindowId;
use std::path::PathBuf;
use uuid::Uuid;

use super::SerializedWindowBounds;
use crate::WorkspaceId;

#[derive(Clone, Debug, PartialEq)]
pub struct SerializedWorkspace {
    pub id: WorkspaceId,
    pub location: PathBuf,
    pub window_bounds: Option<SerializedWindowBounds>,
    pub display: Option<Uuid>,
    pub session_id: Option<String>,
    pub window_id: Option<u64>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct SessionWorkspace {
    pub workspace_id: WorkspaceId,
    pub location: PathBuf,
    pub window_id: Option<WindowId>,
}
