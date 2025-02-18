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
    pub url: Option<String>,
    pub headers: Vec<(bool, String, String)>,
    pub parameters: Vec<(bool, String, String)>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct Request {
    pub meta: RequestMeta,
    pub config: RequestConfig,
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
