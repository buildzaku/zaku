use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::models::request::{Req, ReqCfg, ReqMeta};

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct ReqBuf {
    pub meta: ReqMeta,
    pub config: ReqCfg,
}

impl From<&Req> for ReqBuf {
    fn from(req: &Req) -> Self {
        Self {
            meta: ReqMeta {
                file_name: req.meta.file_name.clone(),
                display_name: req.meta.display_name.clone(),
                has_unsaved_changes: true,
            },
            config: ReqCfg {
                method: req.config.method.clone(),
                url: req.config.url.clone(),
                headers: req.config.headers.clone(),
                parameters: req.config.parameters.clone(),
                content_type: req.config.content_type.clone(),
                body: req.config.body.clone(),
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct SpaceBuf {
    pub absolute_path: String,
    pub requests: HashMap<String, ReqBuf>,
}
