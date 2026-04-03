use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestFileState {
    Parsed(RequestFile),
    Invalid(String),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct RequestFile {
    pub meta: RequestFileMeta,
    pub config: RequestFileConfig,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct RequestFileMeta {
    pub version: u32,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct RequestFileConfig {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<RequestFileHeader>,
    #[serde(default)]
    pub body: Option<RequestFileBody>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct RequestFileHeader {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct RequestFileBody {
    pub kind: RequestFileBodyKind,
    pub content: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RequestFileBodyKind {
    Text,
    Json,
}

pub(crate) fn parse_request_file(contents: &str) -> RequestFileState {
    match toml::from_str::<RequestFile>(contents) {
        Ok(request_file) => RequestFileState::Parsed(request_file),
        Err(error) => RequestFileState::Invalid(error.to_string()),
    }
}
