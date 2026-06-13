mod editor;
mod fallible_options;
pub mod merge_from;
mod theme;

pub use editor::*;
pub use fallible_options::*;
pub use merge_from::MergeFrom as MergeFromTrait;
pub use theme::*;

use gpui::Pixels;
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

impl SettingsContent {
    pub fn theme_mode(&self) -> ThemeAppearanceMode {
        self.theme
            .as_ref()
            .and_then(|theme| theme.mode)
            .unwrap_or_default()
    }

    pub fn ui_density(&self) -> UiDensity {
        self.ui
            .as_ref()
            .and_then(|ui| ui.density)
            .unwrap_or_default()
    }

    pub fn ui_font_size(&self) -> Pixels {
        self.ui
            .as_ref()
            .and_then(|ui| ui.font_size)
            .unwrap_or(gpui::px(13.0))
    }

    pub fn buffer_font_size(&self) -> Pixels {
        self.editor
            .as_ref()
            .and_then(|editor| editor.font_size)
            .unwrap_or(gpui::px(13.0))
    }

    pub fn ui_font_family(&self) -> Option<&str> {
        self.ui.as_ref().and_then(|ui| ui.font_family.as_deref())
    }

    pub fn ui_font_fallbacks(&self) -> Option<&[String]> {
        self.ui.as_ref().and_then(|ui| ui.font_fallbacks.as_deref())
    }

    pub fn ui_font_features(&self) -> Option<&FontFeaturesContent> {
        self.ui.as_ref().and_then(|ui| ui.font_features.as_ref())
    }

    pub fn ui_font_weight(&self) -> Option<FontWeightContent> {
        self.ui.as_ref().and_then(|ui| ui.font_weight)
    }

    pub fn buffer_font_family(&self) -> Option<&str> {
        self.editor
            .as_ref()
            .and_then(|editor| editor.font_family.as_deref())
    }

    pub fn buffer_font_fallbacks(&self) -> Option<&[String]> {
        self.editor
            .as_ref()
            .and_then(|editor| editor.font_fallbacks.as_deref())
    }

    pub fn buffer_font_features(&self) -> Option<&FontFeaturesContent> {
        self.editor
            .as_ref()
            .and_then(|editor| editor.font_features.as_ref())
    }

    pub fn buffer_font_weight(&self) -> Option<FontWeightContent> {
        self.editor.as_ref().and_then(|editor| editor.font_weight)
    }

    pub fn buffer_line_height(&self) -> BufferLineHeight {
        self.editor
            .as_ref()
            .and_then(|editor| editor.line_height)
            .unwrap_or_default()
    }

    pub fn current_line_highlight(&self) -> Option<CurrentLineHighlight> {
        self.editor
            .as_ref()
            .and_then(|editor| editor.current_line_highlight)
    }

    pub fn gutter(&self) -> GutterContent {
        self.editor
            .as_ref()
            .and_then(|editor| editor.gutter.clone())
            .unwrap_or_default()
    }
}
