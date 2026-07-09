pub use settings::{
    FontStyleContent, FontWeightContent, HighlightStyleContent, ThemeColorsContent,
    ThemeStyleContent,
};

use anyhow::{Context, anyhow};
use gpui::{HighlightStyle, Hsla, Rgba};
use palette::FromColor;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, sync::Arc};

use ::settings::{IntoGpui, JSONC_PARSE_OPTIONS};

use crate::{Appearance, StatusColors, SyntaxTheme, Theme, ThemeColors, ThemeFamily, ThemeStyles};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AppearanceContent {
    Light,
    Dark,
}

impl From<AppearanceContent> for Appearance {
    fn from(value: AppearanceContent) -> Self {
        match value {
            AppearanceContent::Light => Self::Light,
            AppearanceContent::Dark => Self::Dark,
        }
    }
}

pub fn parse_color(color: &str) -> Hsla {
    let rgba = Rgba::try_from(color).expect("invalid theme color");
    let rgba = palette::rgb::Srgba::from_components((rgba.r, rgba.g, rgba.b, rgba.a));
    let hsla = palette::Hsla::from_color(rgba);

    gpui::hsla(
        hsla.hue.into_positive_degrees() / 360.0,
        hsla.saturation,
        hsla.lightness,
        hsla.alpha,
    )
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThemeFamilyContent {
    pub name: String,
    pub themes: Vec<ThemeContent>,
}

impl ThemeFamilyContent {
    pub(crate) fn into_theme_family(
        self,
        mut loader: impl FnMut(&str) -> anyhow::Result<Option<Cow<'static, [u8]>>>,
    ) -> anyhow::Result<ThemeFamily> {
        let mut themes = Vec::with_capacity(self.themes.len());
        for theme_content in self.themes {
            let theme_path = theme_content.path.clone();
            let Some(theme) = loader(&theme_path)? else {
                anyhow::bail!("theme file not found at path {theme_path:?}");
            };
            let theme_jsonc = std::str::from_utf8(theme.as_ref())
                .with_context(|| format!("theme file at path {theme_path:?} is not valid UTF-8"))?;
            let style = jsonc_parser::parse_to_serde_value(theme_jsonc, &JSONC_PARSE_OPTIONS)
                .map_err(|error| anyhow!("{error}"))
                .with_context(|| format!("failed to parse theme file at path {theme_path:?}"))?;
            themes.push(theme_content.into_theme(&style));
        }

        Ok(ThemeFamily {
            id: uuid::Uuid::new_v4().to_string(),
            name: self.name.into(),
            themes,
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThemeContent {
    pub name: String,
    pub appearance: AppearanceContent,
    pub path: String,
}

impl ThemeContent {
    fn into_theme(self, style: &ThemeStyleContent) -> Theme {
        let syntax_theme = Arc::new(SyntaxTheme::new(syntax_overrides(style)));

        Theme {
            id: uuid::Uuid::new_v4().to_string(),
            name: self.name.into(),
            appearance: self.appearance.into(),
            styles: ThemeStyles {
                colors: theme_colors(&style.colors),
                status: status_colors(&style.colors),
                syntax: syntax_theme,
            },
        }
    }
}

fn syntax_overrides(this: &ThemeStyleContent) -> Vec<(String, HighlightStyle)> {
    this.syntax
        .iter()
        .map(|(key, style)| {
            (
                key.clone(),
                HighlightStyle {
                    color: style.color.as_deref().map(parse_color),
                    background_color: style.background_color.as_deref().map(parse_color),
                    font_style: style.font_style.map(IntoGpui::into_gpui),
                    font_weight: style.font_weight.map(IntoGpui::into_gpui),
                    ..Default::default()
                },
            )
        })
        .collect()
}

fn status_colors(colors: &ThemeColorsContent) -> StatusColors {
    let color = |color: &Option<String>| {
        parse_color(color.as_deref().expect("theme color should be present"))
    };

    StatusColors {
        conflict: color(&colors.conflict),
        conflict_background: color(&colors.conflict_background),
        conflict_border: color(&colors.conflict_border),

        created: color(&colors.created),
        created_background: color(&colors.created_background),
        created_border: color(&colors.created_border),

        deleted: color(&colors.deleted),
        deleted_background: color(&colors.deleted_background),
        deleted_border: color(&colors.deleted_border),

        error: color(&colors.error),
        error_background: color(&colors.error_background),
        error_border: color(&colors.error_border),

        hidden: color(&colors.hidden),
        hidden_background: color(&colors.hidden_background),
        hidden_border: color(&colors.hidden_border),

        hint: color(&colors.hint),
        hint_background: color(&colors.hint_background),
        hint_border: color(&colors.hint_border),

        ignored: color(&colors.ignored),
        ignored_background: color(&colors.ignored_background),
        ignored_border: color(&colors.ignored_border),

        info: color(&colors.info),
        info_background: color(&colors.info_background),
        info_border: color(&colors.info_border),

        modified: color(&colors.modified),
        modified_background: color(&colors.modified_background),
        modified_border: color(&colors.modified_border),

        renamed: color(&colors.renamed),
        renamed_background: color(&colors.renamed_background),
        renamed_border: color(&colors.renamed_border),

        success: color(&colors.success),
        success_background: color(&colors.success_background),
        success_border: color(&colors.success_border),

        unreachable: color(&colors.unreachable),
        unreachable_background: color(&colors.unreachable_background),
        unreachable_border: color(&colors.unreachable_border),

        warning: color(&colors.warning),
        warning_background: color(&colors.warning_background),
        warning_border: color(&colors.warning_border),
    }
}

fn theme_colors(colors: &ThemeColorsContent) -> ThemeColors {
    let color = |color: &Option<String>| {
        parse_color(color.as_deref().expect("theme color should be present"))
    };

    ThemeColors {
        background: color(&colors.background),
        surface_background: color(&colors.surface_background),
        elevated_surface_background: color(&colors.elevated_surface_background),
        panel_background: color(&colors.panel_background),
        panel_indent_guide: color(&colors.panel_indent_guide),
        panel_indent_guide_hover: color(&colors.panel_indent_guide_hover),
        panel_indent_guide_active: color(&colors.panel_indent_guide_active),

        border: color(&colors.border),
        border_variant: color(&colors.border_variant),
        border_focused: color(&colors.border_focused),
        border_disabled: color(&colors.border_disabled),

        text: color(&colors.text),
        text_muted: color(&colors.text_muted),
        text_placeholder: color(&colors.text_placeholder),
        text_disabled: color(&colors.text_disabled),
        text_accent: color(&colors.text_accent),

        icon: color(&colors.icon),
        icon_muted: color(&colors.icon_muted),
        icon_disabled: color(&colors.icon_disabled),
        icon_accent: color(&colors.icon_accent),

        button_background: color(&colors.button_background),
        button_foreground: color(&colors.button_foreground),
        button_hover_background: color(&colors.button_hover_background),
        button_border: color(&colors.button_border),
        button_secondary_background: color(&colors.button_secondary_background),
        button_secondary_foreground: color(&colors.button_secondary_foreground),
        button_secondary_hover_background: color(&colors.button_secondary_hover_background),
        button_secondary_border: color(&colors.button_secondary_border),

        element_background: color(&colors.element_background),
        element_hover: color(&colors.element_hover),
        element_active: color(&colors.element_active),
        element_selected: color(&colors.element_selected),
        element_selection_background: color(&colors.element_selection_background),
        element_disabled: color(&colors.element_disabled),
        drop_target_background: color(&colors.drop_target_background),
        drop_target_border: color(&colors.drop_target_border),

        ghost_element_background: color(&colors.ghost_element_background),
        ghost_element_hover: color(&colors.ghost_element_hover),
        ghost_element_active: color(&colors.ghost_element_active),
        ghost_element_selected: color(&colors.ghost_element_selected),
        ghost_element_disabled: color(&colors.ghost_element_disabled),

        title_bar_background: color(&colors.title_bar_background),
        title_bar_inactive_background: color(&colors.title_bar_inactive_background),
        status_bar_background: color(&colors.status_bar_background),
        panel_tab_bar_background: color(&colors.panel_tab_bar_background),
        panel_tab_inactive_background: color(&colors.panel_tab_inactive_background),
        panel_tab_active_background: color(&colors.panel_tab_active_background),
        panel_tab_inactive_foreground: color(&colors.panel_tab_inactive_foreground),
        panel_tab_active_foreground: color(&colors.panel_tab_active_foreground),
        tab_bar_background: color(&colors.tab_bar_background),
        tab_inactive_background: color(&colors.tab_inactive_background),
        tab_active_background: color(&colors.tab_active_background),

        editor_background: color(&colors.editor_background),
        editor_foreground: color(&colors.editor_foreground),
        editor_active_line_background: color(&colors.editor_active_line_background),
        editor_gutter_background: color(&colors.editor_gutter_background),
        editor_line_number: color(&colors.editor_line_number),
        editor_active_line_number: color(&colors.editor_active_line_number),

        scrollbar_track_background: color(&colors.scrollbar_track_background),
        scrollbar_track_border: color(&colors.scrollbar_track_border),
        scrollbar_thumb_background: color(&colors.scrollbar_thumb_background),
        scrollbar_thumb_hover_background: color(&colors.scrollbar_thumb_hover_background),
        scrollbar_thumb_active_background: color(&colors.scrollbar_thumb_active_background),
        scrollbar_thumb_border: color(&colors.scrollbar_thumb_border),
    }
}
