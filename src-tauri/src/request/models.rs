use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use specta::Type;
use url::Url;

use crate::{
    space::models::SpaceCookie,
    store::models::ReqBuf,
    utils::{from_indexmap, to_indexmap},
};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct ReqMeta {
    pub file_name: String,
    pub display_name: String,
    pub has_unsaved_changes: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct ReqUrl {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct ReqCfg {
    pub method: String,
    pub url: ReqUrl,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<(bool, String, String)>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<(bool, String, String)>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReqTomlMeta {
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReqTomlConfig {
    pub method: String,
    pub url: Option<String>,
    pub headers: Option<IndexMap<String, String>>,
    pub parameters: Option<IndexMap<String, String>>,
    pub content_type: Option<String>,
    pub body: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReqToml {
    pub meta: ReqTomlMeta,
    pub config: ReqTomlConfig,
}

impl ReqToml {
    pub fn from_reqbuf(req_buf: &ReqBuf) -> Self {
        let meta = ReqTomlMeta {
            name: req_buf.meta.display_name.clone(),
        };

        let cfg = &req_buf.config;
        let config = ReqTomlConfig {
            method: cfg.method.clone(),
            url: cfg.url.raw.clone(),
            headers: to_indexmap(&cfg.headers),
            parameters: to_indexmap(&cfg.parameters),
            content_type: cfg.content_type.clone(),
            body: cfg.body.clone(),
        };

        Self { meta, config }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub enum ReqStatus {
    Idle,
    Pending,
    Success,
    Error,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct HttpReq {
    pub meta: ReqMeta,
    pub config: ReqCfg,
    pub status: ReqStatus,
    pub response: Option<HttpRes>,
}

impl HttpReq {
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
            url: {
                let raw = cfg.url.clone().unwrap_or_default();
                match Url::parse(&raw) {
                    Ok(parsed) => ReqUrl {
                        raw: Some(raw),
                        protocol: Some(parsed.scheme().to_string()),
                        host: parsed.host_str().map(|h| h.to_string()),
                        path: Some(parsed.path().to_string()),
                    },
                    Err(_) => ReqUrl {
                        raw: Some(raw),
                        protocol: None,
                        host: None,
                        path: None,
                    },
                }
            },
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
    pub parent_relpath: String,
    pub relpath: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct HttpErr {
    pub message: String,
    pub code: Option<u16>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct HttpRes {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,

    pub data: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<(String, String)>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cookies: Vec<SpaceCookie>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<u32>,
}
