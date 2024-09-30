use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::space::{Space, SpaceReference};

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct ZakuState {
    pub active_space: Option<Space>,
    pub space_references: Vec<SpaceReference>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct ZakuError {
    pub error: String,
    pub message: String,
}
