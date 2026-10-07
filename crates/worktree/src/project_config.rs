use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::SCHEMA_VERSION;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectFile {
    pub meta: ConfigFileMeta,
    #[serde(default)]
    pub project: VariablesSection,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderFile {
    pub meta: ConfigFileMeta,
    #[serde(default)]
    pub folder: VariablesSection,
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
pub struct VariablesSection {
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
    Ok(toml::from_str(contents)?)
}
