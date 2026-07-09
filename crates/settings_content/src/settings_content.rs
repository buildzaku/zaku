mod editor;
mod fallible_options;
mod git;
pub mod merge_from;
mod theme;
mod ui;

pub use editor::*;
pub use fallible_options::*;
pub use git::*;
pub use merge_from::MergeFrom as MergeFromTrait;
pub use theme::*;
pub use ui::*;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use settings_macros::{MergeFrom, with_fallible_options};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsLoadStatus {
    Loaded,
    PartiallyLoaded { error_message: String },
    FailedToParseJsonc { error: String },
    FailedToLoad { error: String },
}

#[with_fallible_options]
#[derive(Clone, Default, Serialize, Deserialize, MergeFrom)]
pub struct SettingsContent {
    pub theme: Option<ThemeSettingsContent>,
    pub ui: Option<UiSettingsContent>,
    pub editor: Option<EditorSettingsContent>,
    pub git: Option<GitSettingsContent>,
    pub log: Option<HashMap<String, String>>,
}
