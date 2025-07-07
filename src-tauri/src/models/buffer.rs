use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::models::request::{HttpReq, ReqCfg, ReqMeta};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct ReqBuf {
    pub meta: ReqMeta,
    pub config: ReqCfg,
}

impl ReqBuf {
    pub fn from_req(req: &HttpReq) -> Self {
        let meta = ReqMeta {
            file_name: req.meta.file_name.clone(),
            display_name: req.meta.display_name.clone(),
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

#[derive(Clone, Debug, Serialize, Deserialize, Default, Type)]
pub struct SpaceBuf {
    pub abspath: String,
    pub requests: HashMap<String, ReqBuf>,
}
