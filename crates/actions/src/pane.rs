use gpui::Action;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Clone, Copy, PartialEq, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SaveIntent {
    Save,
    SaveAll,
    Close,
    Skip,
}

/// Close the currently active item in the pane.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Default, Action)]
#[action(namespace = pane)]
#[serde(deny_unknown_fields)]
pub struct CloseActiveItem {
    #[serde(default)]
    pub save_intent: Option<SaveIntent>,
}

/// Close all items in the pane.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Default, Action)]
#[action(namespace = pane)]
#[serde(deny_unknown_fields)]
pub struct CloseAllItems {
    #[serde(default)]
    pub save_intent: Option<SaveIntent>,
}
