use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use specta::Type;
use url::Url;

use crate::{
    space::models::SerializedCookie,
    store::ReqBuffer,
    utils::{from_indexmap, to_indexmap},
};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct ReqMeta {
    pub fsname: String,
    pub name: String,
    pub has_unsaved_changes: bool,
    pub relpath: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct ReqUrl {
    pub raw: Option<String>,
    pub protocol: Option<String>,
    pub host: Option<String>,
    pub path: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct ReqCfg {
    pub method: String,
    pub url: ReqUrl,
    pub headers: Vec<(bool, String, String)>,
    pub parameters: Vec<(bool, String, String)>,
    pub content_type: Option<String>,
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
    pub fn from_reqbuf(req_buf: &ReqBuffer) -> Self {
        let meta = ReqTomlMeta {
            name: req_buf.meta.name.clone(),
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
    pub fn from_reqbuf(req_buf: &ReqBuffer, relpath: &Path) -> Self {
        let meta = ReqMeta {
            fsname: req_buf.meta.fsname.clone(),
            name: req_buf.meta.name.clone(),
            has_unsaved_changes: true,
            relpath: relpath.to_path_buf(),
        };

        Self {
            meta,
            config: ReqCfg {
                method: req_buf.config.method.clone(),
                url: ReqUrl {
                    raw: req_buf.config.url.raw.clone(),
                    protocol: req_buf.config.url.protocol.clone(),
                    host: req_buf.config.url.host.clone(),
                    path: req_buf.config.url.path.clone(),
                },
                headers: req_buf.config.headers.clone(),
                parameters: req_buf.config.parameters.clone(),
                content_type: req_buf.config.content_type.clone(),
                body: req_buf.config.body.clone(),
            },
            status: ReqStatus::Idle,
            response: None,
        }
    }

    pub fn from_reqtoml(req_toml: &ReqToml, relpath: &Path) -> Self {
        let fsname = relpath.file_name().unwrap().to_string_lossy().into_owned();

        let meta = ReqMeta {
            fsname,
            name: req_toml.meta.name.clone(),
            has_unsaved_changes: false,
            relpath: relpath.to_path_buf(),
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
    pub location_relpath: PathBuf,
    pub relpath: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct HttpRes {
    pub status: Option<u16>,
    pub data: String,
    pub headers: Vec<(String, String)>,
    pub cookies: Vec<SerializedCookie>,
    pub size_bytes: Option<u32>,
    pub elapsed_ms: Option<u32>,
}
