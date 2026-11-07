use gpui::Action;
use schemars::JsonSchema;
use serde::Deserialize;

/// Permanently delete the selected environment.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = variables_panel)]
#[serde(deny_unknown_fields)]
pub struct Delete {
    #[serde(default)]
    pub skip_prompt: bool,
}

/// Move the selected environment to the system trash.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = variables_panel)]
#[serde(deny_unknown_fields)]
pub struct Trash {
    #[serde(default)]
    pub skip_prompt: bool,
}

gpui::actions!(
    variables_panel,
    [
        /// Duplicate the selected environment.
        Duplicate,
        /// Create a new environment.
        NewEnvironment,
        /// Open the selected entry.
        Open,
        /// Rename the selected environment.
        Rename,
        /// Reveal the selected environment in the system file manager.
        RevealInFileManager,
        /// Toggle focus on the variables panel.
        ToggleFocus
    ]
);
