mod fallible_options;
pub mod merge_from;
mod paths;
mod settings_file;
mod settings_store;

use gpui::{App, Global};
use rust_embed::RustEmbed;
use std::borrow::Cow;

pub use paths::{config_dir, settings_file};
pub use settings_file::watch_config_file;
pub use settings_store::{Settings, SettingsContent, SettingsStore, UiDensity};
use util::asset_str;

#[derive(RustEmbed)]
#[folder = "../../assets"]
#[include = "settings/*"]
#[exclude = "*.DS_Store"]
pub struct SettingsAssets;

pub fn init(cx: &mut App) {
    let store = SettingsStore::new(default_settings());
    cx.set_global(store);
    SettingsStore::observe_active_settings_profile_name(cx).detach();
}

pub fn default_settings() -> Cow<'static, str> {
    asset_str::<SettingsAssets>("settings/default.json")
}

pub fn default_user_settings() -> Cow<'static, str> {
    asset_str::<SettingsAssets>("settings/default_user.json")
}

#[cfg(any(test, feature = "test-support"))]
pub fn test_settings() -> String {
    default_settings().into_owned()
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActiveSettingsProfileName(pub String);

impl Global for ActiveSettingsProfileName {}
