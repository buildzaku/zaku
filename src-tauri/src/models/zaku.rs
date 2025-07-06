use serde::{Deserialize, Serialize};
use specta::Type;

use super::space::{Space, SpaceReference};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct ZakuState {
    pub active_space: Option<Space>,
    pub spacerefs: Vec<SpaceReference>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct ZakuError {
    pub error: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ZakuStore {
    pub active_spaceref: Option<SpaceReference>,
    pub spacerefs: Vec<SpaceReference>,
}
