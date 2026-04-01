use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestState {
    Parsed(RequestFile),
    Invalid(String),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct RequestFile {
    pub meta: RequestMeta,
    pub config: RequestConfig,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct RequestMeta {
    pub version: u32,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct RequestConfig {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<RequestHeader>,
    #[serde(default)]
    pub body: Option<RequestBody>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct RequestHeader {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct RequestBody {
    pub kind: RequestBodyKind,
    pub content: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RequestBodyKind {
    Text,
    Json,
}

pub(crate) fn parse_request_file(contents: &str) -> RequestState {
    match toml::from_str::<RequestFile>(contents) {
        Ok(request_file) => RequestState::Parsed(request_file),
        Err(error) => RequestState::Invalid(error.to_string()),
    }
}
