use gpui::Pixels;
use serde::Deserialize;
use settings_macros::{MergeFrom, with_fallible_options};

use crate::{BufferLineHeight, FontFeaturesContent, FontWeightContent};

#[with_fallible_options]
#[derive(Clone, Default, Deserialize, MergeFrom)]
pub struct EditorSettingsContent {
    pub font_size: Option<Pixels>,
    pub font_family: Option<String>,
    pub font_fallbacks: Option<Vec<String>>,
    pub font_features: Option<FontFeaturesContent>,
    pub font_weight: Option<FontWeightContent>,
    pub line_height: Option<BufferLineHeight>,
    pub gutter: Option<GutterContent>,
}

#[with_fallible_options]
#[derive(Clone, Default, Deserialize, MergeFrom)]
pub struct GutterContent {
    pub line_numbers: Option<bool>,
    pub min_line_number_digits: Option<usize>,
}
