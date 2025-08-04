use std::{cell::RefCell, path::PathBuf, rc::Rc};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::request::models::HttpReq;

#[derive(Clone, Debug, Serialize, Deserialize, Default, Type)]
pub struct CollectionMeta {
    pub fsname: String,
    pub name: Option<String>,
    pub is_expanded: bool,
    pub relpath: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, Type)]
pub struct Collection {
    pub meta: CollectionMeta,
    pub requests: Vec<HttpReq>,
    pub collections: Vec<Collection>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct CreateCollectionDto {
    pub location_relpath: PathBuf,
    pub relpath: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct CreateNewCollection {
    pub location_relpath: PathBuf,
    pub relpath: PathBuf,
}

#[derive(Clone, Debug)]
pub struct CollectionRcRefCell {
    pub meta: CollectionMeta,
    pub requests: Vec<HttpReq>,
    pub collections: Vec<Rc<RefCell<CollectionRcRefCell>>>,
}
