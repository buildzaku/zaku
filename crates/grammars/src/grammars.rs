use anyhow::Context;
use rust_embed::RustEmbed;
use std::path::Path;

use language_core::{LanguageConfig, LanguageQueries, QUERY_FILENAME_PREFIXES};
use util::asset_str;

#[derive(RustEmbed)]
#[folder = "src/"]
#[exclude = "*.rs"]
struct GrammarDir;

#[cfg(feature = "load-grammars")]
pub fn native_grammars() -> Vec<(&'static str, tree_sitter::Language)> {
    vec![
        ("html", tree_sitter_html::LANGUAGE.into()),
        ("json", tree_sitter_json::LANGUAGE.into()),
        ("jsonc", tree_sitter_json::LANGUAGE.into()),
    ]
}

pub fn load_config(name: &str) -> LanguageConfig {
    let config_path = format!("{name}/config.toml");
    let config_toml = asset_str::<GrammarDir>(&config_path);

    toml::from_str(config_toml.as_ref())
        .with_context(|| format!("failed to load config.toml for language {name:?}"))
        .expect("language config should load")
}

pub fn get_file(path: &str) -> Option<rust_embed::EmbeddedFile> {
    GrammarDir::get(path)
}

pub fn load_queries(name: &str) -> LanguageQueries {
    let mut result = LanguageQueries::default();
    for path in GrammarDir::iter() {
        if let Some(remainder) = path
            .strip_prefix(name)
            .and_then(|path| path.strip_prefix('/'))
        {
            if !Path::new(remainder)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("scm"))
            {
                continue;
            }
            for (prefix, query) in QUERY_FILENAME_PREFIXES {
                if remainder.starts_with(prefix) {
                    let contents = asset_str::<GrammarDir>(path.as_ref());
                    match query(&mut result) {
                        None => *query(&mut result) = Some(contents),
                        Some(existing) => existing.to_mut().push_str(contents.as_ref()),
                    }
                }
            }
        }
    }
    result
}
