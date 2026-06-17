pub use language::*;

use std::sync::Arc;

struct LanguageInfo {
    name: &'static str,
}

pub fn init(languages: &LanguageRegistry) {
    languages.register_native_grammars(grammars::native_grammars());

    let built_in_languages = [
        LanguageInfo { name: "json" },
        LanguageInfo { name: "jsonc" },
        LanguageInfo { name: "html" },
    ];

    for registration in built_in_languages {
        register_language(languages, registration.name);
    }
}

fn register_language(languages: &LanguageRegistry, name: &'static str) {
    let config = grammars::load_config(name);
    languages.register_language(
        config.name.clone(),
        config.grammar.clone(),
        config.matcher.clone(),
        Arc::new(move || {
            Ok(LoadedLanguage {
                config: config.clone(),
                queries: grammars::load_queries(name),
            })
        }),
    );
}
