use anyhow::{Context, anyhow};
use serde::{Deserialize, Serialize};
use std::mem;
use tombi_config::{LineWidth, TomlVersion, format::FormatRules};
use tombi_formatter::{FormatOptions, Formatter};
use tombi_schema_store::SchemaStore;
use toml_edit::{Item, Table};

pub const REQUEST_FILE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestFileState {
    Parsed(RequestFile),
    Invalid(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestFile {
    pub meta: RequestFileMeta,
    pub http: RequestFileHttp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestFileMeta {
    pub version: u32,
}

impl Default for RequestFileMeta {
    fn default() -> Self {
        Self {
            version: REQUEST_FILE_VERSION,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestFileHttp {
    pub method: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<RequestFileParam>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<RequestFileHeader>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<RequestFileBody>,
}

impl Default for RequestFileHttp {
    fn default() -> Self {
        Self {
            method: "GET".to_string(),
            url: String::new(),
            params: Vec::new(),
            headers: Vec::new(),
            body: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestFileParam {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "util::serde::is_false")]
    pub disabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestFileHeader {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "util::serde::is_false")]
    pub disabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum RequestFileBody {
    Text {
        #[serde(default)]
        data: String,
    },
    Json {
        #[serde(default)]
        data: String,
    },
    Html {
        #[serde(default)]
        data: String,
    },
    Xml {
        #[serde(default)]
        data: String,
    },
    #[serde(rename = "form-urlencoded")]
    FormUrlEncoded {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        data: Vec<RequestFileFormField>,
    },
}

impl RequestFileBody {
    pub fn body_type(&self) -> RequestFileBodyType {
        match self {
            Self::Text { .. } => RequestFileBodyType::Text,
            Self::Json { .. } => RequestFileBodyType::Json,
            Self::Html { .. } => RequestFileBodyType::Html,
            Self::Xml { .. } => RequestFileBodyType::Xml,
            Self::FormUrlEncoded { .. } => RequestFileBodyType::FormUrlEncoded,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestFileFormField {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "util::serde::is_false")]
    pub disabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RequestFileBodyType {
    Text,
    Json,
    Html,
    Xml,
    #[serde(rename = "form-urlencoded")]
    FormUrlEncoded,
}

impl RequestFileBodyType {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Json => "JSON",
            Self::Html => "HTML",
            Self::Xml => "XML",
            Self::FormUrlEncoded => "URL Encoded",
        }
    }
}

pub fn serialize_request_file(request_file: &RequestFile) -> anyhow::Result<String> {
    let mut document = toml_edit::ser::to_document(request_file)?;
    promote_to_table(document.as_table_mut(), "meta")
        .context("Failed to serialize request meta")?;
    promote_to_table(document.as_table_mut(), "http")
        .context("Failed to serialize request http")?;
    Ok(document.to_string())
}

pub async fn format_request_file(contents: &str) -> anyhow::Result<String> {
    let options = FormatOptions {
        rules: Some(FormatRules {
            line_width: Some(LineWidth::try_from(100).unwrap()),
            ..Default::default()
        }),
    };
    let schema_store = SchemaStore::new_with_options(tombi_schema_store::Options {
        strict: None,
        offline: Some(true),
        cache: Some(tombi_cache::Options {
            no_cache: Some(true),
            cache_ttl: None,
        }),
    });

    Formatter::new(TomlVersion::V1_1_0, &options, None, &schema_store)
        .format(contents)
        .await
        .map_err(|diagnostics| anyhow!("failed to format request file: {diagnostics:?}"))
}

fn promote_to_table(parent: &mut Table, key: &str) -> anyhow::Result<()> {
    let Some(item) = parent.get_mut(key) else {
        return Ok(());
    };
    if item.is_table() {
        return Ok(());
    }

    let original_item = mem::take(item);
    let table = match original_item.into_table() {
        Ok(table) => table,
        Err(original_item) => {
            let item_type = original_item.type_name();
            *item = original_item;
            return Err(anyhow!("expected {key} to be table, got {item_type}"));
        }
    };
    *item = Item::Table(table);
    Ok(())
}

pub fn parse_request_file(contents: &str) -> RequestFileState {
    match toml::from_str::<RequestFile>(contents) {
        Ok(request_file) => RequestFileState::Parsed(request_file),
        Err(error) => RequestFileState::Invalid(error.to_string()),
    }
}

pub fn request_method_short_name(method: &str) -> String {
    let method = method.trim().to_ascii_uppercase();
    match method.as_str() {
        "GET" => "GET".to_string(),
        "POST" => "POST".to_string(),
        "PUT" => "PUT".to_string(),
        "PATCH" => "PTCH".to_string(),
        "DELETE" => "DEL".to_string(),
        "HEAD" => "HEAD".to_string(),
        "OPTIONS" => "OPT".to_string(),
        "QUERY" => "QURY".to_string(),
        _ => method.chars().take(4).collect(),
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

            [http]
            method = "POST"
            url = "https://api.zaku.dev/search"
            params = [{ name = "query", value = "zaku" }, { name = "debug", value = "1", disabled = true }, { name = "test", value = "1" }]
            headers = [{ name = "Content-Type", value = "application/json" }, { name = "X-Debug", value = "1", disabled = true }]
            body = {
                type = "json",
                data = """
            {
              "hello": "world"
            }"""
            }
        "#});

        assert_eq!(
            request_file,
            RequestFileState::Parsed(RequestFile {
                meta: RequestFileMeta {
                    version: REQUEST_FILE_VERSION,
                },
                http: RequestFileHttp {
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
                    body: Some(RequestFileBody::Json {
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
            },
            http: RequestFileHttp {
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
                body: Some(RequestFileBody::Json {
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

            [http]
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

    #[test]
    fn test_form_url_encoded_round_trip() {
        let request_file = RequestFile {
            meta: RequestFileMeta {
                version: REQUEST_FILE_VERSION,
            },
            http: RequestFileHttp {
                method: "POST".to_string(),
                url: "https://api.zaku.dev/form-urlencoded".to_string(),
                params: Vec::new(),
                headers: Vec::new(),
                body: Some(RequestFileBody::FormUrlEncoded {
                    data: vec![
                        RequestFileFormField {
                            name: "foo".to_string(),
                            value: "bar".to_string(),
                            disabled: false,
                        },
                        RequestFileFormField {
                            name: "foo".to_string(),
                            value: " baz".to_string(),
                            disabled: false,
                        },
                        RequestFileFormField {
                            name: "bar".to_string(),
                            value: "qux".to_string(),
                            disabled: true,
                        },
                        RequestFileFormField {
                            name: " baz ".to_string(),
                            value: "the quick brown fox\njumps over the lazy dog".to_string(),
                            disabled: false,
                        },
                        RequestFileFormField {
                            name: String::new(),
                            value: "bar".to_string(),
                            disabled: false,
                        },
                        RequestFileFormField {
                            name: "baz".to_string(),
                            value: "\t ".to_string(),
                            disabled: false,
                        },
                        RequestFileFormField {
                            name: "é".to_string(),
                            value: "\t東京".to_string(),
                            disabled: false,
                        },
                        RequestFileFormField {
                            name: "qux".to_string(),
                            value: "+&=%20".to_string(),
                            disabled: false,
                        },
                        RequestFileFormField {
                            name: "qux".to_string(),
                            value: String::new(),
                            disabled: false,
                        },
                    ],
                }),
            },
        };

        let serialized = serialize_request_file(&request_file).unwrap();
        let expected = indoc! {r#"
            [meta]
            version = 1

            [http]
            method = "POST"
            url = "https://api.zaku.dev/form-urlencoded"
            body = { type = "form-urlencoded", data = [{ name = "foo", value = "bar" }, { name = "foo", value = " baz" }, { name = "bar", value = "qux", disabled = true }, { name = " baz ", value = """
            the quick brown fox
            jumps over the lazy dog""" }, { name = "", value = "bar" }, { name = "baz", value = "\t " }, { name = "é", value = "\t東京" }, { name = "qux", value = "+&=%20" }, { name = "qux", value = "" }] }
        "#};

        assert_eq!(serialized, expected);
        assert_eq!(
            parse_request_file(&serialized),
            RequestFileState::Parsed(request_file)
        );

        let request_file = RequestFile {
            meta: RequestFileMeta {
                version: REQUEST_FILE_VERSION,
            },
            http: RequestFileHttp {
                method: "POST".to_string(),
                url: "https://api.zaku.dev/form-urlencoded".to_string(),
                params: Vec::new(),
                headers: Vec::new(),
                body: Some(RequestFileBody::FormUrlEncoded { data: Vec::new() }),
            },
        };

        let serialized = serialize_request_file(&request_file).unwrap();
        let expected = indoc! {r#"
            [meta]
            version = 1

            [http]
            method = "POST"
            url = "https://api.zaku.dev/form-urlencoded"
            body = { type = "form-urlencoded" }
        "#};

        assert_eq!(serialized, expected);
        assert_eq!(
            parse_request_file(&serialized),
            RequestFileState::Parsed(request_file)
        );
    }
}
