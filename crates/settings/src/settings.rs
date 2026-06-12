mod editor;
mod fallible_options;
mod keymap_file;
pub mod log_settings;
pub mod merge_from;
mod paths;
mod settings_file;
mod settings_store;

pub use settings_macros::RegisterSetting;

#[doc(hidden)]
pub mod private {
    pub use crate::settings_store::RegisteredSetting;
    pub use inventory;
}

pub use editor::{CurrentLineHighlight, GutterContent};
pub use fallible_options::ParseStatus;
pub use keymap_file::{ActionSequence, KeymapFile, KeymapFileLoadResult};
pub use paths::{
    config_dir, data_dir, keymap_file, log_file, logs_dir, old_log_file, settings_file,
};
pub use settings_file::watch_config_file;
pub use settings_store::{
    BufferLineHeight, FontFeaturesContent, FontWeightContent, Settings, SettingsContent,
    SettingsStore, ThemeAppearanceMode, UiDensity,
};

use gpui::App;
use rust_embed::RustEmbed;
use std::{borrow::Cow, fmt};

use util::asset_str;

#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash, PartialOrd, Ord, serde::Serialize)]
pub struct WorktreeId(usize);

impl From<WorktreeId> for usize {
    fn from(value: WorktreeId) -> Self {
        value.0
    }
}

impl WorktreeId {
    pub fn from_usize(handle_id: usize) -> Self {
        Self(handle_id)
    }

    pub fn from_proto(id: u64) -> Self {
        Self(usize::try_from(id).expect("worktree id should fit in usize"))
    }

    pub fn to_proto(self) -> u64 {
        self.0 as u64
    }

    pub fn to_usize(self) -> usize {
        self.0
    }
}

impl fmt::Display for WorktreeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

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

pub fn initial_user_keymap() -> Cow<'static, str> {
    asset_str::<SettingsAssets>("keymaps/initial_user.json")
}
