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
    let path = "themes/zaku/zaku.json";
    let bytes = match registry.assets().load(path) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => {
            log::error!("Failed to load theme at path {path:?}: asset not found");
            return;
        }
        Err(error) => {
            log::error!("Failed to load theme at path {path:?}: {error:?}");
            return;
        }
    };

    let Some((theme_directory, _)) = path.rsplit_once('/') else {
        return;
    };

    let theme_family = match ThemeFamily::from_bytes(&bytes, |theme_path| {
        let theme_path = format!("{theme_directory}/{theme_path}");
        let theme = registry.assets().load(&theme_path)?;
        Ok(theme)
    }) {
        Ok(theme_family) => theme_family,
        Err(error) => {
            log::error!("Failed to parse theme at path {path:?}: {error:?}");
            return;
        }
    };

    registry.insert_theme_families([theme_family]);
}
