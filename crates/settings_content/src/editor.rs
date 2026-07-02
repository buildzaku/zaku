use serde::{Deserialize, Serialize};

use settings_macros::{MergeFrom, with_fallible_options};

use crate::{FontFamilyName, FontFeaturesContent, FontSize, FontWeightContent};

fn deserialize_line_height<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = f32::deserialize(deserializer)?;
    if value < 1.0 {
        return Err(serde::de::Error::custom(
            "editor.line_height.custom must be at least 1.0",
        ));
    }

    Ok(value)
}

#[with_fallible_options]
#[derive(Clone, Default, Serialize, Deserialize, MergeFrom)]
pub struct EditorSettingsContent {
    pub font_size: Option<FontSize>,
    pub font_family: Option<FontFamilyName>,
    pub font_fallbacks: Option<Vec<FontFamilyName>>,
    pub font_features: Option<FontFeaturesContent>,
    pub font_weight: Option<FontWeightContent>,
    pub line_height: Option<BufferLineHeight>,
    pub current_line_highlight: Option<CurrentLineHighlight>,
    pub gutter: Option<GutterContent>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, MergeFrom)]
#[serde(rename_all = "snake_case")]
pub enum BufferLineHeight {
    #[default]
    Comfortable,
    Standard,
    Custom(#[serde(deserialize_with = "deserialize_line_height")] f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, MergeFrom)]
#[serde(rename_all = "snake_case")]
pub enum CurrentLineHighlight {
    None,
    Gutter,
    Line,
    All,
}

#[with_fallible_options]
#[derive(Clone, Default, Serialize, Deserialize, MergeFrom)]
pub struct GutterContent {
    pub line_numbers: Option<bool>,
    pub min_line_number_digits: Option<usize>,
}
