use gpui::{FontStyle, FontWeight, HighlightStyle, Hsla, WindowBackgroundAppearance};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    Appearance, DEFAULT_DARK_THEME, DEFAULT_LIGHT_THEME, StatusColors, StatusColorsRefinement,
    SyntaxTheme, Theme, ThemeColors, ThemeFamily, ThemeStyles,
};

pub(crate) fn zaku_default_themes() -> ThemeFamily {
    ThemeFamily {
        id: Uuid::new_v4().to_string(),
        name: "Zaku (Fallback)".into(),
        themes: vec![fallback_dark_theme(), fallback_light_theme()],
    }
}

pub fn apply_status_color_defaults(status: &mut StatusColorsRefinement) {
    for (foreground_color, background_color) in [
        (&status.deleted, &mut status.deleted_background),
        (&status.created, &mut status.created_background),
        (&status.modified, &mut status.modified_background),
        (&status.conflict, &mut status.conflict_background),
        (&status.error, &mut status.error_background),
        (&status.hidden, &mut status.hidden_background),
    ] {
        if background_color.is_none()
            && let Some(foreground_color) = foreground_color
        {
            *background_color = Some(foreground_color.opacity(0.25));
        }
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
    let (colors, status, syntax) = match appearance {
        Appearance::Dark => {
            let colors = ThemeColors {
                background: gpui::rgb(0x0f0f0f).into(),
                surface_background: gpui::rgb(0x121212).into(),
                elevated_surface_background: gpui::rgb(0x212125).into(),
                panel_background: gpui::rgb(0x141414).into(),
                panel_indent_guide: gpui::rgba(0x8787872e).into(),
                panel_indent_guide_hover: gpui::rgba(0xededed2e).into(),
                panel_indent_guide_active: gpui::rgba(0xededed2e).into(),
                border: gpui::rgb(0x2f2f2f).into(),
                border_variant: gpui::rgb(0x333333).into(),
                border_focused: gpui::rgb(0xd1d1d1).into(),
                border_disabled: gpui::rgba(0x2f2f2f80).into(),
                text: gpui::rgb(0xc3c3c3).into(),
                text_muted: gpui::rgb(0xa6a6b9).into(),
                text_placeholder: gpui::rgb(0x363636).into(),
                text_disabled: gpui::rgb(0x4a4a4a).into(),
                text_accent: gpui::rgb(0xffffff).into(),
                icon: gpui::rgb(0xf1f1f1).into(),
                icon_muted: gpui::rgb(0xa6a6b9).into(),
                icon_disabled: gpui::rgb(0xa6a6b9).into(),
                icon_accent: gpui::rgb(0x74ade8).into(),
                element_background: gpui::rgb(0x313233).into(),
                element_hover: gpui::rgb(0x201f1f).into(),
                element_active: gpui::rgb(0x2e2d2d).into(),
                element_selected: gpui::rgb(0x2e2d2d).into(),
                element_selection_background: gpui::rgba(0xe4e4e433).into(),
                element_disabled: gpui::rgb(0x383737).into(),
                drop_target_background: gpui::rgba(0xe4e4e433).into(),
                drop_target_border: gpui::rgb(0xd1d1d1).into(),
                ghost_element_background: Hsla::transparent_black(),
                ghost_element_hover: gpui::rgb(0x313233).into(),
                ghost_element_active: gpui::rgb(0x3a3a40).into(),
                ghost_element_selected: gpui::rgb(0x3a3a40).into(),
                ghost_element_disabled: Hsla::transparent_black(),
                title_bar_background: gpui::rgb(0x121212).into(),
                title_bar_inactive_background: gpui::rgb(0x121212).into(),
                status_bar_background: gpui::rgb(0x121212).into(),
                panel_tab_bar_background: gpui::rgb(0x1a1a1a).into(),
                panel_tab_inactive_background: gpui::rgb(0x121212).into(),
                panel_tab_active_background: gpui::rgb(0x252525).into(),
                panel_tab_inactive_foreground: gpui::rgb(0xa6a6b9).into(),
                panel_tab_active_foreground: gpui::rgb(0xc3c3c3).into(),
                tab_bar_background: gpui::rgb(0x121212).into(),
                tab_inactive_background: gpui::rgb(0x121212).into(),
                tab_active_background: gpui::rgb(0x141414).into(),
                editor_background: gpui::rgb(0x141414).into(),
                editor_foreground: gpui::rgb(0xc3c3c3).into(),
                editor_active_line_background: gpui::rgba(0x312f2fad).into(),
                editor_gutter_background: gpui::rgb(0x141414).into(),
                editor_line_number: gpui::rgb(0x3f4042).into(),
                editor_active_line_number: gpui::rgb(0xc7c9cd).into(),
                scrollbar_track_background: gpui::rgb(0x141414).into(),
                scrollbar_track_border: gpui::rgb(0x1e1e1e).into(),
                scrollbar_thumb_background: gpui::rgb(0x202020).into(),
                scrollbar_thumb_hover_background: gpui::rgb(0x202020).into(),
                scrollbar_thumb_active_background: gpui::rgb(0x202020).into(),
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

            let syntax = Arc::new(SyntaxTheme::new(vec![
                (
                    "attribute".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0xaaa0fa).into()),
                        ..Default::default()
                    },
                ),
                (
                    "boolean".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0xebc88d).into()),
                        ..Default::default()
                    },
                ),
                (
                    "comment".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0x6d6d6d).into()),
                        font_style: Some(FontStyle::Italic),
                        ..Default::default()
                    },
                ),
                (
                    "constant".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0xebc88d).into()),
                        ..Default::default()
                    },
                ),
                (
                    "constructor".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0x87c3ff).into()),
                        ..Default::default()
                    },
                ),
                (
                    "emphasis".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0x83d6c5).into()),
                        font_style: Some(FontStyle::Italic),
                        ..Default::default()
                    },
                ),
                (
                    "emphasis.strong".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0xf8c762).into()),
                        font_weight: Some(FontWeight::BOLD),
                        ..Default::default()
                    },
                ),
                (
                    "function".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0xefb080).into()),
                        ..Default::default()
                    },
                ),
                (
                    "keyword".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0x83d6c5).into()),
                        ..Default::default()
                    },
                ),
                (
                    "link_text".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0x83d6c5).into()),
                        ..Default::default()
                    },
                ),
                (
                    "link_uri".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0x83d6c5).into()),
                        ..Default::default()
                    },
                ),
                (
                    "number".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0xebc88d).into()),
                        ..Default::default()
                    },
                ),
                (
                    "operator".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0xd6d6dd).into()),
                        ..Default::default()
                    },
                ),
                (
                    "property".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0xaa9bf5).into()),
                        ..Default::default()
                    },
                ),
                (
                    "property.json_key".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0x82d2ce).into()),
                        ..Default::default()
                    },
                ),
                (
                    "punctuation".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0xd6d6dd).into()),
                        ..Default::default()
                    },
                ),
                (
                    "punctuation.bracket.html".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0x898989).into()),
                        ..Default::default()
                    },
                ),
                (
                    "string".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0xe394dc).into()),
                        ..Default::default()
                    },
                ),
                (
                    "string.escape".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0xd6d6dd).into()),
                        ..Default::default()
                    },
                ),
                (
                    "string.regex".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0xd6d6dd).into()),
                        ..Default::default()
                    },
                ),
                (
                    "string.special".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0xd8dee9).into()),
                        ..Default::default()
                    },
                ),
                (
                    "string.special.symbol".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0x88c0d0).into()),
                        ..Default::default()
                    },
                ),
                (
                    "tag".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0x87c3ff).into()),
                        ..Default::default()
                    },
                ),
                (
                    "text.literal".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0xe394dc).into()),
                        ..Default::default()
                    },
                ),
                (
                    "type".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0x87c3ff).into()),
                        ..Default::default()
                    },
                ),
                (
                    "variable".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0x94c1fa).into()),
                        ..Default::default()
                    },
                ),
                (
                    "variable.special".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0xc1808a).into()),
                        ..Default::default()
                    },
                ),
            ]));

            (colors, status, syntax)
        }
        Appearance::Light => {
            let colors = ThemeColors {
                background: gpui::rgb(0xf5f5f8).into(),
                surface_background: gpui::rgb(0xf8f8f8).into(),
                elevated_surface_background: gpui::rgb(0xffffff).into(),
                panel_background: gpui::rgb(0xffffff).into(),
                panel_indent_guide: gpui::rgba(0x8383832e).into(),
                panel_indent_guide_hover: gpui::rgba(0x0303032e).into(),
                panel_indent_guide_active: gpui::rgba(0x0303032e).into(),
                border: gpui::rgb(0xd0d0d0).into(),
                border_variant: gpui::rgb(0xd0d0d0).into(),
                border_focused: gpui::rgb(0x2f2f2f).into(),
                border_disabled: gpui::rgba(0xd0d0d080).into(),
                text: gpui::rgb(0x5f5f5f).into(),
                text_muted: gpui::rgb(0x474750).into(),
                text_placeholder: gpui::rgb(0xcacaca).into(),
                text_disabled: gpui::rgb(0xb7b5b5).into(),
                text_accent: gpui::rgb(0x000000).into(),
                icon: gpui::rgb(0x0b0b0e).into(),
                icon_muted: gpui::rgb(0x474750).into(),
                icon_disabled: gpui::rgb(0x474750).into(),
                icon_accent: gpui::rgb(0x2b7bbb).into(),
                element_background: gpui::rgb(0xdcdfe0).into(),
                element_hover: gpui::rgb(0xf0f0f2).into(),
                element_active: gpui::rgb(0xe7e7e8).into(),
                element_selected: gpui::rgb(0xe7e7e8).into(),
                element_selection_background: gpui::rgba(0x4c4c4c33).into(),
                element_disabled: gpui::rgb(0xefebeb).into(),
                drop_target_background: gpui::rgba(0x4c4c4c33).into(),
                drop_target_border: gpui::rgb(0x2f2f2f).into(),
                ghost_element_background: Hsla::transparent_black(),
                ghost_element_hover: gpui::rgb(0xdcdfe0).into(),
                ghost_element_active: gpui::rgb(0xdcdfe0).into(),
                ghost_element_selected: gpui::rgb(0xdcdfe0).into(),
                ghost_element_disabled: Hsla::transparent_black(),
                title_bar_background: gpui::rgb(0xf8f8f8).into(),
                title_bar_inactive_background: gpui::rgb(0xf8f8f8).into(),
                status_bar_background: gpui::rgb(0xf8f8f8).into(),
                panel_tab_bar_background: gpui::rgb(0xf5f5f5).into(),
                panel_tab_inactive_background: gpui::rgb(0xf8f8f8).into(),
                panel_tab_active_background: gpui::rgb(0xdfdfdf).into(),
                panel_tab_inactive_foreground: gpui::rgb(0x5f5f5f).into(),
                panel_tab_active_foreground: gpui::rgb(0x474750).into(),
                tab_bar_background: gpui::rgb(0xf8f8f8).into(),
                tab_inactive_background: gpui::rgb(0xf8f8f8).into(),
                tab_active_background: gpui::rgb(0xffffff).into(),
                editor_background: gpui::rgb(0xffffff).into(),
                editor_foreground: gpui::rgb(0x0b0b0e).into(),
                editor_active_line_background: gpui::rgba(0xc9c9c9c4).into(),
                editor_gutter_background: gpui::rgb(0xffffff).into(),
                editor_line_number: gpui::rgb(0xa4a7a9).into(),
                editor_active_line_number: gpui::rgb(0x424548).into(),
                scrollbar_track_background: gpui::rgb(0xffffff).into(),
                scrollbar_track_border: gpui::rgb(0xe6dfdf).into(),
                scrollbar_thumb_background: gpui::rgb(0xe4e4e4).into(),
                scrollbar_thumb_hover_background: gpui::rgb(0xe4e4e4).into(),
                scrollbar_thumb_active_background: gpui::rgb(0xe4e4e4).into(),
                scrollbar_thumb_border: gpui::rgb(0xd0d0d0).into(),
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
                hidden_border: gpui::rgb(0xd0d0d0).into(),
                hint: gpui::rgb(0x2b7bbb).into(),
                hint_background: gpui::rgba(0x2b7bbb1a).into(),
                hint_border: gpui::rgba(0x2b7bbb60).into(),
                ignored: gpui::rgb(0x474750).into(),
                ignored_background: gpui::rgba(0x4747501a).into(),
                ignored_border: gpui::rgb(0xd0d0d0).into(),
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
                unreachable_border: gpui::rgb(0xd0d0d0).into(),
                warning: gpui::rgb(0xdec184).into(),
                warning_background: gpui::rgba(0xdec1841a).into(),
                warning_border: gpui::rgb(0x5d4c2f).into(),
            };

            let syntax = Arc::new(SyntaxTheme::new(vec![
                (
                    "attribute".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0x0088aa).into()),
                        ..Default::default()
                    },
                ),
                (
                    "boolean".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0x0088aa).into()),
                        ..Default::default()
                    },
                ),
                (
                    "comment".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0x059669).into()),
                        font_style: Some(FontStyle::Italic),
                        ..Default::default()
                    },
                ),
                (
                    "constant.builtin".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0x0088aa).into()),
                        ..Default::default()
                    },
                ),
                (
                    "emphasis".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0x008080).into()),
                        ..Default::default()
                    },
                ),
                (
                    "emphasis.strong".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0x000080).into()),
                        ..Default::default()
                    },
                ),
                (
                    "function".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0xd97700).into()),
                        ..Default::default()
                    },
                ),
                (
                    "keyword".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0x0088aa).into()),
                        ..Default::default()
                    },
                ),
                (
                    "number".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0xea8500).into()),
                        ..Default::default()
                    },
                ),
                (
                    "operator".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0x2a2a2a).into()),
                        ..Default::default()
                    },
                ),
                (
                    "property.json_key".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0x0451a5).into()),
                        ..Default::default()
                    },
                ),
                (
                    "punctuation".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0x000000).into()),
                        ..Default::default()
                    },
                ),
                (
                    "punctuation.bracket.html".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0x800000).into()),
                        ..Default::default()
                    },
                ),
                (
                    "punctuation.delimiter.html".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0x2a2a2a).into()),
                        ..Default::default()
                    },
                ),
                (
                    "string".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0xc2185b).into()),
                        ..Default::default()
                    },
                ),
                (
                    "string.escape".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0xff0000).into()),
                        ..Default::default()
                    },
                ),
                (
                    "string.special".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0xff0000).into()),
                        ..Default::default()
                    },
                ),
                (
                    "string.special.symbol".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0x2a2a2a).into()),
                        ..Default::default()
                    },
                ),
                (
                    "text.literal".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0xc2185b).into()),
                        ..Default::default()
                    },
                ),
                (
                    "tag".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0x2563eb).into()),
                        ..Default::default()
                    },
                ),
                (
                    "type".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0xd97700).into()),
                        ..Default::default()
                    },
                ),
                (
                    "variable".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0x2a2a2a).into()),
                        ..Default::default()
                    },
                ),
                (
                    "variable.special".to_string(),
                    HighlightStyle {
                        color: Some(gpui::rgb(0xbe185d).into()),
                        ..Default::default()
                    },
                ),
            ]));

            (colors, status, syntax)
        }
    };

    ThemeStyles {
        window_background_appearance: WindowBackgroundAppearance::Opaque,
        colors,
        status,
        syntax,
    }
}
