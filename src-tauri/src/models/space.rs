use serde::{Deserialize, Serialize};
use specta::Type;

use super::collection::Collection;

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct CreateSpaceDto {
    pub name: String,
    pub location: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct SpaceMeta {
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct SpaceConfigFile {
    pub meta: SpaceMeta,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct Space {
    pub absolute_path: String,
    pub meta: SpaceMeta,
    pub root: Collection,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct SpaceReference {
    pub path: String,
    pub name: String,
}
