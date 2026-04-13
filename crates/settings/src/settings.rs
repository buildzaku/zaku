mod fallible_options;
mod keymap_file;
pub mod log_settings;
pub mod merge_from;
mod paths;
mod settings_file;
mod settings_store;

use gpui::App;
use rust_embed::RustEmbed;
use std::borrow::Cow;

pub use fallible_options::ParseStatus;
pub use keymap_file::{ActionSequence, KeymapFile, KeymapFileLoadResult};
pub use paths::{
    config_dir, data_dir, keymap_file, log_file, logs_dir, old_log_file, settings_file,
};
pub use settings_file::watch_config_file;
pub use settings_store::{
    BufferLineHeight, FontFeaturesContent, FontWeightContent, Settings, SettingsContent,
    SettingsStore, UiDensity,
};
use util::asset_str;

#[derive(RustEmbed)]
#[folder = "../../assets"]
#[include = "settings/*"]
#[include = "keymaps/*"]
#[exclude = "*.DS_Store"]
pub struct SettingsAssets;

pub fn init(cx: &mut App) {
    let store = SettingsStore::new(default_settings());
    cx.set_global(store);
}

pub fn default_settings() -> Cow<'static, str> {
    asset_str::<SettingsAssets>("settings/default.json")
}

pub fn initial_user_settings() -> Cow<'static, str> {
    asset_str::<SettingsAssets>("settings/initial_user.json")
}
