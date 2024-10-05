use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::request::Request;

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct CollectionMeta {
    pub folder_name: String,
    pub display_name: Option<String>,
    pub is_open: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct Collection {
    pub meta: CollectionMeta,
    pub requests: Vec<Request>,
    pub collections: Vec<Collection>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct CreateCollectionDto {
    pub relative_location: String,
    pub folder_relative_path: String,
    pub display_name: String,
}
