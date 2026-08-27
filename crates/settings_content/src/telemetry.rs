use serde::{Deserialize, Serialize};

use settings_macros::{MergeFrom, with_fallible_options};

#[with_fallible_options]
#[derive(Clone, Default, Serialize, Deserialize, MergeFrom)]
pub struct TelemetrySettingsContent {
    pub diagnostics: Option<bool>,
    pub metrics: Option<bool>,
}
