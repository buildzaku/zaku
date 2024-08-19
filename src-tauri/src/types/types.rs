use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateSpaceDto {
    pub name: String,
    pub path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpaceMeta {
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpaceConfig {
    pub meta: SpaceMeta,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Collection {
    pub name: String,
    pub requests: Vec<Request>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub name: String,
}

pub struct AppState {
    pub active_space: Option<Space>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Space {
    pub path: String,
    pub config: SpaceConfig,
    pub collections: Vec<Collection>,
    pub requests: Vec<Request>,
}

#[derive(Serialize, Deserialize)]
pub struct CreateSpaceResult {
    pub path: String,
}

#[derive(Serialize, Deserialize)]
pub struct ZakuError {
    pub error: String,
}
