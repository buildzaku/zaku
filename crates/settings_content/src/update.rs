use serde::{Deserialize, Serialize};

use settings_macros::{MergeFrom, with_fallible_options};

#[with_fallible_options]
#[derive(Clone, Default, Serialize, Deserialize, MergeFrom)]
pub struct UpdateSettingsContent {
    pub automatic: Option<bool>,
}
