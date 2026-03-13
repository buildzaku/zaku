use gpui::App;
use std::collections::HashMap;

use crate::{Settings, SettingsStore, settings_store::SettingsContent};

pub fn init(cx: &mut App) {
    LogSettings::register(cx);
    cx.observe_global::<SettingsStore>(|cx| {
        let log_settings = LogSettings::get_global(cx);
        logger::filter::refresh_from_settings(&log_settings.scopes);
    })
    .detach();
}

#[derive(Clone, Debug)]
pub struct LogSettings {
    /// A map of log scopes to the desired log level.
    /// Useful for filtering out noisy logs or enabling more verbose logging.
    ///
    /// Example: {"editor": "trace", "workspace": "debug"}
    pub scopes: HashMap<String, String>,
}

impl Settings for LogSettings {
    fn from_settings(content: &SettingsContent) -> Self {
        LogSettings {
            scopes: content.log.clone().unwrap(),
        }
    }
}
