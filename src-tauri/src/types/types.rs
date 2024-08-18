use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct CreateWorkspaceDto {
    pub name: String,
    pub path: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct WorkspaceMeta {
    pub name: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub meta: WorkspaceMeta,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Collection {
    pub name: String,
    pub requests: Vec<Request>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Request {
    pub name: String,
}

pub struct AppState {
    pub active_workspace: Option<Workspace>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub path: String,
    pub config: WorkspaceConfig,
    pub collections: Vec<Collection>,
    pub requests: Vec<Request>,
}

#[derive(Serialize, Deserialize)]
pub struct CreateWorkspaceResult {
    pub path: String,
}

#[derive(Serialize, Deserialize)]
pub struct ZakuError {
    pub error: String,
}
