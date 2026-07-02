use serde::{Deserialize, Serialize};

use settings_macros::{MergeFrom, with_fallible_options};

use crate::{FontFamilyName, FontFeaturesContent, FontSize, FontWeightContent};

#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    MergeFrom,
)]
#[serde(rename_all = "snake_case")]
pub enum UiDensity {
    #[default]
    Default,
    Compact,
    Comfortable,
}

#[with_fallible_options]
#[derive(Clone, Default, Serialize, Deserialize, MergeFrom)]
pub struct UiSettingsContent {
    pub density: Option<UiDensity>,
    pub font_size: Option<FontSize>,
    pub font_family: Option<FontFamilyName>,
    pub font_fallbacks: Option<Vec<FontFamilyName>>,
    pub font_features: Option<FontFeaturesContent>,
    pub font_weight: Option<FontWeightContent>,
}
