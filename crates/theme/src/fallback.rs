use gpui::Hsla;
use uuid::Uuid;

use crate::{
    Appearance, DEFAULT_DARK_THEME, DEFAULT_LIGHT_THEME, StatusColors, Theme, ThemeColors,
    ThemeFamily, ThemeStyles,
};

pub fn comet_default_themes() -> ThemeFamily {
    ThemeFamily {
        id: Uuid::new_v4().to_string(),
        name: "Comet (Fallback)".into(),
        themes: vec![fallback_dark_theme(), fallback_light_theme()],
    }
}

pub(crate) fn fallback_dark_theme() -> Theme {
    Theme {
        id: Uuid::new_v4().to_string(),
        name: DEFAULT_DARK_THEME.into(),
        appearance: Appearance::Dark,
        styles: fallback_theme_styles(Appearance::Dark),
    }
}

pub(crate) fn fallback_light_theme() -> Theme {
    Theme {
        id: Uuid::new_v4().to_string(),
        name: DEFAULT_LIGHT_THEME.into(),
        appearance: Appearance::Light,
        styles: fallback_theme_styles(Appearance::Light),
    }
}

fn fallback_theme_styles(appearance: Appearance) -> ThemeStyles {
    let (colors, status) = match appearance {
        Appearance::Dark => {
            let colors = ThemeColors {
                background: gpui::rgb(0x0f0f0f).into(),
                surface_background: gpui::rgb(0x18181b).into(),
                elevated_surface_background: gpui::rgb(0x212125).into(),
                panel_background: gpui::rgb(0x161618).into(),
                border: gpui::rgb(0x414150).into(),
                border_variant: gpui::rgb(0x414150).into(),
                border_focused: gpui::rgb(0xffffff).into(),
                border_disabled: gpui::rgba(0x41415080).into(),
                text: gpui::rgb(0xcdd6f4).into(),
                text_muted: gpui::rgb(0xa6a6b9).into(),
                text_placeholder: gpui::rgb(0xa6a6b9).into(),
                text_disabled: gpui::rgb(0xa6a6b9).into(),
                text_accent: gpui::rgb(0x000000).into(),
                icon: gpui::rgb(0xcdd6f4).into(),
                icon_muted: gpui::rgb(0xa6a6b9).into(),
                icon_disabled: gpui::rgb(0xa6a6b9).into(),
                icon_accent: gpui::rgb(0x000000).into(),
                element_background: gpui::rgb(0x3d2c52).into(),
                element_hover: gpui::rgb(0x3a3a40).into(),
                element_active: gpui::rgb(0x3a3a40).into(),
                element_selected: gpui::rgb(0x3a3a40).into(),
                element_selection_background: gpui::rgba(0xffffff33).into(),
                element_disabled: gpui::rgb(0x2a2a36).into(),
                ghost_element_background: Hsla::transparent_black(),
                ghost_element_hover: gpui::rgb(0x3d2c52).into(),
                ghost_element_active: gpui::rgb(0x3a3a40).into(),
                ghost_element_selected: gpui::rgb(0x3a3a40).into(),
                ghost_element_disabled: Hsla::transparent_black(),
                status_bar_background: gpui::rgb(0x18181b).into(),
                editor_background: gpui::rgb(0x161618).into(),
                editor_foreground: gpui::rgb(0xcdd6f4).into(),
            };

            let status = StatusColors {
                info: gpui::rgb(0xffffff).into(),
                info_background: gpui::rgb(0x1f001f).into(),
                info_border: gpui::rgba(0xf891f880).into(),
            };

            (colors, status)
        }
        Appearance::Light => {
            let colors = ThemeColors {
                background: gpui::rgb(0xf5f5f8).into(),
                surface_background: gpui::rgb(0xf8f8f8).into(),
                elevated_surface_background: gpui::rgb(0xffffff).into(),
                panel_background: gpui::rgb(0xffffff).into(),
                border: gpui::rgb(0xd7d7dd).into(),
                border_variant: gpui::rgb(0xd7d7dd).into(),
                border_focused: gpui::rgb(0x000000).into(),
                border_disabled: gpui::rgba(0xd7d7dd80).into(),
                text: gpui::rgb(0x0b0b0e).into(),
                text_muted: gpui::rgb(0x474750).into(),
                text_placeholder: gpui::rgb(0x474750).into(),
                text_disabled: gpui::rgb(0x474750).into(),
                text_accent: gpui::rgb(0x000000).into(),
                icon: gpui::rgb(0x0b0b0e).into(),
                icon_muted: gpui::rgb(0x474750).into(),
                icon_disabled: gpui::rgb(0x474750).into(),
                icon_accent: gpui::rgb(0x000000).into(),
                element_background: gpui::rgb(0xe0e1e6).into(),
                element_hover: gpui::rgb(0xe0e1e6).into(),
                element_active: gpui::rgb(0xe0e1e6).into(),
                element_selected: gpui::rgb(0xe0e1e6).into(),
                element_selection_background: gpui::rgba(0x00000033).into(),
                element_disabled: gpui::rgb(0xe4e4e8).into(),
                ghost_element_background: Hsla::transparent_black(),
                ghost_element_hover: gpui::rgb(0xe0e1e6).into(),
                ghost_element_active: gpui::rgb(0xe0e1e6).into(),
                ghost_element_selected: gpui::rgb(0xe0e1e6).into(),
                ghost_element_disabled: Hsla::transparent_black(),
                status_bar_background: gpui::rgb(0xf8f8f8).into(),
                editor_background: gpui::rgb(0xffffff).into(),
                editor_foreground: gpui::rgb(0x0b0b0e).into(),
            };

            let status = StatusColors {
                info: gpui::rgb(0x000000).into(),
                info_background: gpui::rgb(0xffffff).into(),
                info_border: gpui::rgba(0x00000060).into(),
            };

            (colors, status)
        }
    };

    ThemeStyles { colors, status }
}
