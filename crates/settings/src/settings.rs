mod fallible_options;
pub mod merge_from;
mod paths;
mod settings_file;
mod settings_store;

use gpui::{App, BorrowAppContext, Global};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadSettings {
    None,
    Default,
    User,
}

pub fn init(load_settings: LoadSettings, cx: &mut App) {
    let store = SettingsStore::new(default_settings());
    cx.set_global(store);
    SettingsStore::observe_active_settings_profile_name(cx).detach();

    let user_settings = match load_settings {
        LoadSettings::None => return,
        LoadSettings::Default => default_user_settings().into_owned(),
        LoadSettings::User => match SettingsStore::load_settings() {
            Ok(user_settings) => user_settings,
            Err(error) => {
                eprintln!("failed to load settings file: {error}");
                default_user_settings().into_owned()
            }
        },
    };

    cx.update_global::<SettingsStore, _>(|store, cx| {
        store.set_user_settings(&user_settings, cx);
    });
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
