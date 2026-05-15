use serde::Deserialize;

pub const REQUEST_FILE_VERSION: u32 = 1;

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
    pub params: Vec<RequestFileParam>,
    #[serde(default)]
    pub headers: Vec<RequestFileHeader>,
    #[serde(default)]
    pub body: Option<RequestFileBody>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct RequestFileParam {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct RequestFileHeader {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct RequestFileBody {
    pub r#type: RequestFileBodyType,
    #[serde(default)]
    pub data: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RequestFileBodyType {
    Text,
    Json,
    Xml,
}

pub(crate) fn parse_request_file(contents: &str) -> RequestFileState {
    match toml::from_str::<RequestFile>(contents) {
        Ok(request_file) => RequestFileState::Parsed(request_file),
        Err(error) => RequestFileState::Invalid(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use indoc::indoc;

    #[test]
    fn test_parse_request_file() {
        let request_file = parse_request_file(indoc! {r#"
            [meta]
            version = 1
            name = "Search"

            [config]
            method = "POST"
            url = "https://api.zaku.dev/search"
            params = [
                { name = "query", value = "zaku" },
                { name = "debug", value = "1", disabled = true },
                { name = "test", value = "1", disabled = false },
            ]
            headers = [
                { name = "Content-Type", value = "application/json" },
                { name = "X-Debug", value = "1", disabled = true },
            ]

            [config.body]
            type = "json"
            data = '''
            {
              "hello": "world"
            }
            '''
        "#});

        assert_eq!(
            request_file,
            RequestFileState::Parsed(RequestFile {
                meta: RequestFileMeta {
                    version: REQUEST_FILE_VERSION,
                    name: Some("Search".to_string()),
                },
                config: RequestFileConfig {
                    method: "POST".to_string(),
                    url: "https://api.zaku.dev/search".to_string(),
                    params: vec![
                        RequestFileParam {
                            name: "query".to_string(),
                            value: "zaku".to_string(),
                            disabled: false,
                        },
                        RequestFileParam {
                            name: "debug".to_string(),
                            value: "1".to_string(),
                            disabled: true,
                        },
                        RequestFileParam {
                            name: "test".to_string(),
                            value: "1".to_string(),
                            disabled: false,
                        },
                    ],
                    headers: vec![
                        RequestFileHeader {
                            name: "Content-Type".to_string(),
                            value: "application/json".to_string(),
                            disabled: false,
                        },
                        RequestFileHeader {
                            name: "X-Debug".to_string(),
                            value: "1".to_string(),
                            disabled: true,
                        },
                    ],
                    body: Some(RequestFileBody {
                        r#type: RequestFileBodyType::Json,
                        data: indoc! {r#"
                            {
                              "hello": "world"
                            }
                        "#}
                        .to_string(),
                    }),
                },
            })
        );
    }
}
