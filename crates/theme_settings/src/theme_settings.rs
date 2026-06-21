mod schema;

pub use crate::schema::{
    FontStyleContent, FontWeightContent, HighlightStyleContent, StatusColorsContent,
    ThemeColorsContent, ThemeContent, ThemeFamilyContent, ThemeStyleContent,
    WindowBackgroundContent, status_colors_refinement, syntax_overrides, theme_colors_refinement,
};

use anyhow::Context;
use gpui::{App, WindowBackgroundAppearance};
use refineable::Refineable;
use std::sync::Arc;

use settings::{IntoGpui, SettingsContent, SettingsStore, ThemeAppearanceMode};
use theme::{
    Appearance, AppearanceContent, GlobalTheme, LoadThemes, StatusColors, SyntaxTheme,
    SystemAppearance, Theme, ThemeColors, ThemeFamily, ThemeRegistry, ThemeStyles,
    apply_status_color_defaults, default_theme,
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

        let theme_family = match serde_json::from_slice(&theme)
            .with_context(|| format!("failed to parse theme at path \"{path}\""))
        {
            Ok(theme_family) => theme_family,
            Err(error) => {
                log::error!("{error:?}");
                continue;
            }
        };

        let refined = refine_theme_family(theme_family);
        registry.insert_theme_families([refined]);
    }
}

pub fn refine_theme_family(theme_family_content: ThemeFamilyContent) -> ThemeFamily {
    let mut themes = Vec::with_capacity(theme_family_content.themes.len());
    for theme_content in &theme_family_content.themes {
        themes.push(refine_theme(theme_content));
    }

    ThemeFamily {
        id: uuid::Uuid::new_v4().to_string(),
        name: theme_family_content.name.into(),
        themes,
    }
}

pub fn refine_theme(theme: &ThemeContent) -> Theme {
    let appearance = match theme.appearance {
        AppearanceContent::Light => Appearance::Light,
        AppearanceContent::Dark => Appearance::Dark,
    };

    let mut refined_status_colors = match theme.appearance {
        AppearanceContent::Light => StatusColors::light(),
        AppearanceContent::Dark => StatusColors::dark(),
    };
    let mut status_colors_refinement = status_colors_refinement(&theme.style.status);
    apply_status_color_defaults(&mut status_colors_refinement);
    refined_status_colors.refine(&status_colors_refinement);

    let mut refined_theme_colors = match theme.appearance {
        AppearanceContent::Light => ThemeColors::light(),
        AppearanceContent::Dark => ThemeColors::dark(),
    };
    let theme_colors_refinement = theme_colors_refinement(&theme.style.colors);
    refined_theme_colors.refine(&theme_colors_refinement);

    let syntax_theme = Arc::new(SyntaxTheme::new(syntax_overrides(&theme.style)));

    let window_background_appearance = theme
        .style
        .window_background_appearance
        .map_or(WindowBackgroundAppearance::Opaque, IntoGpui::into_gpui);

    Theme {
        id: uuid::Uuid::new_v4().to_string(),
        name: theme.name.clone().into(),
        appearance,
        styles: ThemeStyles {
            window_background_appearance,
            colors: refined_theme_colors,
            status: refined_status_colors,
            syntax: syntax_theme,
        },
    }
}
