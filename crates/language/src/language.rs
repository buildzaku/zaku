mod buffer;
mod language_registry;
mod syntax_map;
mod text_diff;

pub use buffer::*;
pub use language_core::{
    BlockCommentConfig, BracketPair, BracketPairConfig, BracketPairContent, BracketsConfig,
    BracketsPatternConfig, DecreaseIndentConfig, Grammar, GrammarId, HighlightId, HighlightMap,
    HighlightsConfig, IndentConfig, InjectionConfig, InjectionPatternConfig, LanguageConfig,
    LanguageConfigOverride, LanguageId, LanguageMatcher, LanguageName, LanguageQueries, Override,
    OverrideConfig, OverrideEntry, QUERY_FILENAME_PREFIXES, RedactionConfig, SoftWrap,
    WrapCharactersConfig, auto_indent_using_last_non_empty_line_default, deserialize_regex,
    regex_json_schema, serialize_regex,
};
pub use language_registry::{
    AvailableLanguage, LanguageNotFound, LanguageRegistry, LoadedLanguage,
};
pub use syntax_map::{
    OwnedSyntaxLayer, ParseTimeout, SyntaxLayer, SyntaxMap, SyntaxMapCapture, SyntaxMapCaptures,
    SyntaxSnapshot, ToTreeSitterPoint,
};
pub use text::{
    Anchor, Bias, BufferId, Edit, HistoryEntry, LineEnding, OffsetUtf16, Point, PointUtf16,
    ReplicaId, Rope, Selection, SelectionGoal, TextDimension, TextSummary, ToOffset, ToOffsetUtf16,
    ToPoint, ToPointUtf16, Transaction, TransactionId, Unclipped,
};

use parking_lot::Mutex;
#[cfg(any(test, feature = "test"))]
use std::borrow::Cow;
use std::{
    fmt,
    sync::{Arc, LazyLock},
};
use tree_sitter::{Parser, QueryCursor};

use theme::SyntaxTheme;

static QUERY_CURSORS: Mutex<Vec<QueryCursor>> = Mutex::new(Vec::new());
static PARSERS: Mutex<Vec<Parser>> = Mutex::new(Vec::new());

pub static PLAIN_TEXT: LazyLock<Arc<Language>> = LazyLock::new(|| {
    Arc::new(Language::new(
        LanguageConfig {
            name: LanguageName::new_static("Plain Text"),
            soft_wrap: Some(SoftWrap::EditorWidth),
            matcher: LanguageMatcher {
                path_suffixes: vec!["txt".to_owned()],
                first_line_pattern: None,
            },
            brackets: BracketPairConfig {
                pairs: vec![
                    BracketPair {
                        start: "(".to_string(),
                        end: ")".to_string(),
                        close: true,
                        surround: true,
                        newline: false,
                    },
                    BracketPair {
                        start: "[".to_string(),
                        end: "]".to_string(),
                        close: true,
                        surround: true,
                        newline: false,
                    },
                    BracketPair {
                        start: "{".to_string(),
                        end: "}".to_string(),
                        close: true,
                        surround: true,
                        newline: false,
                    },
                    BracketPair {
                        start: "\"".to_string(),
                        end: "\"".to_string(),
                        close: true,
                        surround: true,
                        newline: false,
                    },
                    BracketPair {
                        start: "'".to_string(),
                        end: "'".to_string(),
                        close: true,
                        surround: true,
                        newline: false,
                    },
                ],
                disabled_scopes_by_bracket_ix: Vec::new(),
            },
            ..LanguageConfig::default()
        },
        None,
    ))
});

pub struct Language {
    pub(crate) id: LanguageId,
    pub(crate) config: LanguageConfig,
    pub(crate) grammar: Option<Arc<Grammar>>,
}

impl Language {
    pub fn new(config: LanguageConfig, ts_language: Option<tree_sitter::Language>) -> Self {
        Self::new_with_id(LanguageId::new(), config, ts_language)
    }

    pub fn id(&self) -> LanguageId {
        self.id
    }

    pub(crate) fn new_with_id(
        id: LanguageId,
        config: LanguageConfig,
        ts_language: Option<tree_sitter::Language>,
    ) -> Self {
        Self {
            id,
            config,
            grammar: ts_language.map(|ts_language| Arc::new(Grammar::new(ts_language))),
        }
    }

    pub fn with_queries(mut self, queries: LanguageQueries) -> anyhow::Result<Self> {
        if let Some(grammar) = self.grammar.take() {
            let grammar =
                Arc::try_unwrap(grammar).map_err(|_| anyhow::anyhow!("cannot mutate grammar"))?;
            let grammar = grammar.with_queries(queries, &mut self.config)?;
            self.grammar = Some(Arc::new(grammar));
        }
        Ok(self)
    }

    pub fn with_highlights_query(self, source: &str) -> anyhow::Result<Self> {
        self.with_grammar_query(|grammar| grammar.with_highlights_query(source))
    }

    pub fn with_brackets_query(self, source: &str) -> anyhow::Result<Self> {
        self.with_grammar_query_and_name(|grammar, name| grammar.with_brackets_query(source, name))
    }

    pub fn with_indents_query(self, source: &str) -> anyhow::Result<Self> {
        self.with_grammar_query_and_name(|grammar, name| grammar.with_indents_query(source, name))
    }

    pub fn with_injection_query(self, source: &str) -> anyhow::Result<Self> {
        self.with_grammar_query_and_name(|grammar, name| grammar.with_injection_query(source, name))
    }

    pub fn with_override_query(mut self, source: &str) -> anyhow::Result<Self> {
        if let Some(grammar) = self.grammar.take() {
            let grammar =
                Arc::try_unwrap(grammar).map_err(|_| anyhow::anyhow!("cannot mutate grammar"))?;
            let grammar = grammar.with_override_query(
                source,
                &self.config.name,
                &self.config.overrides,
                &mut self.config.brackets,
            )?;
            self.grammar = Some(Arc::new(grammar));
        }
        Ok(self)
    }

    pub fn with_redaction_query(self, source: &str) -> anyhow::Result<Self> {
        self.with_grammar_query_and_name(|grammar, name| grammar.with_redaction_query(source, name))
    }

    fn with_grammar_query(
        mut self,
        build: impl FnOnce(Grammar) -> anyhow::Result<Grammar>,
    ) -> anyhow::Result<Self> {
        if let Some(grammar) = self.grammar.take() {
            let grammar =
                Arc::try_unwrap(grammar).map_err(|_| anyhow::anyhow!("cannot mutate grammar"))?;
            self.grammar = Some(Arc::new(build(grammar)?));
        }
        Ok(self)
    }

    fn with_grammar_query_and_name(
        mut self,
        build: impl FnOnce(Grammar, &LanguageName) -> anyhow::Result<Grammar>,
    ) -> anyhow::Result<Self> {
        if let Some(grammar) = self.grammar.take() {
            let grammar =
                Arc::try_unwrap(grammar).map_err(|_| anyhow::anyhow!("cannot mutate grammar"))?;
            self.grammar = Some(Arc::new(build(grammar, &self.config.name)?));
        }
        Ok(self)
    }

    pub fn name(&self) -> LanguageName {
        self.config.name.clone()
    }

    pub fn path_suffixes(&self) -> &[String] {
        &self.config.matcher.path_suffixes
    }

    pub fn should_autoclose_before(&self, character: char) -> bool {
        character.is_whitespace() || self.config.autoclose_before.contains(character)
    }

    pub fn set_theme(&self, theme: &SyntaxTheme) {
        if let Some(grammar) = self.grammar.as_ref()
            && let Some(highlights_config) = &grammar.highlights_config
        {
            *grammar.highlight_map.lock() =
                build_highlight_map(highlights_config.query.capture_names(), theme);
        }
    }

    pub fn grammar(&self) -> Option<&Arc<Grammar>> {
        self.grammar.as_ref()
    }

    pub fn config(&self) -> &LanguageConfig {
        &self.config
    }
}

impl PartialEq for Language {
    fn eq(&self, other: &Self) -> bool {
        self.id.eq(&other.id)
    }
}

impl Eq for Language {}

impl fmt::Debug for Language {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Language")
            .field("id", &self.id)
            .field("name", &self.config.name)
            .field("grammar", &self.grammar.is_some())
            .finish()
    }
}

#[inline]
pub fn build_highlight_map(capture_names: &[&str], theme: &SyntaxTheme) -> HighlightMap {
    HighlightMap::from_ids(
        capture_names
            .iter()
            .map(|capture_name| theme.highlight_id(capture_name).map(HighlightId::new)),
    )
}

pub fn with_parser<F, R>(func: F) -> R
where
    F: FnOnce(&mut Parser) -> R,
{
    let mut parser = PARSERS.lock().pop().unwrap_or_default();
    parser.reset();
    parser
        .set_included_ranges(&[])
        .expect("included ranges should reset");
    let result = func(&mut parser);
    PARSERS.lock().push(parser);
    result
}

#[cfg(any(test, feature = "test"))]
pub fn html_lang() -> Arc<Language> {
    let language = Language::new(
        toml::from_str(include_str!("../../grammars/src/html/config.toml"))
            .expect("html language config should load"),
        Some(tree_sitter_html::LANGUAGE.into()),
    )
    .with_queries(LanguageQueries {
        highlights: Some(Cow::from(include_str!(
            "../../grammars/src/html/highlights.scm"
        ))),
        brackets: Some(Cow::from(include_str!(
            "../../grammars/src/html/brackets.scm"
        ))),
        indents: Some(Cow::from(include_str!(
            "../../grammars/src/html/indents.scm"
        ))),
        injections: Some(Cow::from(include_str!(
            "../../grammars/src/html/injections.scm"
        ))),
        overrides: Some(Cow::from(include_str!(
            "../../grammars/src/html/overrides.scm"
        ))),
        ..LanguageQueries::default()
    })
    .expect("html queries should parse");
    Arc::new(language)
}

#[cfg(any(test, feature = "test"))]
pub fn json_lang() -> Arc<Language> {
    let language = Language::new(
        toml::from_str(include_str!("../../grammars/src/json/config.toml"))
            .expect("json language config should load"),
        Some(tree_sitter_json::LANGUAGE.into()),
    )
    .with_queries(LanguageQueries {
        highlights: Some(Cow::from(include_str!(
            "../../grammars/src/json/highlights.scm"
        ))),
        brackets: Some(Cow::from(include_str!(
            "../../grammars/src/json/brackets.scm"
        ))),
        indents: Some(Cow::from(include_str!(
            "../../grammars/src/json/indents.scm"
        ))),
        overrides: Some(Cow::from(include_str!(
            "../../grammars/src/json/overrides.scm"
        ))),
        redactions: Some(Cow::from(include_str!(
            "../../grammars/src/json/redactions.scm"
        ))),
        ..LanguageQueries::default()
    })
    .expect("json queries should parse");
    Arc::new(language)
}

#[cfg(any(test, feature = "test"))]
pub fn jsonc_lang() -> Arc<Language> {
    let language = Language::new(
        toml::from_str(include_str!("../../grammars/src/jsonc/config.toml"))
            .expect("jsonc language config should load"),
        Some(tree_sitter_json::LANGUAGE.into()),
    )
    .with_queries(LanguageQueries {
        highlights: Some(Cow::from(include_str!(
            "../../grammars/src/jsonc/highlights.scm"
        ))),
        brackets: Some(Cow::from(include_str!(
            "../../grammars/src/jsonc/brackets.scm"
        ))),
        indents: Some(Cow::from(include_str!(
            "../../grammars/src/jsonc/indents.scm"
        ))),
        injections: Some(Cow::from(include_str!(
            "../../grammars/src/jsonc/injections.scm"
        ))),
        overrides: Some(Cow::from(include_str!(
            "../../grammars/src/jsonc/overrides.scm"
        ))),
        redactions: Some(Cow::from(include_str!(
            "../../grammars/src/jsonc/redactions.scm"
        ))),
    })
    .expect("jsonc queries should parse");
    Arc::new(language)
}

#[cfg(any(test, feature = "test"))]
pub fn xml_lang() -> Arc<Language> {
    let language = Language::new(
        toml::from_str(include_str!("../../grammars/src/xml/config.toml"))
            .expect("xml language config should load"),
        Some(tree_sitter_xml::LANGUAGE_XML.into()),
    )
    .with_queries(LanguageQueries {
        highlights: Some(Cow::from(include_str!(
            "../../grammars/src/xml/highlights.scm"
        ))),
        brackets: Some(Cow::from(include_str!(
            "../../grammars/src/xml/brackets.scm"
        ))),
        indents: Some(Cow::from(include_str!(
            "../../grammars/src/xml/indents.scm"
        ))),
        overrides: Some(Cow::from(include_str!(
            "../../grammars/src/xml/overrides.scm"
        ))),
        ..LanguageQueries::default()
    })
    .expect("xml queries should parse");
    Arc::new(language)
}

#[cfg(test)]
mod tests {
    use super::*;

    use gpui::HighlightStyle;
    use std::ops::ControlFlow;
    use tree_sitter::ParseOptions;

    #[test]
    fn test_highlight_map() {
        let theme = SyntaxTheme::new(
            [
                "constant",
                "constant.builtin",
                "property",
                "property.json_key",
                "string",
                "string.escape",
            ]
            .into_iter()
            .map(|name| (name.to_string(), HighlightStyle::default())),
        );

        let capture_names = &[
            "property.special",
            "constant.builtin.json",
            "string.escape.unicode",
        ];

        let map = build_highlight_map(capture_names, &theme);
        assert_eq!(
            theme.get_capture_name(map.get(0).unwrap()),
            Some("property")
        );
        assert_eq!(
            theme.get_capture_name(map.get(1).unwrap()),
            Some("constant.builtin")
        );
        assert_eq!(
            theme.get_capture_name(map.get(2).unwrap()),
            Some("string.escape")
        );
    }

    #[test]
    fn test_with_parser_resets_after_cancellation() {
        let json_language: tree_sitter::Language = tree_sitter_json::LANGUAGE.into();

        PARSERS.lock().clear();

        let repeated_entries = r#"{"a":1},"#.repeat(5_000);
        let large_input = format!(r#"[{repeated_entries}{{"a":1}}]"#);
        let small_input = "{}";

        let cancelled = with_parser(|parser| {
            parser.set_language(&json_language).unwrap();
            let bytes = large_input.as_bytes();
            let mut break_immediately = |_: &_| ControlFlow::Break(());
            parser.parse_with_options(
                &mut |offset, _| {
                    if offset < bytes.len() {
                        &bytes[offset..]
                    } else {
                        &[]
                    }
                },
                None,
                Some(ParseOptions {
                    progress_callback: Some(&mut break_immediately),
                }),
            )
        });
        assert!(
            cancelled.is_none(),
            "first parse should be cancelled by the progress callback"
        );

        let tree = with_parser(|parser| {
            let bytes = small_input.as_bytes();
            parser
                .parse_with_options(
                    &mut |offset, _| {
                        if offset < bytes.len() {
                            &bytes[offset..]
                        } else {
                            &[]
                        }
                    },
                    None,
                    None,
                )
                .expect("parse of small_input should succeed")
        });

        assert_eq!(tree.root_node().byte_range(), 0..small_input.len());
        assert_eq!(tree.root_node().kind(), "document");
        let tree_sexp = tree.root_node().to_sexp();
        assert!(
            !tree.root_node().has_error(),
            "tree should be error-free, got: {tree_sexp}"
        );
    }
}
