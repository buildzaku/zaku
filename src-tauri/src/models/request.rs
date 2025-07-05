use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    core::utils::from_indexmap,
    models::{buffer::ReqBuf, toml::ReqToml},
};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct ReqMeta {
    pub file_name: String,
    pub display_name: String,
    pub has_unsaved_changes: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct ReqCfg {
    pub method: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<(bool, String, String)>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<(bool, String, String)>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub enum ReqStatus {
    Idle,
    Pending,
    Success,
    Error,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct Req {
    pub meta: ReqMeta,
    pub config: ReqCfg,
    pub response: Option<Res>,
    pub status: ReqStatus,
}

impl Req {
    pub fn from_reqbuf(req_buf: &ReqBuf) -> Self {
        let meta = ReqMeta {
            file_name: req_buf.meta.file_name.clone(),
            display_name: req_buf.meta.display_name.clone(),
            has_unsaved_changes: true,
        };

        Self {
            meta,
            config: req_buf.config.clone(),
            status: ReqStatus::Idle,
            response: None,
        }
    }

    pub fn from_reqtoml(req_toml: &ReqToml, file_name: String) -> Self {
        let meta = ReqMeta {
            file_name,
            display_name: req_toml.meta.name.clone(),
            has_unsaved_changes: false,
        };

        let cfg = &req_toml.config;
        let config = ReqCfg {
            method: cfg.method.clone(),
            url: cfg.url.clone(),
            headers: cfg.headers.as_ref().map(from_indexmap).unwrap_or_default(),
            parameters: cfg
                .parameters
                .as_ref()
                .map(from_indexmap)
                .unwrap_or_default(),
            content_type: cfg.content_type.clone(),
            body: cfg.body.clone(),
        };

        Self {
            meta,
            config,
            status: ReqStatus::Idle,
            response: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct CreateRequestDto {
    pub parent_relative_path: String,
    pub relative_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct Res {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,

    pub data: String,
}
