use anyhow::{Context, anyhow};
use serde::{Deserialize, Serialize};
use std::mem;
use toml_edit::{Item, Table};

pub const REQUEST_FILE_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestFileState {
    Parsed(RequestFile),
    Invalid(String),
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct RequestFile {
    pub meta: RequestFileMeta,
    pub config: RequestFileConfig,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct RequestFileMeta {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct RequestFileConfig {
    pub method: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<RequestFileParam>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<RequestFileHeader>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<RequestFileBody>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct RequestFileParam {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "util::serde::is_false")]
    pub disabled: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct RequestFileHeader {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "util::serde::is_false")]
    pub disabled: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct RequestFileBody {
    pub r#type: RequestFileBodyType,
    #[serde(default)]
    pub data: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RequestFileBodyType {
    Text,
    Json,
    Xml,
}

pub fn serialize_request_file(request_file: &RequestFile) -> anyhow::Result<String> {
    let mut document = toml_edit::ser::to_document(request_file)?;
    promote_to_table(document.as_table_mut(), "meta")
        .context("Failed to serialize request meta")?;
    promote_to_table(document.as_table_mut(), "config")
        .context("Failed to serialize request config")?;
    Ok(document.to_string())
}

fn promote_to_table(parent: &mut Table, key: &str) -> anyhow::Result<()> {
    let Some(item) = parent.get_mut(key) else {
        return Ok(());
    };
    if item.is_table() {
        return Ok(());
    }

    let item_type = item.type_name();
    let table = mem::take(item)
        .into_table()
        .map_err(|_| anyhow!("Expected {key} to be table, got {item_type}"))?;
    *item = Item::Table(table);
    Ok(())
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
    use pretty_assertions::assert_eq;

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
            body = { type = "json", data = '''
            {
              "hello": "world"
            }''' }
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
                            }"#}
                        .to_string(),
                    }),
                },
            })
        );
    }

    #[test]
    fn test_serialize_request_file() {
        let request_file = RequestFile {
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
                        }"#}
                    .to_string(),
                }),
            },
        };

        let serialized = serialize_request_file(&request_file).unwrap();
        let expected = indoc! {r#"
            [meta]
            version = 1
            name = "Search"

            [config]
            method = "POST"
            url = "https://api.zaku.dev/search"
            params = [{ name = "query", value = "zaku" }, { name = "debug", value = "1", disabled = true }, { name = "test", value = "1" }]
            headers = [{ name = "Content-Type", value = "application/json" }, { name = "X-Debug", value = "1", disabled = true }]
            body = { type = "json", data = """
            {
              "hello": "world"
            }""" }
        "#};

        assert_eq!(serialized, expected);
        assert_eq!(
            parse_request_file(&serialized),
            RequestFileState::Parsed(request_file)
        );
    }
}
