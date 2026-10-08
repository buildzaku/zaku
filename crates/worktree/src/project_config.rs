use anyhow::anyhow;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{collections::HashMap, hash::BuildHasher};

use crate::SCHEMA_VERSION;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectFile {
    pub meta: ConfigFileMeta,
    #[serde(default)]
    pub request: RequestSection,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderFile {
    pub meta: ConfigFileMeta,
    #[serde(default)]
    pub request: RequestSection,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentFile {
    pub meta: ConfigFileMeta,
    #[serde(default)]
    pub environment: EnvironmentSection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigFileMeta {
    pub version: u32,
}

impl Default for ConfigFileMeta {
    fn default() -> Self {
        Self {
            version: SCHEMA_VERSION,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestSection {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<Variable>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentSection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<EnvironmentColor>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<Variable>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Variable {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "util::serde::is_false")]
    pub disabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnvironmentColor {
    Accent,
    Info,
    Success,
    Warning,
    Error,
    Hint,
}

pub fn parse_config_file<T: DeserializeOwned>(contents: &str) -> anyhow::Result<T> {
    toml::from_str(contents).map_err(|error| {
        let message = error.message();
        // Display includes a source snippet; a single line reads better in toasts and logs.
        match error.span().and_then(|span| contents.get(..span.start)) {
            Some(contents_before_error) => {
                let line = contents_before_error.matches('\n').count() + 1;
                let column = contents_before_error
                    .chars()
                    .rev()
                    .take_while(|character| *character != '\n')
                    .count()
                    + 1;
                anyhow!("{message} at line {line} column {column}")
            }
            None => anyhow!("{message}"),
        }
    })
}

pub fn substitute_variables_in_str(
    text: &str,
    variables: &HashMap<String, String, impl BuildHasher>,
) -> String {
    fn inner<'a>(
        text: &str,
        variables: &'a HashMap<String, String, impl BuildHasher>,
        resolving_names: &mut Vec<&'a str>,
    ) -> String {
        let mut substituted_text = String::with_capacity(text.len());
        let mut rest = text;
        while let Some((before_braces, after_braces)) = rest.split_once("{{") {
            substituted_text.push_str(before_braces);
            let Some((name, remainder)) = after_braces
                .split_once('}')
                .and_then(|(name, after_name)| Some((name, after_name.strip_prefix('}')?)))
                .filter(|(name, _)| !name.is_empty())
            else {
                substituted_text.push_str("{{");
                rest = after_braces;
                continue;
            };

            match variables.get_key_value(name) {
                // Self-referencing variables stay literal instead of recursing forever.
                Some((name, value)) if !resolving_names.contains(&name.as_str()) => {
                    resolving_names.push(name);
                    substituted_text.push_str(&inner(value, variables, resolving_names));
                    resolving_names.pop();
                }
                _ => {
                    substituted_text.push_str("{{");
                    substituted_text.push_str(name);
                    substituted_text.push_str("}}");
                }
            }
            rest = remainder;
        }
        substituted_text.push_str(rest);
        substituted_text
    }

    inner(text, variables, &mut Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    use indoc::indoc;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_parse_project_file() {
        let project_file = parse_config_file::<ProjectFile>(indoc! {r#"
            [meta]
            version = 1

            [request]
            variables = [{ name = "base_url", value = "https://api.zaku.dev" }]
        "#})
        .unwrap();
        assert_eq!(
            project_file,
            ProjectFile {
                meta: ConfigFileMeta {
                    version: SCHEMA_VERSION,
                },
                request: RequestSection {
                    variables: vec![Variable {
                        name: "base_url".to_string(),
                        value: "https://api.zaku.dev".to_string(),
                        disabled: false,
                    }],
                },
            }
        );

        let project_file = parse_config_file::<ProjectFile>(indoc! {"
            [meta]
            version = 1
        "})
        .unwrap();
        assert_eq!(project_file, ProjectFile::default());
    }

    #[test]
    fn test_parse_folder_file() {
        let folder_file = parse_config_file::<FolderFile>(indoc! {r#"
            [meta]
            version = 1

            [request]
            variables = [{ name = "user_id", value = "1" }]
        "#})
        .unwrap();
        assert_eq!(
            folder_file,
            FolderFile {
                meta: ConfigFileMeta {
                    version: SCHEMA_VERSION,
                },
                request: RequestSection {
                    variables: vec![Variable {
                        name: "user_id".to_string(),
                        value: "1".to_string(),
                        disabled: false,
                    }],
                },
            }
        );

        let folder_file = parse_config_file::<FolderFile>(indoc! {"
            [meta]
            version = 1
        "})
        .unwrap();
        assert_eq!(folder_file, FolderFile::default());
    }

    #[test]
    fn test_parse_environment_file() {
        let environment_file = parse_config_file::<EnvironmentFile>(indoc! {r#"
            [meta]
            version = 1

            [environment]
            color = "accent"
            variables = [
              { name = "base_url", value = "http://localhost:8000" },
              { name = "channel", value = "beta", disabled = true }
            ]
        "#})
        .unwrap();
        assert_eq!(
            environment_file,
            EnvironmentFile {
                meta: ConfigFileMeta {
                    version: SCHEMA_VERSION,
                },
                environment: EnvironmentSection {
                    color: Some(EnvironmentColor::Accent),
                    variables: vec![
                        Variable {
                            name: "base_url".to_string(),
                            value: "http://localhost:8000".to_string(),
                            disabled: false,
                        },
                        Variable {
                            name: "channel".to_string(),
                            value: "beta".to_string(),
                            disabled: true,
                        },
                    ],
                },
            }
        );

        let environment_file = parse_config_file::<EnvironmentFile>(indoc! {"
            [meta]
            version = 1
        "})
        .unwrap();
        assert_eq!(environment_file, EnvironmentFile::default());
    }

    #[test]
    fn test_parse_config_file_reports_error_location() {
        let error = parse_config_file::<ProjectFile>(indoc! {r#"
            [meta]
            version = 1

            [request]
            variables = [{ name = "city", value = "Zürich", disabled = "true" }]
        "#})
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            r#"invalid type: string "true", expected a boolean at line 5 column 60"#
        );

        let error = parse_config_file::<ProjectFile>("").unwrap_err();
        assert_eq!(error.to_string(), "missing field `meta` at line 1 column 1");
    }

    #[test]
    fn test_substitute_variables_in_str() {
        let variables = [
            ("host", "api.zaku.dev"),
            ("base_url", "https://{{host}}"),
            ("user_id", "1"),
            ("open_braces", "{{"),
            ("foo", "{{foo}}"),
            ("bar", "{{baz}}"),
            ("baz", "{{bar}}"),
        ]
        .into_iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect::<HashMap<_, _>>();

        assert_eq!(
            substitute_variables_in_str("{{base_url}}/users/{{user_id}}", &variables),
            "https://api.zaku.dev/users/1"
        );
        assert_eq!(
            substitute_variables_in_str("{{base_url}}/hosts/{{host}}", &variables),
            "https://api.zaku.dev/hosts/api.zaku.dev"
        );

        assert_eq!(
            substitute_variables_in_str("{{foo}}", &variables),
            "{{foo}}"
        );
        assert_eq!(
            substitute_variables_in_str("{{bar}}", &variables),
            "{{bar}}"
        );
        assert_eq!(
            substitute_variables_in_str("Bearer {{token}}", &variables),
            "Bearer {{token}}"
        );
        assert_eq!(
            substitute_variables_in_str("{{ host }}", &variables),
            "{{ host }}"
        );

        assert_eq!(
            substitute_variables_in_str("https://{{host", &variables),
            "https://{{host"
        );
        assert_eq!(
            substitute_variables_in_str("{{host}/{{user_id}}", &variables),
            "{{host}/1"
        );
        assert_eq!(
            substitute_variables_in_str("{{}}/{{host}}", &variables),
            "{{}}/api.zaku.dev"
        );

        assert_eq!(
            substitute_variables_in_str("{{open_braces}}host}}", &variables),
            "{{host}}"
        );
    }
}
