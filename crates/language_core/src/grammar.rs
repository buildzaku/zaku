use anyhow::Context;
use gpui::SharedString;
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    sync::atomic::{AtomicUsize, Ordering},
};
use tree_sitter::Query;

use crate::{
    BracketPairConfig, HighlightId, HighlightMap, LanguageConfig, LanguageConfigOverride,
    LanguageName, LanguageQueries,
};

pub static NEXT_GRAMMAR_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GrammarId(pub usize);

impl GrammarId {
    pub fn new() -> Self {
        Self(NEXT_GRAMMAR_ID.fetch_add(1, Ordering::SeqCst))
    }
}

impl Default for GrammarId {
    fn default() -> Self {
        Self::new()
    }
}

pub struct HighlightsConfig {
    pub query: Query,
    pub identifier_capture_indices: Vec<u32>,
}

pub struct IndentConfig {
    pub query: Query,
    pub indent_capture_ix: u32,
    pub start_capture_ix: Option<u32>,
    pub end_capture_ix: Option<u32>,
    pub outdent_capture_ix: Option<u32>,
    pub suffixed_start_captures: HashMap<u32, SharedString>,
}

pub struct InjectionConfig {
    pub query: Query,
    pub content_capture_ix: u32,
    pub language_capture_ix: Option<u32>,
    pub patterns: Vec<InjectionPatternConfig>,
}

pub struct RedactionConfig {
    pub query: Query,
    pub redaction_capture_ix: u32,
}

pub struct OverrideConfig {
    pub query: Query,
    pub values: HashMap<u32, OverrideEntry>,
}

#[derive(Debug)]
pub struct OverrideEntry {
    pub name: String,
    pub range_is_inclusive: bool,
    pub value: LanguageConfigOverride,
}

#[derive(Debug, Clone, Default)]
pub struct InjectionPatternConfig {
    pub language: Option<Box<str>>,
    pub combined: bool,
}

#[derive(Debug)]
pub struct BracketsConfig {
    pub query: Query,
    pub open_capture_ix: u32,
    pub close_capture_ix: u32,
    pub patterns: Vec<BracketsPatternConfig>,
}

#[derive(Debug, Clone, Default)]
pub struct BracketsPatternConfig {
    pub newline_only: bool,
    pub rainbow_exclude: bool,
}

enum Capture<'a> {
    Required(&'static str, &'a mut u32),
    Optional(&'static str, &'a mut Option<u32>),
}

fn populate_capture_indices(
    query: &Query,
    language_name: &LanguageName,
    query_type: &str,
    expected_prefixes: &[&str],
    captures: &mut [Capture<'_>],
) -> anyhow::Result<bool> {
    let mut found_required_indices = Vec::new();
    'outer: for (capture_index, name) in query.capture_names().iter().enumerate() {
        for (required_index, capture) in captures.iter_mut().enumerate() {
            match capture {
                Capture::Required(capture_name, index) if capture_name == name => {
                    **index =
                        u32::try_from(capture_index).context("capture index exceeds u32 range")?;
                    found_required_indices.push(required_index);
                    continue 'outer;
                }
                Capture::Optional(capture_name, index) if capture_name == name => {
                    **index = Some(
                        u32::try_from(capture_index).context("capture index exceeds u32 range")?,
                    );
                    continue 'outer;
                }
                _ => {}
            }
        }
        if !name.starts_with('_')
            && !expected_prefixes
                .iter()
                .any(|prefix| name.starts_with(prefix))
        {
            log::warn!(
                "Unrecognized capture name '{name}' in {language_name} {query_type} TreeSitter query \
                (suppress this warning by prefixing with '_')",
            );
        }
    }
    let mut missing_required_captures = Vec::new();
    for (capture_index, capture) in captures.iter().enumerate() {
        if let Capture::Required(capture_name, _) = capture
            && !found_required_indices.contains(&capture_index)
        {
            missing_required_captures.push(*capture_name);
        }
    }
    let success = missing_required_captures.is_empty();
    if !success {
        log::error!(
            "Missing required capture(s) in {} {} TreeSitter query: {}",
            language_name,
            query_type,
            missing_required_captures.join(", ")
        );
    }
    Ok(success)
}

pub struct Grammar {
    id: GrammarId,
    pub ts_language: tree_sitter::Language,
    pub error_query: Option<Query>,
    pub highlights_config: Option<HighlightsConfig>,
    pub brackets_config: Option<BracketsConfig>,
    pub redactions_config: Option<RedactionConfig>,
    pub indents_config: Option<IndentConfig>,
    pub injection_config: Option<InjectionConfig>,
    pub override_config: Option<OverrideConfig>,
    pub highlight_map: Mutex<HighlightMap>,
}

impl Grammar {
    pub fn new(ts_language: tree_sitter::Language) -> Self {
        Self {
            id: GrammarId::new(),
            highlights_config: None,
            brackets_config: None,
            indents_config: None,
            injection_config: None,
            override_config: None,
            redactions_config: None,
            error_query: Some(
                Query::new(&ts_language, "(ERROR) @error").expect("error query should compile"),
            ),
            ts_language,
            highlight_map: Mutex::default(),
        }
    }

    pub fn id(&self) -> GrammarId {
        self.id
    }

    pub fn highlight_map(&self) -> HighlightMap {
        self.highlight_map.lock().clone()
    }

    pub fn highlight_id_for_name(&self, name: &str) -> Option<HighlightId> {
        self.highlights_config
            .as_ref()?
            .query
            .capture_index_for_name(name)
            .and_then(|capture_id| self.highlight_map.lock().get(capture_id))
    }

    pub fn with_queries(
        mut self,
        queries: LanguageQueries,
        config: &mut LanguageConfig,
    ) -> anyhow::Result<Self> {
        let name = &config.name;
        if let Some(query) = queries.highlights {
            self = self
                .with_highlights_query(query.as_ref())
                .context("Error loading highlights query")?;
        }
        if let Some(query) = queries.brackets {
            self = self
                .with_brackets_query(query.as_ref(), name)
                .context("Error loading brackets query")?;
        }
        if let Some(query) = queries.indents {
            self = self
                .with_indents_query(query.as_ref(), name)
                .context("Error loading indents query")?;
        }
        if let Some(query) = queries.injections {
            self = self
                .with_injection_query(query.as_ref(), name)
                .context("Error loading injection query")?;
        }
        if let Some(query) = queries.overrides {
            self = self
                .with_override_query(
                    query.as_ref(),
                    name,
                    &config.overrides,
                    &mut config.brackets,
                )
                .context("Error loading override query")?;
        }
        if let Some(query) = queries.redactions {
            self = self
                .with_redaction_query(query.as_ref(), name)
                .context("Error loading redaction query")?;
        }
        Ok(self)
    }

    pub fn with_highlights_query(mut self, source: &str) -> anyhow::Result<Self> {
        let query = Query::new(&self.ts_language, source)?;

        let mut identifier_capture_indices = Vec::new();
        for name in [
            "variable",
            "constant",
            "constructor",
            "function",
            "function.method",
            "function.method.call",
            "function.special",
            "property",
            "type",
            "type.interface",
        ] {
            identifier_capture_indices.extend(query.capture_index_for_name(name));
        }

        self.highlights_config = Some(HighlightsConfig {
            query,
            identifier_capture_indices,
        });

        Ok(self)
    }

    pub fn with_brackets_query(
        mut self,
        source: &str,
        language_name: &LanguageName,
    ) -> anyhow::Result<Self> {
        let query = Query::new(&self.ts_language, source)?;
        let mut open_capture_index = 0;
        let mut close_capture_index = 0;
        let has_required_captures = populate_capture_indices(
            &query,
            language_name,
            "brackets",
            &[],
            &mut [
                Capture::Required("open", &mut open_capture_index),
                Capture::Required("close", &mut close_capture_index),
            ],
        )?;

        if has_required_captures {
            let patterns = (0..query.pattern_count())
                .map(|pattern_index| {
                    let mut config = BracketsPatternConfig::default();
                    for setting in query.property_settings(pattern_index) {
                        let setting_key = setting.key.as_ref();
                        if setting_key == "newline.only" {
                            config.newline_only = true;
                        }
                        if setting_key == "rainbow.exclude" {
                            config.rainbow_exclude = true;
                        }
                    }
                    config
                })
                .collect();
            self.brackets_config = Some(BracketsConfig {
                query,
                open_capture_ix: open_capture_index,
                close_capture_ix: close_capture_index,
                patterns,
            });
        }
        Ok(self)
    }

    pub fn with_indents_query(
        mut self,
        source: &str,
        language_name: &LanguageName,
    ) -> anyhow::Result<Self> {
        let query = Query::new(&self.ts_language, source)?;
        let mut indent_capture_index = 0;
        let mut start_capture_index = None;
        let mut end_capture_index = None;
        let mut outdent_capture_index = None;
        let has_required_captures = populate_capture_indices(
            &query,
            language_name,
            "indents",
            &["start."],
            &mut [
                Capture::Required("indent", &mut indent_capture_index),
                Capture::Optional("start", &mut start_capture_index),
                Capture::Optional("end", &mut end_capture_index),
                Capture::Optional("outdent", &mut outdent_capture_index),
            ],
        )?;

        if has_required_captures {
            let mut suffixed_start_captures = HashMap::default();
            for (capture_index, name) in query.capture_names().iter().enumerate() {
                if let Some(suffix) = name.strip_prefix("start.") {
                    suffixed_start_captures.insert(
                        u32::try_from(capture_index).context("capture index exceeds u32 range")?,
                        suffix.to_owned().into(),
                    );
                }
            }

            self.indents_config = Some(IndentConfig {
                query,
                indent_capture_ix: indent_capture_index,
                start_capture_ix: start_capture_index,
                end_capture_ix: end_capture_index,
                outdent_capture_ix: outdent_capture_index,
                suffixed_start_captures,
            });
        }
        Ok(self)
    }

    pub fn with_injection_query(
        mut self,
        source: &str,
        language_name: &LanguageName,
    ) -> anyhow::Result<Self> {
        let query = Query::new(&self.ts_language, source)?;
        let mut language_capture_index = None;
        let mut prefixed_language_capture_index = None;
        let mut content_capture_index = None;
        let mut prefixed_content_capture_index = None;
        let has_required_captures = populate_capture_indices(
            &query,
            language_name,
            "injections",
            &[],
            &mut [
                Capture::Optional("language", &mut language_capture_index),
                Capture::Optional("injection.language", &mut prefixed_language_capture_index),
                Capture::Optional("content", &mut content_capture_index),
                Capture::Optional("injection.content", &mut prefixed_content_capture_index),
            ],
        )?;

        if has_required_captures {
            language_capture_index = match (language_capture_index, prefixed_language_capture_index)
            {
                (None, Some(index)) => Some(index),
                (Some(_), Some(_)) => {
                    anyhow::bail!("Both language and injection.language captures are present");
                }
                _ => language_capture_index,
            };
            content_capture_index = match (content_capture_index, prefixed_content_capture_index) {
                (None, Some(index)) => Some(index),
                (Some(_), Some(_)) => {
                    anyhow::bail!("Both content and injection.content captures are present");
                }
                _ => content_capture_index,
            };
            let patterns = (0..query.pattern_count())
                .map(|pattern_index| {
                    let mut config = InjectionPatternConfig::default();
                    for setting in query.property_settings(pattern_index) {
                        match setting.key.as_ref() {
                            "language" | "injection.language" => {
                                config.language.clone_from(&setting.value);
                            }
                            "combined" | "injection.combined" => {
                                config.combined = true;
                            }
                            _ => {}
                        }
                    }
                    config
                })
                .collect();
            if let Some(content_capture_index) = content_capture_index {
                self.injection_config = Some(InjectionConfig {
                    query,
                    language_capture_ix: language_capture_index,
                    content_capture_ix: content_capture_index,
                    patterns,
                });
            } else {
                log::error!(
                    "Missing required capture in injections {language_name} TreeSitter query: \
                    content or injection.content",
                );
            }
        }
        Ok(self)
    }

    pub fn with_override_query(
        mut self,
        source: &str,
        language_name: &LanguageName,
        overrides: &HashMap<String, LanguageConfigOverride>,
        brackets: &mut BracketPairConfig,
    ) -> anyhow::Result<Self> {
        let query = Query::new(&self.ts_language, source)?;

        let mut override_configs_by_id = HashMap::default();
        for (capture_index, mut name) in query.capture_names().iter().copied().enumerate() {
            let mut range_is_inclusive = false;
            if name.starts_with('_') {
                continue;
            }
            if let Some(prefix) = name.strip_suffix(".inclusive") {
                name = prefix;
                range_is_inclusive = true;
            }

            let value = overrides.get(name).cloned().unwrap_or_default();
            override_configs_by_id.insert(
                u32::try_from(capture_index).context("capture index exceeds u32 range")?,
                OverrideEntry {
                    name: name.to_string(),
                    range_is_inclusive,
                    value,
                },
            );
        }

        let referenced_override_names = overrides
            .keys()
            .chain(brackets.disabled_scopes_by_bracket_ix.iter().flatten());

        for referenced_name in referenced_override_names {
            if !override_configs_by_id
                .values()
                .any(|entry| entry.name == *referenced_name)
            {
                anyhow::bail!(
                    "Language {language_name:?} has overrides in config not in query: {referenced_name:?}"
                );
            }
        }

        for entry in override_configs_by_id.values_mut() {
            entry.value.disabled_bracket_ixs = brackets
                .disabled_scopes_by_bracket_ix
                .iter()
                .enumerate()
                .filter_map(|(bracket_index, disabled_scope_names)| {
                    if disabled_scope_names.contains(&entry.name) {
                        Some(u16::try_from(bracket_index).expect("bracket index should fit in u16"))
                    } else {
                        None
                    }
                })
                .collect();
        }

        brackets.disabled_scopes_by_bracket_ix.clear();

        self.override_config = Some(OverrideConfig {
            query,
            values: override_configs_by_id,
        });
        Ok(self)
    }

    pub fn with_redaction_query(
        mut self,
        source: &str,
        language_name: &LanguageName,
    ) -> anyhow::Result<Self> {
        let query = Query::new(&self.ts_language, source)?;
        let mut redaction_capture_index = 0;
        let has_required_captures = populate_capture_indices(
            &query,
            language_name,
            "redactions",
            &[],
            &mut [Capture::Required("redact", &mut redaction_capture_index)],
        )?;

        if has_required_captures {
            self.redactions_config = Some(RedactionConfig {
                query,
                redaction_capture_ix: redaction_capture_index,
            });
        }
        Ok(self)
    }
}
