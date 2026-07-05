use gpui::App;
use std::sync::Arc;

use settings::{SettingsContent, SettingsStore, ThemeAppearanceMode};
use theme::{
    Appearance, GlobalTheme, LoadThemes, SystemAppearance, Theme, ThemeFamily, ThemeRegistry,
    default_theme,
};

pub fn init(themes_to_load: LoadThemes, cx: &mut App) {
    let should_load_bundled_themes = matches!(&themes_to_load, LoadThemes::All(_));

    theme::init(themes_to_load, cx);

    if should_load_bundled_themes {
        let registry = ThemeRegistry::global(cx);
        load_bundled_themes(&registry);
    }

    reload_theme(cx);

    cx.observe_global::<SettingsStore>(reload_theme).detach();
}

fn configured_theme(cx: &mut App) -> Arc<Theme> {
    let themes = ThemeRegistry::global(cx);
    let system_appearance = SystemAppearance::global(cx);
    let theme_mode = cx
        .try_global::<SettingsStore>()
        .and_then(|settings| {
            settings
                .content()
                .theme
                .as_ref()
                .and_then(|theme| theme.mode)
        })
        .unwrap_or_default();
    let appearance = match theme_mode {
        ThemeAppearanceMode::System => system_appearance.0,
        ThemeAppearanceMode::Light => Appearance::Light,
        ThemeAppearanceMode::Dark => Appearance::Dark,
    };
    let theme_name = default_theme(appearance);

    match themes
        .get(theme_name)
        .or_else(|_| themes.get(default_theme(Appearance::Dark)))
    {
        Ok(theme) => theme,
        Err(error) => {
            log::error!("Failed to load configured theme: {error}");
            GlobalTheme::theme(cx).clone()
        }
    }
}

pub fn reload_theme(cx: &mut App) {
    let theme = configured_theme(cx);
    GlobalTheme::update_theme(cx, theme);
    cx.refresh_windows();
}

pub fn set_mode(content: &mut SettingsContent, mode: ThemeAppearanceMode) {
    content.theme.get_or_insert_default().mode = Some(mode);
}

pub fn load_bundled_themes(registry: &ThemeRegistry) {
    let theme_paths = match registry.assets().list("themes/") {
        Ok(theme_paths) => theme_paths,
        Err(error) => {
            log::error!("Failed to list theme assets: {error:?}");
            return;
        }
    };
    let theme_paths = theme_paths
        .into_iter()
        .filter(|path| path.ends_with(".json"));

    for path in theme_paths {
        let theme = match registry.assets().load(&path) {
            Ok(Some(theme)) => theme,
            Ok(None) => continue,
            Err(error) => {
                log::error!("Failed to load theme at path {path:?}: {error:?}");
                continue;
            }
        };

        let refined = match ThemeFamily::from_bytes(&theme) {
            Ok(theme_family) => theme_family,
            Err(error) => {
                log::error!("Failed to parse theme at path {path:?}: {error:?}");
                continue;
            }
        };

        registry.insert_theme_families([refined]);
    }
}
