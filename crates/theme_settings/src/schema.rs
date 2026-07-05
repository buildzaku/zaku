use gpui::HighlightStyle;
use serde::Deserialize;

use settings::IntoGpui;

use theme::{AppearanceContent, StatusColorsRefinement, ThemeColorsRefinement};

pub use settings::{
    FontStyleContent, FontWeightContent, HighlightStyleContent, StatusColorsContent,
    ThemeColorsContent, ThemeStyleContent, WindowBackgroundContent,
};

#[derive(Debug, Clone, Deserialize)]
pub struct ThemeFamilyContent {
    pub name: String,
    pub themes: Vec<ThemeContent>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThemeContent {
    pub name: String,
    pub appearance: AppearanceContent,
    pub style: ThemeStyleContent,
}

pub fn syntax_overrides(this: &ThemeStyleContent) -> Vec<(String, HighlightStyle)> {
    this.syntax
        .iter()
        .map(|(key, style)| {
            (
                key.clone(),
                HighlightStyle {
                    color: style
                        .color
                        .as_ref()
                        .and_then(|color| theme::try_parse_color(color).ok()),
                    background_color: style
                        .background_color
                        .as_ref()
                        .and_then(|color| theme::try_parse_color(color).ok()),
                    font_style: style.font_style.map(IntoGpui::into_gpui),
                    font_weight: style.font_weight.map(IntoGpui::into_gpui),
                    ..Default::default()
                },
            )
        })
        .collect()
}

pub fn status_colors_refinement(colors: &StatusColorsContent) -> StatusColorsRefinement {
    StatusColorsRefinement {
        conflict: colors
            .conflict
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        conflict_background: colors
            .conflict_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        conflict_border: colors
            .conflict_border
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        created: colors
            .created
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        created_background: colors
            .created_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        created_border: colors
            .created_border
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        deleted: colors
            .deleted
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        deleted_background: colors
            .deleted_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        deleted_border: colors
            .deleted_border
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        error: colors
            .error
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        error_background: colors
            .error_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        error_border: colors
            .error_border
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        hidden: colors
            .hidden
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        hidden_background: colors
            .hidden_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        hidden_border: colors
            .hidden_border
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        hint: colors
            .hint
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        hint_background: colors
            .hint_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        hint_border: colors
            .hint_border
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        ignored: colors
            .ignored
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        ignored_background: colors
            .ignored_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        ignored_border: colors
            .ignored_border
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        info: colors
            .info
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        info_background: colors
            .info_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        info_border: colors
            .info_border
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        modified: colors
            .modified
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        modified_background: colors
            .modified_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        modified_border: colors
            .modified_border
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        renamed: colors
            .renamed
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        renamed_background: colors
            .renamed_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        renamed_border: colors
            .renamed_border
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        success: colors
            .success
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        success_background: colors
            .success_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        success_border: colors
            .success_border
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        unreachable: colors
            .unreachable
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        unreachable_background: colors
            .unreachable_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        unreachable_border: colors
            .unreachable_border
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        warning: colors
            .warning
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        warning_background: colors
            .warning_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        warning_border: colors
            .warning_border
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
    }
}

pub fn theme_colors_refinement(colors: &ThemeColorsContent) -> ThemeColorsRefinement {
    let scrollbar_thumb_background = colors
        .scrollbar_thumb_background
        .as_ref()
        .and_then(|color| theme::try_parse_color(color).ok());
    let scrollbar_thumb_active_background = colors
        .scrollbar_thumb_active_background
        .as_ref()
        .and_then(|color| theme::try_parse_color(color).ok())
        .or(scrollbar_thumb_background);

    ThemeColorsRefinement {
        background: colors
            .background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        surface_background: colors
            .surface_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        elevated_surface_background: colors
            .elevated_surface_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        panel_background: colors
            .panel_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        panel_indent_guide: colors
            .panel_indent_guide
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        panel_indent_guide_hover: colors
            .panel_indent_guide_hover
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        panel_indent_guide_active: colors
            .panel_indent_guide_active
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        border: colors
            .border
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        border_variant: colors
            .border_variant
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        border_focused: colors
            .border_focused
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        border_disabled: colors
            .border_disabled
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        text: colors
            .text
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        text_muted: colors
            .text_muted
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        text_placeholder: colors
            .text_placeholder
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        text_disabled: colors
            .text_disabled
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        text_accent: colors
            .text_accent
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        icon: colors
            .icon
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        icon_muted: colors
            .icon_muted
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        icon_disabled: colors
            .icon_disabled
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        icon_accent: colors
            .icon_accent
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        button_background: colors
            .button_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        button_foreground: colors
            .button_foreground
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        button_hover_background: colors
            .button_hover_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        button_border: colors
            .button_border
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        button_secondary_background: colors
            .button_secondary_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        button_secondary_foreground: colors
            .button_secondary_foreground
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        button_secondary_hover_background: colors
            .button_secondary_hover_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        button_secondary_border: colors
            .button_secondary_border
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        element_background: colors
            .element_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        element_hover: colors
            .element_hover
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        element_active: colors
            .element_active
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        element_selected: colors
            .element_selected
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        element_selection_background: colors
            .element_selection_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        element_disabled: colors
            .element_disabled
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        drop_target_background: colors
            .drop_target_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        drop_target_border: colors
            .drop_target_border
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        ghost_element_background: colors
            .ghost_element_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        ghost_element_hover: colors
            .ghost_element_hover
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        ghost_element_active: colors
            .ghost_element_active
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        ghost_element_selected: colors
            .ghost_element_selected
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        ghost_element_disabled: colors
            .ghost_element_disabled
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        title_bar_background: colors
            .title_bar_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        title_bar_inactive_background: colors
            .title_bar_inactive_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        status_bar_background: colors
            .status_bar_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        panel_tab_bar_background: colors
            .panel_tab_bar_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        panel_tab_inactive_background: colors
            .panel_tab_inactive_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        panel_tab_active_background: colors
            .panel_tab_active_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        panel_tab_inactive_foreground: colors
            .panel_tab_inactive_foreground
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        panel_tab_active_foreground: colors
            .panel_tab_active_foreground
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        tab_bar_background: colors
            .tab_bar_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        tab_inactive_background: colors
            .tab_inactive_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        tab_active_background: colors
            .tab_active_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        editor_background: colors
            .editor_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        editor_foreground: colors
            .editor_foreground
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        editor_active_line_background: colors
            .editor_active_line_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        editor_gutter_background: colors
            .editor_gutter_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        editor_line_number: colors
            .editor_line_number
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        editor_active_line_number: colors
            .editor_active_line_number
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        scrollbar_track_background: colors
            .scrollbar_track_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        scrollbar_track_border: colors
            .scrollbar_track_border
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        scrollbar_thumb_background,
        scrollbar_thumb_hover_background: colors
            .scrollbar_thumb_hover_background
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
        scrollbar_thumb_active_background,
        scrollbar_thumb_border: colors
            .scrollbar_thumb_border
            .as_ref()
            .and_then(|color| theme::try_parse_color(color).ok()),
    }
}
