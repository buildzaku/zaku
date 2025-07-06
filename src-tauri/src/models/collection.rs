use serde::{Deserialize, Serialize};
use specta::Type;

use super::request::HttpReq;

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct CollectionMeta {
    pub dir_name: String,
    pub display_name: Option<String>,
    pub is_expanded: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct Collection {
    pub meta: CollectionMeta,
    pub requests: Vec<HttpReq>,
    pub collections: Vec<Collection>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct CreateCollectionDto {
    pub parent_relpath: String,
    pub relpath: String,
}
