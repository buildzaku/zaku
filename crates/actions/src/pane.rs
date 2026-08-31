use gpui::Action;
use schemars::JsonSchema;
use serde::Deserialize;

use util::serde::default_true;

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SaveIntent {
    Save,
    SaveAll,
    Close,
    Skip,
}

/// Close the currently active item in the pane.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = pane)]
#[serde(deny_unknown_fields)]
pub struct CloseActiveItem {
    #[serde(default)]
    pub save_intent: Option<SaveIntent>,
}

/// Close all items in the pane.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = pane)]
#[serde(deny_unknown_fields)]
pub struct CloseAllItems {
    #[serde(default)]
    pub save_intent: Option<SaveIntent>,
}

/// Activate the previous item in the pane.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = pane)]
#[serde(deny_unknown_fields, default)]
pub struct ActivatePreviousItem {
    /// Whether to wrap from the first item to the last item.
    #[serde(default = "default_true")]
    pub wrap_around: bool,
}

impl Default for ActivatePreviousItem {
    fn default() -> Self {
        Self { wrap_around: true }
    }
}

/// Activate the next item in the pane.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = pane)]
#[serde(deny_unknown_fields, default)]
pub struct ActivateNextItem {
    /// Whether to wrap from the last item to the first item.
    #[serde(default = "default_true")]
    pub wrap_around: bool,
}

impl Default for ActivateNextItem {
    fn default() -> Self {
        Self { wrap_around: true }
    }
}
