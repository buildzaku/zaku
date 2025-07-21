use std::{cell::RefCell, collections::HashMap, rc::Rc};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::request::models::HttpReq;

#[derive(Clone, Debug, Serialize, Deserialize, Default, Type)]
pub struct CollectionMeta {
    pub fsname: String,
    pub name: Option<String>,
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
    pub location_relpath: String,
    pub relpath: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct CreateNewCollection {
    pub parent_relpath: String,
    pub relpath: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ColName {
    pub mappings: HashMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct CollectionRcRefCell {
    pub meta: CollectionMeta,
    pub requests: Vec<HttpReq>,
    pub collections: Vec<Rc<RefCell<CollectionRcRefCell>>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SanitizedSegment {
    pub name: String,
    pub fsname: String,
}
