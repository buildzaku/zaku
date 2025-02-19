use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ts_rs::TS;

use super::{collection::Collection, request::Request};

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct CreateSpaceDto {
    pub name: String,
    pub location: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct SpaceMeta {
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct SpaceConfigFile {
    pub meta: SpaceMeta,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct Space {
    pub absolute_path: String,
    pub meta: SpaceMeta,
    pub root: Collection,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct SpaceReference {
    pub path: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct SpaceBuffer {
    pub absolute_path: String,
    pub requests: HashMap<String, Request>,
}
