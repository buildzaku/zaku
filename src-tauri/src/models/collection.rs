use serde::{Deserialize, Serialize};
use specta::Type;

use super::request::Req;

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct CollectionMeta {
    pub dir_name: String,
    pub display_name: Option<String>,
    pub is_expanded: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct Collection {
    pub meta: CollectionMeta,
    pub requests: Vec<Req>,
    pub collections: Vec<Collection>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct CreateCollectionDto {
    pub parent_relative_path: String,
    pub relative_path: String,
}
