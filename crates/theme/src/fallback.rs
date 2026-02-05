use gpui::Hsla;
use uuid::Uuid;

use crate::{
    Appearance, DEFAULT_DARK_THEME, DEFAULT_LIGHT_THEME, StatusColors, Theme, ThemeColors,
    ThemeFamily, ThemeStyleContent, ThemeStyles,
};

pub fn comet_default_themes() -> ThemeFamily {
    ThemeFamily {
        id: Uuid::new_v4().to_string(),
        name: "Comet (Fallback)".into(),
        themes: vec![fallback_dark_theme(), fallback_light_theme()],
    }
}

pub(crate) fn fallback_dark_theme() -> Theme {
    let style = ThemeStyleContent {
        background: Some("#141414ff".to_string()),
        panel_background: Some("#1a1a1aff".to_string()),
        border: Some("#2a2a2aff".to_string()),
        border_variant: Some("#2a2a2aff".to_string()),
        border_focused: Some("#41d4dcff".to_string()),
        border_disabled: Some("#2a2a2a80".to_string()),
        text: Some("#ffffffff".to_string()),
        text_muted: Some("#9a9a9aff".to_string()),
        text_placeholder: Some("#8a8a8aff".to_string()),
        text_disabled: Some("#7a7a7aff".to_string()),
        text_accent: Some("#41d4dcff".to_string()),
        icon: Some("#ffffffff".to_string()),
        icon_muted: Some("#8a8a8aff".to_string()),
        icon_disabled: Some("#7a7a7aff".to_string()),
        icon_accent: Some("#41d4dcff".to_string()),
        element_background: Some("#292929ff".to_string()),
        element_hover: Some("#363636ff".to_string()),
        element_active: Some("#404040ff".to_string()),
        element_selected: Some("#404040ff".to_string()),
        element_disabled: Some("#202020ff".to_string()),
        ghost_element_background: Some("#00000000".to_string()),
        ghost_element_hover: Some("#292929ff".to_string()),
        ghost_element_active: Some("#404040ff".to_string()),
        ghost_element_selected: Some("#404040ff".to_string()),
        ghost_element_disabled: Some("#00000000".to_string()),
        status_bar_background: Some("#141414ff".to_string()),
        editor_background: Some("#1a1a1aff".to_string()),
        editor_foreground: Some("#ffffffff".to_string()),
        info: Some("#41d4dcff".to_string()),
        info_background: Some("#41d4dc33".to_string()),
        info_border: Some("#41d4dc80".to_string()),
    };

    Theme {
        id: Uuid::new_v4().to_string(),
        name: DEFAULT_DARK_THEME.into(),
        appearance: Appearance::Dark,
        styles: style.to_styles().unwrap_or_else(|error| {
            eprintln!("invalid fallback dark theme: {error:?}");
            fallback_theme_styles(Appearance::Dark)
        }),
    }
}

pub(crate) fn fallback_light_theme() -> Theme {
    let style = ThemeStyleContent {
        background: Some("#f7f7f7ff".to_string()),
        panel_background: Some("#ffffffff".to_string()),
        border: Some("#d0d0d0ff".to_string()),
        border_variant: Some("#d0d0d0ff".to_string()),
        border_focused: Some("#007a83ff".to_string()),
        border_disabled: Some("#d0d0d080".to_string()),
        text: Some("#1a1a1aff".to_string()),
        text_muted: Some("#6b6b6bff".to_string()),
        text_placeholder: Some("#7a7a7aff".to_string()),
        text_disabled: Some("#9a9a9aff".to_string()),
        text_accent: Some("#007a83ff".to_string()),
        icon: Some("#1a1a1aff".to_string()),
        icon_muted: Some("#6b6b6bff".to_string()),
        icon_disabled: Some("#9a9a9aff".to_string()),
        icon_accent: Some("#007a83ff".to_string()),
        element_background: Some("#f0f0f0ff".to_string()),
        element_hover: Some("#e7e7e7ff".to_string()),
        element_active: Some("#dededeff".to_string()),
        element_selected: Some("#dededeff".to_string()),
        element_disabled: Some("#f0f0f0ff".to_string()),
        ghost_element_background: Some("#00000000".to_string()),
        ghost_element_hover: Some("#e7e7e7ff".to_string()),
        ghost_element_active: Some("#dededeff".to_string()),
        ghost_element_selected: Some("#dededeff".to_string()),
        ghost_element_disabled: Some("#00000000".to_string()),
        status_bar_background: Some("#f7f7f7ff".to_string()),
        editor_background: Some("#ffffffff".to_string()),
        editor_foreground: Some("#1a1a1aff".to_string()),
        info: Some("#007a83ff".to_string()),
        info_background: Some("#007a8320".to_string()),
        info_border: Some("#007a8360".to_string()),
    };

    Theme {
        id: Uuid::new_v4().to_string(),
        name: DEFAULT_LIGHT_THEME.into(),
        appearance: Appearance::Light,
        styles: style.to_styles().unwrap_or_else(|error| {
            eprintln!("invalid fallback light theme: {error:?}");
            fallback_theme_styles(Appearance::Light)
        }),
    }
}

fn fallback_theme_styles(appearance: Appearance) -> ThemeStyles {
    let colors = match appearance {
        Appearance::Dark => ThemeColors {
            background: Hsla::transparent_black(),
            panel_background: Hsla::transparent_black(),
            border: Hsla::transparent_black(),
            border_variant: Hsla::transparent_black(),
            border_focused: Hsla::transparent_black(),
            border_disabled: Hsla::transparent_black(),
            text: gpui::rgb(0xffffff).into(),
            text_muted: gpui::rgb(0xb0b0b0).into(),
            text_placeholder: gpui::rgb(0x8a8a8a).into(),
            text_disabled: gpui::rgb(0x8a8a8a).into(),
            text_accent: gpui::rgb(0xffffff).into(),
            icon: gpui::rgb(0xffffff).into(),
            icon_muted: gpui::rgb(0xb0b0b0).into(),
            icon_disabled: gpui::rgb(0x8a8a8a).into(),
            icon_accent: gpui::rgb(0xffffff).into(),
            element_background: Hsla::transparent_black(),
            element_hover: Hsla::transparent_black(),
            element_active: Hsla::transparent_black(),
            element_selected: Hsla::transparent_black(),
            element_disabled: Hsla::transparent_black(),
            ghost_element_background: Hsla::transparent_black(),
            ghost_element_hover: Hsla::transparent_black(),
            ghost_element_active: Hsla::transparent_black(),
            ghost_element_selected: Hsla::transparent_black(),
            ghost_element_disabled: Hsla::transparent_black(),
            status_bar_background: Hsla::transparent_black(),
            editor_background: Hsla::transparent_black(),
            editor_foreground: gpui::rgb(0xffffff).into(),
        },
        Appearance::Light => ThemeColors {
            background: gpui::rgb(0xffffff).into(),
            panel_background: gpui::rgb(0xffffff).into(),
            border: gpui::rgb(0xcccccc).into(),
            border_variant: gpui::rgb(0xcccccc).into(),
            border_focused: gpui::rgb(0x333333).into(),
            border_disabled: gpui::rgb(0xcccccc).into(),
            text: gpui::rgb(0x000000).into(),
            text_muted: gpui::rgb(0x333333).into(),
            text_placeholder: gpui::rgb(0x666666).into(),
            text_disabled: gpui::rgb(0x666666).into(),
            text_accent: gpui::rgb(0x000000).into(),
            icon: gpui::rgb(0x000000).into(),
            icon_muted: gpui::rgb(0x333333).into(),
            icon_disabled: gpui::rgb(0x666666).into(),
            icon_accent: gpui::rgb(0x000000).into(),
            element_background: gpui::rgb(0xffffff).into(),
            element_hover: gpui::rgb(0xffffff).into(),
            element_active: gpui::rgb(0xffffff).into(),
            element_selected: gpui::rgb(0xffffff).into(),
            element_disabled: gpui::rgb(0xffffff).into(),
            ghost_element_background: Hsla::transparent_black(),
            ghost_element_hover: Hsla::transparent_black(),
            ghost_element_active: Hsla::transparent_black(),
            ghost_element_selected: Hsla::transparent_black(),
            ghost_element_disabled: Hsla::transparent_black(),
            status_bar_background: gpui::rgb(0xffffff).into(),
            editor_background: gpui::rgb(0xffffff).into(),
            editor_foreground: gpui::rgb(0x000000).into(),
        },
    };

    let info = colors.text_accent;
    let info_border = colors.border;

    ThemeStyles {
        colors,
        status: StatusColors {
            info,
            info_background: Hsla::transparent_black(),
            info_border,
        },
    }
}
