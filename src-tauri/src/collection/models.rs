use serde::{Deserialize, Serialize};
use specta::Type;

use crate::request::models::HttpReq;

#[derive(Clone, Debug, Serialize, Deserialize, Default, Type)]
pub struct CollectionMeta {
    pub dir_name: String,
    pub display_name: Option<String>,
    pub is_expanded: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, Type)]
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

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct CreateNewCollection {
    pub parent_relpath: String,
    pub relpath: String,
}
