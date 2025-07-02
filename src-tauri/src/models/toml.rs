use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::models::buffer::ReqBuf;

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

impl From<&ReqBuf> for ReqToml {
    fn from(req_buf: &ReqBuf) -> Self {
        let meta = ReqTomlMeta {
            name: req_buf.meta.display_name.clone(),
        };

        fn to_indexmap(items: &[(bool, String, String)]) -> Option<IndexMap<String, String>> {
            if items.is_empty() {
                return None;
            }
            Some(
                items
                    .iter()
                    .map(|(included, key, value)| {
                        let key = if *included { key } else { &format!("!{}", key) };
                        (key.clone(), value.clone())
                    })
                    .collect(),
            )
        }

        let cfg = &req_buf.config;
        let config = ReqTomlConfig {
            method: cfg.method.clone(),
            url: cfg.url.clone(),
            headers: to_indexmap(&cfg.headers),
            parameters: to_indexmap(&cfg.parameters),
            content_type: cfg.content_type.clone(),
            body: cfg.body.clone(),
        };

        Self { meta, config }
    }
}
