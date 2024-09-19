use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestMeta {
    pub file_name: String,
    pub display_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestConfig {
    pub method: String,
    pub url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub meta: RequestMeta,
    pub config: RequestConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestFileMeta {
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestFile {
    pub meta: RequestFileMeta,
    pub config: RequestConfig,
}
