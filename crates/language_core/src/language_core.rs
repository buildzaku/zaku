mod highlight_map;
mod language_config;
mod language_name;
mod queries;

pub use highlight_map::{HighlightId, HighlightMap};
pub use language_config::{
    BlockCommentConfig, BracketPair, BracketPairConfig, BracketPairContent, DecreaseIndentConfig,
    LanguageConfig, LanguageConfigOverride, LanguageMatcher, Override, SoftWrap,
    WrapCharactersConfig, auto_indent_using_last_non_empty_line_default, deserialize_regex,
    regex_json_schema, serialize_regex,
};
pub use language_name::{LanguageId, LanguageName};
pub use queries::{LanguageQueries, QUERY_FILENAME_PREFIXES};
