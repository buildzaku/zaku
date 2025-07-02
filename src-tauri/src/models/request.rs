use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::models::buffer::ReqBuf;

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct ReqMeta {
    pub file_name: String,
    pub display_name: String,
    pub has_unsaved_changes: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct ReqCfg {
    pub method: String,

    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    #[ts(optional, as = "Option<_>")]
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<(bool, String, String)>,

    #[ts(optional, as = "Option<_>")]
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<(bool, String, String)>,

    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,

    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub enum ReqStatus {
    Idle,
    Pending,
    Success,
    Error,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct Req {
    pub meta: ReqMeta,
    pub config: ReqCfg,
    pub response: Option<Res>,
    pub status: ReqStatus,
}

impl From<&ReqBuf> for Req {
    fn from(req_buf: &ReqBuf) -> Self {
        Self {
            meta: ReqMeta {
                file_name: req_buf.meta.file_name.clone(),
                display_name: req_buf.meta.display_name.clone(),
                has_unsaved_changes: true,
            },
            config: req_buf.config.clone(),
            response: None,
            status: ReqStatus::Idle,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct CreateRequestDto {
    pub parent_relative_path: String,
    pub relative_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct Res {
    #[ts(optional)]
    pub status: Option<u16>,

    pub data: String,
}
