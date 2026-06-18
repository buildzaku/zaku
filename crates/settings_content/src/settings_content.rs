mod editor;
mod fallible_options;
pub mod merge_from;
mod theme;

pub use editor::*;
pub use fallible_options::*;
pub use merge_from::MergeFrom as MergeFromTrait;
pub use theme::*;

use serde::Deserialize;
use settings_macros::{MergeFrom, with_fallible_options};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseStatus {
    Success,
    Failed { error: String },
}

#[with_fallible_options]
#[derive(Clone, Default, Deserialize, MergeFrom)]
pub struct SettingsContent {
    pub theme: Option<ThemeSettingsContent>,
    pub ui: Option<UiSettingsContent>,
    pub editor: Option<EditorSettingsContent>,
    pub log: Option<HashMap<String, String>>,
}
