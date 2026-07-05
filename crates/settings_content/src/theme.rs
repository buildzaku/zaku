use indexmap::IndexMap;
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{MapAccess, Visitor},
};
use std::{fmt, sync::Arc};

use settings_macros::{MergeFrom, with_fallible_options};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, MergeFrom)]
#[serde(rename_all = "snake_case")]
pub enum ThemeAppearanceMode {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum FeatureValue {
    Bool(bool),
    Number(serde_json::Number),
}

fn is_valid_feature_tag(tag: &str) -> bool {
    tag.len() == 4
        && tag
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, MergeFrom)]
#[serde(transparent)]
pub struct FontFeaturesContent(pub IndexMap<String, u32>);

impl FontFeaturesContent {
    pub fn new() -> Self {
        Self(IndexMap::default())
    }
}

impl<'de> Deserialize<'de> for FontFeaturesContent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct FontFeaturesVisitor;

        impl<'de> Visitor<'de> for FontFeaturesVisitor {
            type Value = FontFeaturesContent;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a map of font features")
            }

            fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut feature_map = IndexMap::default();

                while let Some((key, value)) =
                    access.next_entry::<String, Option<FeatureValue>>()?
                {
                    if !is_valid_feature_tag(&key) {
                        log::error!("Invalid font feature tag: {key}");
                        continue;
                    }

                    let Some(value) = value else {
                        continue;
                    };

                    match value {
                        FeatureValue::Bool(enable) => {
                            feature_map.insert(key, u32::from(enable));
                        }
                        FeatureValue::Number(value) => {
                            if let Some(value) =
                                value.as_u64().and_then(|value| u32::try_from(value).ok())
                            {
                                feature_map.insert(key, value);
                            } else {
                                log::error!(
                                    "Invalid font feature value {value} for feature tag {key}",
                                );
                            }
                        }
                    }
                }

                Ok(FontFeaturesContent(feature_map))
            }
        }

        deserializer.deserialize_map(FontFeaturesVisitor)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, MergeFrom)]
#[serde(transparent)]
pub struct FontSize(pub f32);

impl fmt::Display for FontSize {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:.2}", self.0)
    }
}

impl From<f32> for FontSize {
    fn from(value: f32) -> Self {
        Self(value)
    }
}

#[with_fallible_options]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, MergeFrom)]
#[serde(transparent)]
pub struct FontFamilyName(pub Arc<str>);

impl AsRef<str> for FontFamilyName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for FontFamilyName {
    fn from(value: String) -> Self {
        Self(Arc::from(value))
    }
}

impl From<FontFamilyName> for String {
    fn from(value: FontFamilyName) -> Self {
        value.0.to_string()
    }
}

#[with_fallible_options]
#[derive(Clone, Default, Serialize, Deserialize, MergeFrom)]
pub struct ThemeSettingsContent {
    pub mode: Option<ThemeAppearanceMode>,
}

#[with_fallible_options]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, MergeFrom)]
#[serde(default)]
pub struct ThemeStyleContent {
    #[serde(default)]
    pub colors: ThemeColorsContent,

    #[serde(default)]
    pub syntax: IndexMap<String, HighlightStyleContent>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, MergeFrom)]
#[serde(default)]
pub struct HighlightStyleContent {
    pub color: Option<String>,

    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "treat_error_as_none"
    )]
    pub background_color: Option<String>,

    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "treat_error_as_none"
    )]
    pub font_style: Option<FontStyleContent>,

    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "treat_error_as_none"
    )]
    pub font_weight: Option<FontWeightContent>,
}

impl HighlightStyleContent {
    pub fn is_empty(&self) -> bool {
        self.color.is_none()
            && self.background_color.is_none()
            && self.font_style.is_none()
            && self.font_weight.is_none()
    }
}

fn treat_error_as_none<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    let value: serde_json::Value = Deserialize::deserialize(deserializer)?;
    Ok(T::deserialize(value).ok())
}

#[with_fallible_options]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, MergeFrom)]
#[serde(default)]
pub struct ThemeColorsContent {
    #[serde(rename = "background")]
    pub background: Option<String>,
    #[serde(rename = "surface.background")]
    pub surface_background: Option<String>,
    #[serde(rename = "elevated_surface.background")]
    pub elevated_surface_background: Option<String>,
    #[serde(rename = "panel.background")]
    pub panel_background: Option<String>,
    #[serde(rename = "panel.indent_guide")]
    pub panel_indent_guide: Option<String>,
    #[serde(rename = "panel.indent_guide_hover")]
    pub panel_indent_guide_hover: Option<String>,
    #[serde(rename = "panel.indent_guide_active")]
    pub panel_indent_guide_active: Option<String>,

    #[serde(rename = "border")]
    pub border: Option<String>,
    #[serde(rename = "border.variant")]
    pub border_variant: Option<String>,
    #[serde(rename = "border.focused")]
    pub border_focused: Option<String>,
    #[serde(rename = "border.disabled")]
    pub border_disabled: Option<String>,

    #[serde(rename = "text")]
    pub text: Option<String>,
    #[serde(rename = "text.muted")]
    pub text_muted: Option<String>,
    #[serde(rename = "text.placeholder")]
    pub text_placeholder: Option<String>,
    #[serde(rename = "text.disabled")]
    pub text_disabled: Option<String>,
    #[serde(rename = "text.accent")]
    pub text_accent: Option<String>,

    #[serde(rename = "icon")]
    pub icon: Option<String>,
    #[serde(rename = "icon.muted")]
    pub icon_muted: Option<String>,
    #[serde(rename = "icon.disabled")]
    pub icon_disabled: Option<String>,
    #[serde(rename = "icon.accent")]
    pub icon_accent: Option<String>,

    #[serde(rename = "button.background")]
    pub button_background: Option<String>,
    #[serde(rename = "button.foreground")]
    pub button_foreground: Option<String>,
    #[serde(rename = "button.hover_background")]
    pub button_hover_background: Option<String>,
    #[serde(rename = "button.border")]
    pub button_border: Option<String>,
    #[serde(rename = "button.secondary_background")]
    pub button_secondary_background: Option<String>,
    #[serde(rename = "button.secondary_foreground")]
    pub button_secondary_foreground: Option<String>,
    #[serde(rename = "button.secondary_hover_background")]
    pub button_secondary_hover_background: Option<String>,
    #[serde(rename = "button.secondary_border")]
    pub button_secondary_border: Option<String>,

    #[serde(rename = "element.background")]
    pub element_background: Option<String>,
    #[serde(rename = "element.hover")]
    pub element_hover: Option<String>,
    #[serde(rename = "element.active")]
    pub element_active: Option<String>,
    #[serde(rename = "element.selected")]
    pub element_selected: Option<String>,
    #[serde(rename = "element.selection_background")]
    pub element_selection_background: Option<String>,
    #[serde(rename = "element.disabled")]
    pub element_disabled: Option<String>,
    #[serde(rename = "drop_target.background")]
    pub drop_target_background: Option<String>,
    #[serde(rename = "drop_target.border")]
    pub drop_target_border: Option<String>,

    #[serde(rename = "ghost_element.background")]
    pub ghost_element_background: Option<String>,
    #[serde(rename = "ghost_element.hover")]
    pub ghost_element_hover: Option<String>,
    #[serde(rename = "ghost_element.active")]
    pub ghost_element_active: Option<String>,
    #[serde(rename = "ghost_element.selected")]
    pub ghost_element_selected: Option<String>,
    #[serde(rename = "ghost_element.disabled")]
    pub ghost_element_disabled: Option<String>,

    #[serde(rename = "title_bar.background")]
    pub title_bar_background: Option<String>,
    #[serde(rename = "title_bar.inactive_background")]
    pub title_bar_inactive_background: Option<String>,
    #[serde(rename = "status_bar.background")]
    pub status_bar_background: Option<String>,
    #[serde(rename = "panel.tab_bar.background")]
    pub panel_tab_bar_background: Option<String>,
    #[serde(rename = "panel.tab.inactive_background")]
    pub panel_tab_inactive_background: Option<String>,
    #[serde(rename = "panel.tab.active_background")]
    pub panel_tab_active_background: Option<String>,
    #[serde(rename = "panel.tab.inactive_foreground")]
    pub panel_tab_inactive_foreground: Option<String>,
    #[serde(rename = "panel.tab.active_foreground")]
    pub panel_tab_active_foreground: Option<String>,
    #[serde(rename = "tab_bar.background")]
    pub tab_bar_background: Option<String>,
    #[serde(rename = "tab.inactive_background")]
    pub tab_inactive_background: Option<String>,
    #[serde(rename = "tab.active_background")]
    pub tab_active_background: Option<String>,

    #[serde(rename = "editor.background")]
    pub editor_background: Option<String>,
    #[serde(rename = "editor.foreground")]
    pub editor_foreground: Option<String>,
    #[serde(rename = "editor.active_line_background")]
    pub editor_active_line_background: Option<String>,
    #[serde(rename = "editor.gutter.background")]
    pub editor_gutter_background: Option<String>,
    #[serde(rename = "editor.line_number")]
    pub editor_line_number: Option<String>,
    #[serde(rename = "editor.active_line_number")]
    pub editor_active_line_number: Option<String>,

    #[serde(rename = "scrollbar.track.background")]
    pub scrollbar_track_background: Option<String>,
    #[serde(rename = "scrollbar.track.border")]
    pub scrollbar_track_border: Option<String>,
    #[serde(rename = "scrollbar.thumb.background")]
    pub scrollbar_thumb_background: Option<String>,
    #[serde(rename = "scrollbar.thumb.hover_background")]
    pub scrollbar_thumb_hover_background: Option<String>,
    #[serde(rename = "scrollbar.thumb.active_background")]
    pub scrollbar_thumb_active_background: Option<String>,
    #[serde(rename = "scrollbar.thumb.border")]
    pub scrollbar_thumb_border: Option<String>,

    #[serde(rename = "conflict")]
    pub conflict: Option<String>,
    #[serde(rename = "conflict.background")]
    pub conflict_background: Option<String>,
    #[serde(rename = "conflict.border")]
    pub conflict_border: Option<String>,

    #[serde(rename = "created")]
    pub created: Option<String>,
    #[serde(rename = "created.background")]
    pub created_background: Option<String>,
    #[serde(rename = "created.border")]
    pub created_border: Option<String>,

    #[serde(rename = "deleted")]
    pub deleted: Option<String>,
    #[serde(rename = "deleted.background")]
    pub deleted_background: Option<String>,
    #[serde(rename = "deleted.border")]
    pub deleted_border: Option<String>,

    #[serde(rename = "error")]
    pub error: Option<String>,
    #[serde(rename = "error.background")]
    pub error_background: Option<String>,
    #[serde(rename = "error.border")]
    pub error_border: Option<String>,

    #[serde(rename = "hidden")]
    pub hidden: Option<String>,
    #[serde(rename = "hidden.background")]
    pub hidden_background: Option<String>,
    #[serde(rename = "hidden.border")]
    pub hidden_border: Option<String>,

    #[serde(rename = "hint")]
    pub hint: Option<String>,
    #[serde(rename = "hint.background")]
    pub hint_background: Option<String>,
    #[serde(rename = "hint.border")]
    pub hint_border: Option<String>,

    #[serde(rename = "ignored")]
    pub ignored: Option<String>,
    #[serde(rename = "ignored.background")]
    pub ignored_background: Option<String>,
    #[serde(rename = "ignored.border")]
    pub ignored_border: Option<String>,

    #[serde(rename = "info")]
    pub info: Option<String>,
    #[serde(rename = "info.background")]
    pub info_background: Option<String>,
    #[serde(rename = "info.border")]
    pub info_border: Option<String>,

    #[serde(rename = "modified")]
    pub modified: Option<String>,
    #[serde(rename = "modified.background")]
    pub modified_background: Option<String>,
    #[serde(rename = "modified.border")]
    pub modified_border: Option<String>,

    #[serde(rename = "renamed")]
    pub renamed: Option<String>,
    #[serde(rename = "renamed.background")]
    pub renamed_background: Option<String>,
    #[serde(rename = "renamed.border")]
    pub renamed_border: Option<String>,

    #[serde(rename = "success")]
    pub success: Option<String>,
    #[serde(rename = "success.background")]
    pub success_background: Option<String>,
    #[serde(rename = "success.border")]
    pub success_border: Option<String>,

    #[serde(rename = "unreachable")]
    pub unreachable: Option<String>,
    #[serde(rename = "unreachable.background")]
    pub unreachable_background: Option<String>,
    #[serde(rename = "unreachable.border")]
    pub unreachable_border: Option<String>,

    #[serde(rename = "warning")]
    pub warning: Option<String>,
    #[serde(rename = "warning.background")]
    pub warning_background: Option<String>,
    #[serde(rename = "warning.border")]
    pub warning_border: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, MergeFrom)]
#[serde(rename_all = "snake_case")]
pub enum FontStyleContent {
    Normal,
    Italic,
    Oblique,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, MergeFrom)]
#[serde(transparent)]
pub struct FontWeightContent(pub f32);

impl FontWeightContent {
    pub const THIN: FontWeightContent = FontWeightContent(100.0);
    pub const EXTRA_LIGHT: FontWeightContent = FontWeightContent(200.0);
    pub const LIGHT: FontWeightContent = FontWeightContent(300.0);
    pub const NORMAL: FontWeightContent = FontWeightContent(400.0);
    pub const MEDIUM: FontWeightContent = FontWeightContent(500.0);
    pub const SEMIBOLD: FontWeightContent = FontWeightContent(600.0);
    pub const BOLD: FontWeightContent = FontWeightContent(700.0);
    pub const EXTRA_BOLD: FontWeightContent = FontWeightContent(800.0);
    pub const BLACK: FontWeightContent = FontWeightContent(900.0);
}

impl fmt::Display for FontWeightContent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl From<f32> for FontWeightContent {
    fn from(weight: f32) -> Self {
        FontWeightContent(weight)
    }
}

impl Default for FontWeightContent {
    fn default() -> Self {
        Self::NORMAL
    }
}
