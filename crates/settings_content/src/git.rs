use serde::{Deserialize, Serialize};

use settings_macros::{MergeFrom, with_fallible_options};

#[with_fallible_options]
#[derive(Clone, Default, Serialize, Deserialize, MergeFrom)]
pub struct GitSettingsContent {
    pub enabled: Option<bool>,
    pub status: Option<GitStatusSettingsContent>,
}

#[with_fallible_options]
#[derive(Clone, Default, Serialize, Deserialize, MergeFrom)]
pub struct GitStatusSettingsContent {
    pub enabled: Option<bool>,
    pub project_panel: Option<GitStatusProjectPanelSettingsContent>,
    pub tabs: Option<GitStatusTabsSettingsContent>,
}

#[with_fallible_options]
#[derive(Clone, Default, Serialize, Deserialize, MergeFrom)]
pub struct GitStatusProjectPanelSettingsContent {
    pub colors: Option<bool>,
    pub indicators: Option<bool>,
}

#[with_fallible_options]
#[derive(Clone, Default, Serialize, Deserialize, MergeFrom)]
pub struct GitStatusTabsSettingsContent {
    pub colors: Option<bool>,
}
