use serde::{Deserialize, Serialize};

use super::request::Request;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CollectionMeta {
    pub folder_name: String,
    pub display_name: Option<String>,
    pub is_open: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Collection {
    pub meta: CollectionMeta,
    pub requests: Vec<Request>,
    pub collections: Vec<Collection>,
}
