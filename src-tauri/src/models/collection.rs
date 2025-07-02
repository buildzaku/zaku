use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::request::Req;

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct CollectionMeta {
    pub dir_name: String,
    pub display_name: Option<String>,
    pub is_expanded: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct Collection {
    pub meta: CollectionMeta,
    pub requests: Vec<Req>,
    pub collections: Vec<Collection>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct CreateCollectionDto {
    pub parent_relative_path: String,
    pub relative_path: String,
}
