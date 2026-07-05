use gpui::{Hsla, WindowBackgroundAppearance};
use serde::Deserialize;
use std::sync::Arc;

use refineable::Refineable;

use crate::{StatusColors, StatusColorsRefinement, SyntaxTheme, Theme};

#[derive(Debug, Clone, PartialEq, Refineable)]
pub struct ThemeStyles {
    pub window_background_appearance: WindowBackgroundAppearance,

    #[refineable]
    pub colors: ThemeColors,

    #[refineable]
    pub status: StatusColors,

    pub syntax: Arc<SyntaxTheme>,
}

#[derive(Debug, Clone, Default, PartialEq, Refineable)]
#[refineable(Debug, Deserialize)]
pub struct ThemeColors {
    pub background: Hsla,
    pub surface_background: Hsla,
    pub elevated_surface_background: Hsla,
    pub panel_background: Hsla,
    pub panel_indent_guide: Hsla,
    pub panel_indent_guide_hover: Hsla,
    pub panel_indent_guide_active: Hsla,

    pub border: Hsla,
    pub border_variant: Hsla,
    pub border_focused: Hsla,
    pub border_disabled: Hsla,

    pub text: Hsla,
    pub text_muted: Hsla,
    pub text_placeholder: Hsla,
    pub text_disabled: Hsla,
    pub text_accent: Hsla,

    pub icon: Hsla,
    pub icon_muted: Hsla,
    pub icon_disabled: Hsla,
    pub icon_accent: Hsla,

    pub button_background: Hsla,
    pub button_foreground: Hsla,
    pub button_hover_background: Hsla,
    pub button_border: Hsla,
    pub button_secondary_background: Hsla,
    pub button_secondary_foreground: Hsla,
    pub button_secondary_hover_background: Hsla,
    pub button_secondary_border: Hsla,

    pub element_background: Hsla,
    pub element_hover: Hsla,
    pub element_active: Hsla,
    pub element_selected: Hsla,
    pub element_selection_background: Hsla,
    pub element_disabled: Hsla,
    pub drop_target_background: Hsla,
    pub drop_target_border: Hsla,

    pub ghost_element_background: Hsla,
    pub ghost_element_hover: Hsla,
    pub ghost_element_active: Hsla,
    pub ghost_element_selected: Hsla,
    pub ghost_element_disabled: Hsla,

    pub title_bar_background: Hsla,
    pub title_bar_inactive_background: Hsla,
    pub status_bar_background: Hsla,
    pub panel_tab_bar_background: Hsla,
    pub panel_tab_inactive_background: Hsla,
    pub panel_tab_active_background: Hsla,
    pub panel_tab_inactive_foreground: Hsla,
    pub panel_tab_active_foreground: Hsla,
    pub tab_bar_background: Hsla,
    pub tab_inactive_background: Hsla,
    pub tab_active_background: Hsla,

    pub editor_background: Hsla,
    pub editor_foreground: Hsla,
    pub editor_active_line_background: Hsla,
    pub editor_gutter_background: Hsla,
    pub editor_line_number: Hsla,
    pub editor_active_line_number: Hsla,

    pub scrollbar_track_background: Hsla,
    pub scrollbar_track_border: Hsla,
    pub scrollbar_thumb_background: Hsla,
    pub scrollbar_thumb_hover_background: Hsla,
    pub scrollbar_thumb_active_background: Hsla,
    pub scrollbar_thumb_border: Hsla,
}

impl ThemeColors {
    pub fn dark() -> Self {
        Theme::default_dark().styles.colors
    }

    pub fn light() -> Self {
        Theme::default_light().styles.colors
    }
}
