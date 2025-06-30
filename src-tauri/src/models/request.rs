use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct RequestMeta {
    pub file_name: String,
    pub display_name: String,
    pub has_unsaved_changes: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct RequestConfig {
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
pub struct Request {
    pub meta: RequestMeta,
    pub config: RequestConfig,
    pub response: Option<Response>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]

pub struct RequestFileMeta {
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestFileConfig {
    pub method: String,
    pub url: Option<String>,
    pub headers: Option<IndexMap<String, String>>,
    pub parameters: Option<IndexMap<String, String>>,
    pub content_type: Option<String>,
    pub body: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestFile {
    pub meta: RequestFileMeta,
    pub config: RequestFileConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct CreateRequestDto {
    pub parent_relative_path: String,
    pub relative_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct Response {
    pub status: u16,
    pub data: String,
}
