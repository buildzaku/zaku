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
    OwnedSyntaxLayer, ParseTimeout, SyntaxLayer, SyntaxMap, SyntaxSnapshot, ToTreeSitterPoint,
};
pub use text::{
    Anchor, Bias, Buffer as TextBuffer, BufferId, BufferSnapshot as TextBufferSnapshot, Edit,
    HistoryEntry, LineEnding, OffsetUtf16, Point, PointUtf16, ReplicaId, Rope, Selection,
    SelectionGoal, TextDimension, TextSummary, ToOffset, ToOffsetUtf16, ToPoint, ToPointUtf16,
    Transaction, TransactionId, Unclipped,
};

use parking_lot::Mutex;
use std::{
    fmt,
    sync::{Arc, LazyLock},
};
use tree_sitter::Parser;

use theme::SyntaxTheme;

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
