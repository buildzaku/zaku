use regex::Regex;
use schemars::{JsonSchema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::{
    collections::{HashMap, HashSet},
    num::NonZeroU32,
    path::Path,
    sync::Arc,
};

use util::serde::default_true;

use crate::LanguageName;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SoftWrap {
    None,
    EditorWidth,
    Bounded,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct LanguageConfig {
    pub name: LanguageName,
    pub grammar: Option<Arc<str>>,
    #[serde(flatten)]
    pub matcher: LanguageMatcher,
    #[serde(default)]
    pub brackets: BracketPairConfig,
    #[serde(default = "auto_indent_using_last_non_empty_line_default")]
    pub auto_indent_using_last_non_empty_line: bool,
    #[serde(default)]
    pub auto_indent_on_paste: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_regex")]
    #[schemars(schema_with = "regex_json_schema")]
    pub increase_indent_pattern: Option<Regex>,
    #[serde(default, deserialize_with = "deserialize_regex")]
    #[schemars(schema_with = "regex_json_schema")]
    pub decrease_indent_pattern: Option<Regex>,
    #[serde(default)]
    pub decrease_indent_patterns: Vec<DecreaseIndentConfig>,
    #[serde(default)]
    pub autoclose_before: String,
    #[serde(default)]
    pub line_comments: Vec<Arc<str>>,
    #[serde(default)]
    pub block_comment: Option<BlockCommentConfig>,
    #[serde(default)]
    pub overrides: HashMap<String, LanguageConfigOverride>,
    #[serde(default)]
    pub word_characters: HashSet<char>,
    #[serde(default)]
    pub hard_tabs: Option<bool>,
    #[serde(default)]
    #[schemars(range(min = 1, max = 128))]
    pub tab_size: Option<NonZeroU32>,
    #[serde(default)]
    pub soft_wrap: Option<SoftWrap>,
    #[serde(default)]
    pub wrap_characters: Option<WrapCharactersConfig>,
    #[serde(default)]
    pub completion_query_characters: HashSet<char>,
    #[serde(default)]
    pub linked_edit_characters: HashSet<char>,
}

impl LanguageConfig {
    pub const FILE_NAME: &str = "config.toml";

    pub fn load(config_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let config = std::fs::read_to_string(config_path.as_ref())?;
        toml::from_str(&config).map_err(Into::into)
    }
}

impl Default for LanguageConfig {
    fn default() -> Self {
        Self {
            name: LanguageName::new_static(""),
            grammar: None,
            matcher: LanguageMatcher::default(),
            brackets: BracketPairConfig::default(),
            auto_indent_using_last_non_empty_line: auto_indent_using_last_non_empty_line_default(),
            auto_indent_on_paste: None,
            increase_indent_pattern: None,
            decrease_indent_pattern: None,
            decrease_indent_patterns: Vec::new(),
            autoclose_before: String::new(),
            line_comments: Vec::new(),
            block_comment: None,
            overrides: HashMap::default(),
            word_characters: HashSet::default(),
            hard_tabs: None,
            tab_size: None,
            soft_wrap: None,
            wrap_characters: None,
            completion_query_characters: HashSet::default(),
            linked_edit_characters: HashSet::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct DecreaseIndentConfig {
    #[serde(default, deserialize_with = "deserialize_regex")]
    #[schemars(schema_with = "regex_json_schema")]
    pub pattern: Option<Regex>,
    #[serde(default)]
    pub valid_after: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct LanguageMatcher {
    #[serde(default)]
    pub path_suffixes: Vec<String>,
    #[serde(
        default,
        serialize_with = "serialize_regex",
        deserialize_with = "deserialize_regex"
    )]
    #[schemars(schema_with = "regex_json_schema")]
    pub first_line_pattern: Option<Regex>,
}

impl Ord for LanguageMatcher {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.path_suffixes.cmp(&other.path_suffixes).then_with(|| {
            self.first_line_pattern
                .as_ref()
                .map(Regex::as_str)
                .cmp(&other.first_line_pattern.as_ref().map(Regex::as_str))
        })
    }
}

impl PartialOrd for LanguageMatcher {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for LanguageMatcher {}

impl PartialEq for LanguageMatcher {
    fn eq(&self, other: &Self) -> bool {
        self.path_suffixes == other.path_suffixes
            && self.first_line_pattern.as_ref().map(Regex::as_str)
                == other.first_line_pattern.as_ref().map(Regex::as_str)
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
pub struct BlockCommentConfig {
    pub start: Arc<str>,
    pub end: Arc<str>,
    pub prefix: Arc<str>,
    #[schemars(range(min = 1, max = 128))]
    pub tab_size: u32,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct LanguageConfigOverride {
    #[serde(default)]
    pub line_comments: Override<Vec<Arc<str>>>,
    #[serde(default)]
    pub block_comment: Override<BlockCommentConfig>,
    #[serde(skip)]
    pub disabled_bracket_ixs: Vec<u16>,
    #[serde(default)]
    pub word_characters: Override<HashSet<char>>,
    #[serde(default)]
    pub completion_query_characters: Override<HashSet<char>>,
    #[serde(default)]
    pub linked_edit_characters: Override<HashSet<char>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Override<T> {
    Remove { remove: bool },
    Set(T),
}

impl<T> Override<T> {
    pub fn as_option<'a>(this: Option<&'a Self>, original: Option<&'a T>) -> Option<&'a T> {
        match this {
            Some(Self::Set(value)) => Some(value),
            Some(Self::Remove { remove: true }) => None,
            Some(Self::Remove { remove: false }) | None => original,
        }
    }
}

impl<T> Default for Override<T> {
    fn default() -> Self {
        Override::Remove { remove: false }
    }
}

#[derive(Debug, Clone, Default, JsonSchema)]
#[schemars(with = "Vec::<BracketPairContent>")]
pub struct BracketPairConfig {
    pub pairs: Vec<BracketPair>,
    pub disabled_scopes_by_bracket_ix: Vec<Vec<String>>,
}

impl BracketPairConfig {
    pub fn is_closing_brace(&self, character: char) -> bool {
        self.pairs
            .iter()
            .any(|pair| pair.end.starts_with(character))
    }
}

impl<'de> Deserialize<'de> for BracketPairConfig {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let result = Vec::<BracketPairContent>::deserialize(deserializer)?;
        let (brackets, disabled_scopes_by_bracket_ix) = result
            .into_iter()
            .map(|entry| (entry.bracket_pair, entry.not_in))
            .unzip();

        Ok(BracketPairConfig {
            pairs: brackets,
            disabled_scopes_by_bracket_ix,
        })
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct BracketPairContent {
    #[serde(flatten)]
    pub bracket_pair: BracketPair,
    #[serde(default)]
    pub not_in: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize, JsonSchema)]
pub struct BracketPair {
    pub start: String,
    pub end: String,
    pub close: bool,
    #[serde(default = "default_true")]
    pub surround: bool,
    pub newline: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct WrapCharactersConfig {
    pub start_prefix: String,
    pub start_suffix: String,
    pub end_prefix: String,
    pub end_suffix: String,
}

pub const fn auto_indent_using_last_non_empty_line_default() -> bool {
    true
}

pub fn deserialize_regex<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<Regex>, D::Error> {
    let source = Option::<String>::deserialize(deserializer)?;
    if let Some(source) = source {
        Ok(Some(Regex::new(&source).map_err(de::Error::custom)?))
    } else {
        Ok(None)
    }
}

pub fn regex_json_schema(_: &mut SchemaGenerator) -> schemars::Schema {
    json_schema!({
        "type": "string"
    })
}

pub fn serialize_regex<S>(
    regex: &Option<Regex>,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match regex {
        Some(regex) => serializer.serialize_str(regex.as_str()),
        None => serializer.serialize_none(),
    }
}
