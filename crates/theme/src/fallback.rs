use gpui::Hsla;
use uuid::Uuid;

use crate::{
    Appearance, DEFAULT_DARK_THEME, DEFAULT_LIGHT_THEME, StatusColors, Theme, ThemeColors,
    ThemeFamily, ThemeStyles,
};

pub fn zaku_default_themes() -> ThemeFamily {
    ThemeFamily {
        id: Uuid::new_v4().to_string(),
        name: "Zaku (Fallback)".into(),
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
                surface_background: gpui::rgb(0x121212).into(),
                elevated_surface_background: gpui::rgb(0x212125).into(),
                panel_background: gpui::rgb(0x141414).into(),
                border: gpui::rgb(0x2f2f2f).into(),
                border_variant: gpui::rgb(0x2f2f2f).into(),
                border_focused: gpui::rgb(0xffffff).into(),
                border_disabled: gpui::rgba(0x2f2f2f80).into(),
                text: gpui::rgb(0xf1f1f1).into(),
                text_muted: gpui::rgb(0xa6a6b9).into(),
                text_placeholder: gpui::rgb(0xa6a6b9).into(),
                text_disabled: gpui::rgb(0xa6a6b9).into(),
                text_accent: gpui::rgb(0x74ade8).into(),
                icon: gpui::rgb(0xf1f1f1).into(),
                icon_muted: gpui::rgb(0xa6a6b9).into(),
                icon_disabled: gpui::rgb(0xa6a6b9).into(),
                icon_accent: gpui::rgb(0x74ade8).into(),
                element_background: gpui::rgb(0x313233).into(),
                element_hover: gpui::rgb(0x3a3a40).into(),
                element_active: gpui::rgb(0x3a3a40).into(),
                element_selected: gpui::rgb(0x3a3a40).into(),
                element_selection_background: gpui::rgba(0xffffff33).into(),
                element_disabled: gpui::rgb(0x2a2a36).into(),
                ghost_element_background: Hsla::transparent_black(),
                ghost_element_hover: gpui::rgb(0x313233).into(),
                ghost_element_active: gpui::rgb(0x3a3a40).into(),
                ghost_element_selected: gpui::rgb(0x3a3a40).into(),
                ghost_element_disabled: Hsla::transparent_black(),
                status_bar_background: gpui::rgb(0x121212).into(),
                editor_background: gpui::rgb(0x141414).into(),
                editor_foreground: gpui::rgb(0xf1f1f1).into(),
                scrollbar_track_background: gpui::rgb(0x141414).into(),
                scrollbar_track_border: gpui::rgb(0x2f2f2f).into(),
                scrollbar_thumb_background: gpui::rgb(0x3a3a40).into(),
                scrollbar_thumb_hover_background: gpui::rgb(0x3a3a40).into(),
                scrollbar_thumb_active_background: gpui::rgb(0x3a3a40).into(),
                scrollbar_thumb_border: gpui::rgb(0x2f2f2f).into(),
            };

            let status = StatusColors {
                conflict: gpui::rgb(0xdec184).into(),
                conflict_background: gpui::rgba(0xdec1841a).into(),
                conflict_border: gpui::rgb(0x5d4c2f).into(),
                created: gpui::rgb(0xa1c181).into(),
                created_background: gpui::rgba(0xa1c1811a).into(),
                created_border: gpui::rgb(0x38482f).into(),
                deleted: gpui::rgb(0xd07277).into(),
                deleted_background: gpui::rgba(0xd072771a).into(),
                deleted_border: gpui::rgb(0x4c2b2c).into(),
                error: gpui::rgb(0xd07277).into(),
                error_background: gpui::rgba(0xd072771a).into(),
                error_border: gpui::rgb(0x4c2b2c).into(),
                hidden: gpui::rgb(0xa6a6b9).into(),
                hidden_background: gpui::rgba(0xa6a6b91a).into(),
                hidden_border: gpui::rgb(0x2f2f2f).into(),
                hint: gpui::rgb(0x74ade8).into(),
                hint_background: gpui::rgba(0x74ade81a).into(),
                hint_border: gpui::rgb(0x293b5b).into(),
                ignored: gpui::rgb(0xa6a6b9).into(),
                ignored_background: gpui::rgba(0xa6a6b91a).into(),
                ignored_border: gpui::rgb(0x464b57).into(),
                info: gpui::rgb(0xffffff).into(),
                info_background: gpui::rgb(0x1f001f).into(),
                info_border: gpui::rgba(0xf891f880).into(),
                modified: gpui::rgb(0xdec184).into(),
                modified_background: gpui::rgba(0xdec1841a).into(),
                modified_border: gpui::rgb(0x5d4c2f).into(),
                renamed: gpui::rgb(0x74ade8).into(),
                renamed_background: gpui::rgba(0x74ade81a).into(),
                renamed_border: gpui::rgb(0x293b5b).into(),
                success: gpui::rgb(0xa1c181).into(),
                success_background: gpui::rgba(0xa1c1811a).into(),
                success_border: gpui::rgb(0x38482f).into(),
                unreachable: gpui::rgb(0xa6a6b9).into(),
                unreachable_background: gpui::rgba(0xa6a6b91a).into(),
                unreachable_border: gpui::rgb(0x464b57).into(),
                warning: gpui::rgb(0xdec184).into(),
                warning_background: gpui::rgba(0xdec1841a).into(),
                warning_border: gpui::rgb(0x5d4c2f).into(),
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
                text_accent: gpui::rgb(0x2b7bbb).into(),
                icon: gpui::rgb(0x0b0b0e).into(),
                icon_muted: gpui::rgb(0x474750).into(),
                icon_disabled: gpui::rgb(0x474750).into(),
                icon_accent: gpui::rgb(0x2b7bbb).into(),
                element_background: gpui::rgb(0xdcdfe0).into(),
                element_hover: gpui::rgb(0xdcdfe0).into(),
                element_active: gpui::rgb(0xdcdfe0).into(),
                element_selected: gpui::rgb(0xdcdfe0).into(),
                element_selection_background: gpui::rgba(0x00000033).into(),
                element_disabled: gpui::rgb(0xe4e4e8).into(),
                ghost_element_background: Hsla::transparent_black(),
                ghost_element_hover: gpui::rgb(0xdcdfe0).into(),
                ghost_element_active: gpui::rgb(0xdcdfe0).into(),
                ghost_element_selected: gpui::rgb(0xdcdfe0).into(),
                ghost_element_disabled: Hsla::transparent_black(),
                status_bar_background: gpui::rgb(0xf8f8f8).into(),
                editor_background: gpui::rgb(0xffffff).into(),
                editor_foreground: gpui::rgb(0x0b0b0e).into(),
                scrollbar_track_background: gpui::rgb(0xffffff).into(),
                scrollbar_track_border: gpui::rgb(0xd7d7dd).into(),
                scrollbar_thumb_background: gpui::rgb(0xdcdfe0).into(),
                scrollbar_thumb_hover_background: gpui::rgb(0xdcdfe0).into(),
                scrollbar_thumb_active_background: gpui::rgb(0xdcdfe0).into(),
                scrollbar_thumb_border: gpui::rgb(0xd7d7dd).into(),
            };

            let status = StatusColors {
                conflict: gpui::rgb(0xdec184).into(),
                conflict_background: gpui::rgba(0xdec1841a).into(),
                conflict_border: gpui::rgb(0x5d4c2f).into(),
                created: gpui::rgb(0x669f59).into(),
                created_background: gpui::rgba(0x669f591a).into(),
                created_border: gpui::rgb(0x38482f).into(),
                deleted: gpui::rgb(0xd07277).into(),
                deleted_background: gpui::rgba(0xd072771a).into(),
                deleted_border: gpui::rgb(0x4c2b2c).into(),
                error: gpui::rgb(0xd07277).into(),
                error_background: gpui::rgba(0xd072771a).into(),
                error_border: gpui::rgb(0x4c2b2c).into(),
                hidden: gpui::rgb(0x474750).into(),
                hidden_background: gpui::rgba(0x4747501a).into(),
                hidden_border: gpui::rgb(0xd7d7dd).into(),
                hint: gpui::rgb(0x2b7bbb).into(),
                hint_background: gpui::rgba(0x2b7bbb1a).into(),
                hint_border: gpui::rgb(0x2b7bbb60).into(),
                ignored: gpui::rgb(0x474750).into(),
                ignored_background: gpui::rgba(0x4747501a).into(),
                ignored_border: gpui::rgb(0xd7d7dd).into(),
                info: gpui::rgb(0x000000).into(),
                info_background: gpui::rgb(0xffffff).into(),
                info_border: gpui::rgba(0x00000060).into(),
                modified: gpui::rgb(0xdec184).into(),
                modified_background: gpui::rgba(0xdec1841a).into(),
                modified_border: gpui::rgb(0x5d4c2f).into(),
                renamed: gpui::rgb(0x2b7bbb).into(),
                renamed_background: gpui::rgba(0x2b7bbb1a).into(),
                renamed_border: gpui::rgba(0x2b7bbb60).into(),
                success: gpui::rgb(0x669f59).into(),
                success_background: gpui::rgba(0x669f591a).into(),
                success_border: gpui::rgb(0x38482f).into(),
                unreachable: gpui::rgb(0x474750).into(),
                unreachable_background: gpui::rgba(0x4747501a).into(),
                unreachable_border: gpui::rgb(0xd7d7dd).into(),
                warning: gpui::rgb(0xdec184).into(),
                warning_background: gpui::rgba(0xdec1841a).into(),
                warning_border: gpui::rgb(0x5d4c2f).into(),
            };

            (colors, status)
        }
    };

    ThemeStyles { colors, status }
}
