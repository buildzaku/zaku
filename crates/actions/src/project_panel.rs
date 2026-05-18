use gpui::Action;
use schemars::JsonSchema;
use serde::Deserialize;

/// Permanently delete the selected file or directory.
#[derive(PartialEq, Clone, Default, Debug, Deserialize, JsonSchema, Action)]
#[action(namespace = project_panel)]
#[serde(deny_unknown_fields)]
pub struct Delete {
    #[serde(default)]
    pub skip_prompt: bool,
}

/// Move the selected file or directory to the system trash.
#[derive(PartialEq, Clone, Default, Debug, Deserialize, JsonSchema, Action)]
#[action(namespace = project_panel)]
#[serde(deny_unknown_fields)]
pub struct Trash {
    #[serde(default)]
    pub skip_prompt: bool,
}

gpui::actions!(
    project_panel,
    [
        /// Expand the selected entry in the project tree.
        ExpandSelectedEntry,
        /// Collapse the selected entry in the project tree.
        CollapseSelectedEntry,
        /// Collapse the selected entry and its children in the project tree.
        CollapseSelectedEntryAndChildren,
        /// Collapse all entries in the project tree.
        CollapseAllEntries,
        /// Create a new directory.
        NewDirectory,
        /// Create a new file.
        NewFile,
        /// Copy the selected file or directory.
        Copy,
        /// Duplicate the selected file or directory.
        Duplicate,
        /// Reveal the selected item in the system file manager.
        RevealInFileManager,
        /// Cut the selected file or directory.
        Cut,
        /// Paste the previously cut or copied item.
        Paste,
        /// Rename the selected file or directory.
        Rename,
        /// Open the selected entry.
        Open,
        /// Toggle focus on the project panel.
        ToggleFocus
    ]
);
