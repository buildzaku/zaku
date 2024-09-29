use serde::{Deserialize, Serialize};

use super::space::{Space, SpaceReference};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ZakuState {
    pub active_space: Option<Space>,
    pub space_references: Vec<SpaceReference>,
}

#[derive(Serialize, Deserialize)]
pub struct ZakuError {
    pub error: String,
    pub message: String,
}
