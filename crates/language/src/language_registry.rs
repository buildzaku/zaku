use anyhow::{Context, anyhow};
use futures::{
    Future,
    channel::{mpsc, oneshot},
};
use gpui::{App, BackgroundExecutor};
use parking_lot::RwLock;
use std::{
    cell::LazyCell,
    collections::{HashMap, hash_map},
    fmt,
    path::Path,
    sync::Arc,
};

use theme::Theme;

use crate::{
    Bias, File, Language, LanguageConfig, LanguageId, LanguageMatcher, LanguageName,
    LanguageQueries, PLAIN_TEXT, Point, Rope,
};

pub struct LanguageRegistry {
    state: RwLock<LanguageRegistryState>,
    executor: BackgroundExecutor,
}

struct LanguageRegistryState {
    languages: Vec<Arc<Language>>,
    available_languages: Vec<AvailableLanguage>,
    grammars: HashMap<Arc<str>, tree_sitter::Language>,
    loading_languages: HashMap<LanguageId, Vec<oneshot::Sender<anyhow::Result<Arc<Language>>>>>,
    subscriptions: Vec<mpsc::UnboundedSender<()>>,
    theme: Option<Arc<Theme>>,
    version: usize,
    reload_count: usize,
}

#[derive(Clone)]
pub struct AvailableLanguage {
    id: LanguageId,
    name: LanguageName,
    grammar: Option<Arc<str>>,
    matcher: LanguageMatcher,
    load: Arc<dyn Fn() -> anyhow::Result<LoadedLanguage> + 'static + Send + Sync>,
    loaded: bool,
}

impl AvailableLanguage {
    pub fn name(&self) -> LanguageName {
        self.name.clone()
    }

    pub fn matcher(&self) -> &LanguageMatcher {
        &self.matcher
    }
}

#[derive(Copy, Clone, Default)]
enum LanguageMatchPrecedence {
    #[default]
    Undetermined,
    PathOrContent(usize),
}

#[derive(Debug)]
pub struct LanguageNotFound;

impl fmt::Display for LanguageNotFound {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "language not found")
    }
}

pub struct LoadedLanguage {
    pub config: LanguageConfig,
    pub queries: LanguageQueries,
}

impl LanguageRegistry {
    pub fn new(executor: BackgroundExecutor) -> Self {
        let registry = Self {
            state: RwLock::new(LanguageRegistryState {
                languages: Vec::new(),
                available_languages: Vec::new(),
                grammars: HashMap::default(),
                loading_languages: HashMap::default(),
                subscriptions: Vec::new(),
                theme: None,
                version: 0,
                reload_count: 0,
            }),
            executor,
        };
        registry.add(PLAIN_TEXT.clone());
        registry
    }

    #[cfg(any(test, feature = "test"))]
    pub fn test_new(executor: BackgroundExecutor) -> Self {
        Self::new(executor)
    }

    pub fn reload(&self) {
        self.state.write().reload();
    }

    pub fn register_language(
        &self,
        name: LanguageName,
        grammar_name: Option<Arc<str>>,
        matcher: LanguageMatcher,
        load: Arc<dyn Fn() -> anyhow::Result<LoadedLanguage> + 'static + Send + Sync>,
    ) {
        let state = &mut *self.state.write();

        for existing_language in &mut state.available_languages {
            if existing_language.name == name {
                existing_language.grammar = grammar_name;
                existing_language.matcher = matcher;
                existing_language.load = load;
                return;
            }
        }

        state.available_languages.push(AvailableLanguage {
            id: LanguageId::new(),
            name,
            grammar: grammar_name,
            matcher,
            load,
            loaded: false,
        });
        state.version += 1;
        state.reload_count += 1;
        state.notify_subscribers();
    }

    #[cfg(any(test, feature = "test"))]
    pub fn register_test_language(&self, config: LanguageConfig) {
        self.register_language(
            config.name.clone(),
            config.grammar.clone(),
            config.matcher.clone(),
            Arc::new(move || {
                Ok(LoadedLanguage {
                    config: config.clone(),
                    queries: LanguageQueries::default(),
                })
            }),
        );
    }

    pub fn register_native_grammars(
        &self,
        grammars: impl IntoIterator<Item = (impl Into<Arc<str>>, impl Into<tree_sitter::Language>)>,
    ) {
        self.state.write().grammars.extend(
            grammars
                .into_iter()
                .map(|(name, grammar)| (name.into(), grammar.into())),
        );
    }

    pub fn language_names(&self) -> Vec<LanguageName> {
        let state = self.state.read();
        let mut result = state
            .available_languages
            .iter()
            .filter(|language| !language.loaded)
            .map(|language| language.name.clone())
            .chain(state.languages.iter().map(|language| language.name()))
            .collect::<Vec<_>>();
        result.sort_unstable_by_key(|language_name| language_name.as_ref().to_lowercase());
        result
    }

    pub fn grammar_names(&self) -> Vec<Arc<str>> {
        let state = self.state.read();
        let mut result = state.grammars.keys().cloned().collect::<Vec<_>>();
        result.sort_unstable_by_key(|grammar_name| grammar_name.to_lowercase());
        result
    }

    pub fn add(&self, language: Arc<Language>) {
        let mut state = self.state.write();
        state.available_languages.push(AvailableLanguage {
            id: language.id,
            name: language.name(),
            grammar: language.config.grammar.clone(),
            matcher: language.config.matcher.clone(),
            load: Arc::new(|| Err(anyhow!("already loaded"))),
            loaded: true,
        });
        state.add(language);
    }

    pub fn subscribe(&self) -> mpsc::UnboundedReceiver<()> {
        let (sender, receiver) = mpsc::unbounded();
        self.state.write().subscriptions.push(sender);
        receiver
    }

    pub fn version(&self) -> usize {
        self.state.read().version
    }

    pub fn reload_count(&self) -> usize {
        self.state.read().reload_count
    }

    pub fn set_theme(&self, theme: Arc<Theme>) {
        let mut state = self.state.write();
        state.theme = Some(theme);
        if let Some(theme) = state.theme.as_ref() {
            for language in &state.languages {
                language.set_theme(theme.syntax());
            }
        }
    }

    pub fn language_for_name(
        self: &Arc<Self>,
        name: &str,
    ) -> impl Future<Output = anyhow::Result<Arc<Language>>> + use<> {
        let name = name.to_string();
        let receiver = self.get_or_load_language(|language_name, _, current_best_match| {
            match current_best_match {
                LanguageMatchPrecedence::Undetermined
                    if language_name.as_ref().eq_ignore_ascii_case(&name) =>
                {
                    Some(LanguageMatchPrecedence::PathOrContent(name.len()))
                }
                LanguageMatchPrecedence::Undetermined
                | LanguageMatchPrecedence::PathOrContent(_) => None,
            }
        });
        async move { receive_loaded_language(receiver).await }
    }

    pub fn language_name_for_extension(self: &Arc<Self>, extension: &str) -> Option<LanguageName> {
        self.state
            .read()
            .available_languages
            .iter()
            .find(|language| {
                language
                    .matcher()
                    .path_suffixes
                    .iter()
                    .any(|suffix| suffix == extension)
            })
            .map(|language| language.name.clone())
    }

    pub fn available_language_for_name(self: &Arc<Self>, name: &str) -> Option<AvailableLanguage> {
        self.state
            .read()
            .available_languages
            .iter()
            .find(|language| language.name.as_ref() == name)
            .cloned()
    }

    pub fn language_for_file(
        self: &Arc<Self>,
        file: &Arc<dyn File>,
        content: Option<&Rope>,
        cx: &App,
    ) -> Option<AvailableLanguage> {
        self.language_for_file_internal(&file.full_path(cx), content)
    }

    pub fn language_for_file_path(self: &Arc<Self>, path: &Path) -> Option<AvailableLanguage> {
        self.language_for_file_internal(path, None)
    }

    fn language_for_file_internal(
        self: &Arc<Self>,
        path: &Path,
        content: Option<&Rope>,
    ) -> Option<AvailableLanguage> {
        let filename = path.file_name().and_then(|filename| filename.to_str());
        let extension = filename.and_then(|filename| filename.split('.').next_back());
        let path_suffixes = [extension, filename, path.to_str()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let content = LazyCell::new(|| {
            content.map(|content| {
                let end = content.clip_point(Point::new(0, 256), Bias::Left);
                let end = content.point_to_offset(end);
                content.chunks_in_range(0..end).collect::<String>()
            })
        });

        self.find_matching_language(move |_, matcher, current_best_match| {
            let path_match_length = matcher.path_suffixes.iter().fold(0, |length, path_suffix| {
                let extension_suffix = format!(".{path_suffix}");

                let matched_suffix_length = path_suffixes
                    .iter()
                    .find(|suffix| suffix.ends_with(&extension_suffix) || *suffix == path_suffix)
                    .map(|suffix| suffix.len());

                matched_suffix_length.map_or(length, |suffix_length| length.max(suffix_length))
            });
            let path_match_length = (path_match_length > 0).then_some(path_match_length);
            let content_matches = || {
                matcher.first_line_pattern.as_ref().is_some_and(|pattern| {
                    content
                        .as_ref()
                        .is_some_and(|content| pattern.is_match(content))
                })
            };

            match current_best_match {
                LanguageMatchPrecedence::PathOrContent(current_length) => path_match_length
                    .filter(|length| *length >= current_length)
                    .map(LanguageMatchPrecedence::PathOrContent),
                LanguageMatchPrecedence::Undetermined => path_match_length
                    .map(LanguageMatchPrecedence::PathOrContent)
                    .or_else(|| {
                        content_matches().then_some(LanguageMatchPrecedence::PathOrContent(1))
                    }),
            }
        })
    }

    pub fn load_language_for_file_path<'a>(
        self: &Arc<Self>,
        path: &'a Path,
    ) -> impl Future<Output = anyhow::Result<Arc<Language>>> + 'a {
        let language = self.language_for_file_path(path);

        let registry = self.clone();
        async move {
            if let Some(language) = language {
                receive_loaded_language(registry.load_language(&language)).await
            } else {
                Err(anyhow!(LanguageNotFound))
            }
        }
    }

    fn find_matching_language(
        self: &Arc<Self>,
        callback: impl Fn(
            &LanguageName,
            &LanguageMatcher,
            LanguageMatchPrecedence,
        ) -> Option<LanguageMatchPrecedence>,
    ) -> Option<AvailableLanguage> {
        let state = self.state.read();
        state
            .available_languages
            .iter()
            .rev()
            .fold(None, |best_language_match, language| {
                let current_match_type = best_language_match
                    .as_ref()
                    .map_or(LanguageMatchPrecedence::default(), |(_, score)| *score);
                let language_score =
                    callback(&language.name, &language.matcher, current_match_type);

                match (language_score, current_match_type) {
                    (
                        Some(LanguageMatchPrecedence::PathOrContent(_)),
                        LanguageMatchPrecedence::Undetermined,
                    ) => language_score.map(|new_score| (language.clone(), new_score)),
                    (
                        Some(LanguageMatchPrecedence::PathOrContent(new_length)),
                        LanguageMatchPrecedence::PathOrContent(current_length),
                    ) => {
                        if new_length > current_length {
                            language_score.map(|new_score| (language.clone(), new_score))
                        } else {
                            best_language_match
                        }
                    }
                    (None | Some(LanguageMatchPrecedence::Undetermined), _) => best_language_match,
                }
            })
            .map(|(available_language, _)| available_language)
    }

    pub fn load_language(
        self: &Arc<Self>,
        language: &AvailableLanguage,
    ) -> oneshot::Receiver<anyhow::Result<Arc<Language>>> {
        let (sender, receiver) = oneshot::channel();

        let mut state = self.state.write();

        for loaded_language in &state.languages {
            if loaded_language.id == language.id {
                send_language_result(sender, Ok(loaded_language.clone()));
                return receiver;
            }
        }

        match state.loading_languages.entry(language.id) {
            hash_map::Entry::Occupied(mut entry) => entry.get_mut().push(sender),
            hash_map::Entry::Vacant(entry) => {
                let registry = self.clone();
                let id = language.id;
                let name = language.name.clone();
                let language_load = language.load.clone();

                self.executor
                    .spawn(async move {
                        let language = async {
                            let loaded_language = (language_load)()?;
                            if let Some(grammar_name) = loaded_language.config.grammar.clone() {
                                let grammar = Some(registry.get_or_load_grammar(&grammar_name)?);

                                Language::new_with_id(id, loaded_language.config, grammar)
                                    .with_queries(loaded_language.queries)
                            } else {
                                Ok(Language::new_with_id(id, loaded_language.config, None))
                            }
                        }
                        .await;

                        match language {
                            Ok(language) => {
                                let language = Arc::new(language);
                                let mut state = registry.state.write();

                                state.add(language.clone());
                                state.mark_language_loaded(id);
                                if let Some(mut senders) = state.loading_languages.remove(&id) {
                                    for sender in senders.drain(..) {
                                        send_language_result(sender, Ok(language.clone()));
                                    }
                                }
                            }
                            Err(error) => {
                                log::error!("Failed to load language {name}: {error:?}");
                                let mut state = registry.state.write();
                                state.mark_language_loaded(id);
                                if let Some(mut senders) = state.loading_languages.remove(&id) {
                                    for sender in senders.drain(..) {
                                        send_language_result(
                                            sender,
                                            Err(anyhow!("failed to load language {name}: {error}")),
                                        );
                                    }
                                }
                            }
                        }
                    })
                    .detach();

                entry.insert(vec![sender]);
            }
        }

        receiver
    }

    fn get_or_load_language(
        self: &Arc<Self>,
        callback: impl Fn(
            &LanguageName,
            &LanguageMatcher,
            LanguageMatchPrecedence,
        ) -> Option<LanguageMatchPrecedence>,
    ) -> oneshot::Receiver<anyhow::Result<Arc<Language>>> {
        let Some(language) = self.find_matching_language(callback) else {
            let (sender, receiver) = oneshot::channel();
            send_language_result(sender, Err(anyhow!(LanguageNotFound)));
            return receiver;
        };

        self.load_language(&language)
    }

    fn get_or_load_grammar(&self, name: &Arc<str>) -> anyhow::Result<tree_sitter::Language> {
        let state = self.state.read();
        state
            .grammars
            .get(name.as_ref())
            .cloned()
            .with_context(|| format!("no such grammar {name}"))
    }

    pub fn to_vec(&self) -> Vec<Arc<Language>> {
        self.state.read().languages.clone()
    }
}

impl LanguageRegistryState {
    fn add(&mut self, language: Arc<Language>) {
        if let Some(theme) = self.theme.as_ref() {
            language.set_theme(theme.syntax());
        }
        self.languages.push(language);
        self.version += 1;
        self.notify_subscribers();
    }

    fn reload(&mut self) {
        self.languages.clear();
        for language in &mut self.available_languages {
            language.loaded = false;
        }
        self.version += 1;
        self.reload_count += 1;
        self.notify_subscribers();
    }

    fn mark_language_loaded(&mut self, id: LanguageId) {
        if let Some(language) = self
            .available_languages
            .iter_mut()
            .find(|language| language.id == id)
        {
            language.loaded = true;
        }
    }

    fn notify_subscribers(&mut self) {
        self.subscriptions
            .retain(|sender| sender.unbounded_send(()).is_ok());
    }
}

async fn receive_loaded_language(
    receiver: oneshot::Receiver<anyhow::Result<Arc<Language>>>,
) -> anyhow::Result<Arc<Language>> {
    receiver.await?
}

fn send_language_result(
    sender: oneshot::Sender<anyhow::Result<Arc<Language>>>,
    result: anyhow::Result<Arc<Language>>,
) {
    if sender.send(result).is_err() {
        log::trace!("Language load receiver dropped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use regex::Regex;

    #[gpui::test]
    fn test_select_language(cx: &mut App) {
        let registry = Arc::new(LanguageRegistry::test_new(cx.background_executor().clone()));
        registry.add(Arc::new(Language::new(
            LanguageConfig {
                name: LanguageName::new_static("JSON"),
                matcher: LanguageMatcher {
                    path_suffixes: vec!["json".to_string()],
                    ..Default::default()
                },
                ..Default::default()
            },
            Some(tree_sitter_json::LANGUAGE.into()),
        )));
        registry.add(Arc::new(Language::new(
            LanguageConfig {
                name: LanguageName::new_static("JSONC"),
                matcher: LanguageMatcher {
                    path_suffixes: vec!["jsonc".to_string(), "settings.jsonc".to_string()],
                    ..Default::default()
                },
                ..Default::default()
            },
            Some(tree_sitter_json::LANGUAGE.into()),
        )));

        assert_eq!(
            registry
                .language_for_file_path(Path::new("settings.json"))
                .map(|language| language.name),
            Some("JSON".into())
        );
        assert_eq!(
            registry
                .language_for_file_path(Path::new("settings.jsonc"))
                .map(|language| language.name),
            Some("JSONC".into())
        );

        assert_eq!(
            registry
                .language_for_file_path(Path::new("project/project.settings.jsonc"))
                .map(|language| language.name),
            Some("JSONC".into())
        );

        assert!(
            registry
                .language_for_file_path(Path::new("request"))
                .is_none()
        );
        assert!(
            registry
                .language_for_file_path(Path::new("request.toml"))
                .is_none()
        );
    }

    #[gpui::test]
    fn test_first_line_pattern(cx: &mut App) {
        let registry = Arc::new(LanguageRegistry::test_new(cx.background_executor().clone()));

        registry.register_test_language(LanguageConfig {
            name: "XML".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["xml".into()],
                first_line_pattern: Some(Regex::new(r"<\?xml\b").unwrap()),
            },
            ..Default::default()
        });

        assert!(
            registry
                .language_for_file_internal(Path::new("response"), None)
                .is_none()
        );

        let plain_text: Rope = "nothing".into();
        assert!(
            registry
                .language_for_file_internal(Path::new("response"), Some(&plain_text))
                .is_none()
        );

        let xml_document: Rope = r#"<?xml version="1.0"?>"#.into();
        assert_eq!(
            registry
                .language_for_file_internal(Path::new("response"), Some(&xml_document))
                .unwrap()
                .name(),
            "XML"
        );
    }
}
