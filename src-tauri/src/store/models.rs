use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    request::models::{HttpReq, ReqCfg, ReqMeta},
    space::models::SpaceReference,
};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Store {
    pub spaceref: Option<SpaceReference>,
    pub spacerefs: Vec<SpaceReference>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct ReqBuf {
    pub meta: ReqMeta,
    pub config: ReqCfg,
}

impl ReqBuf {
    pub fn from_req(req: &HttpReq) -> Self {
        let meta = ReqMeta {
            fsname: req.meta.fsname.clone(),
            name: req.meta.name.clone(),
            has_unsaved_changes: true,
        };

        let config = ReqCfg {
            method: req.config.method.clone(),
            url: req.config.url.clone(),
            headers: req.config.headers.clone(),
            parameters: req.config.parameters.clone(),
            content_type: req.config.content_type.clone(),
            body: req.config.body.clone(),
        };

        Self { meta, config }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Type)]
pub enum Theme {
    System,
    Light,
    Dark,
}
