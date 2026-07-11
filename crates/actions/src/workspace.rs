use gpui::Action;
use schemars::JsonSchema;
use serde::Deserialize;

pub use welcome::OpenRecentProject;

/// Opens a project directory.
#[derive(Clone, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = workspace)]
pub struct Open {
    #[serde(default = "Open::default_create_new_window")]
    pub create_new_window: bool,
}

impl Open {
    pub const DEFAULT: Self = Self {
        create_new_window: false,
    };

    fn default_create_new_window() -> bool {
        Self::DEFAULT.create_new_window
    }
}

impl Default for Open {
    fn default() -> Self {
        Self::DEFAULT
    }
}

gpui::actions!(
    workspace,
    [
        /// Close the current project.
        CloseProject,
        /// Close the current window.
        CloseWindow,
        /// Copy the selected item's absolute path.
        CopyPath,
        /// Copy the selected item's relative path.
        CopyRelativePath,
        /// Create a new project.
        NewProject,
        /// Open a new window.
        NewWindow,
        /// Save the active item.
        Save,
        /// Send the current request.
        SendRequest,
        /// Suppress the current notification.
        SuppressNotification,
        /// Toggle the bottom dock.
        ToggleBottomDock,
        /// Toggle the left dock.
        ToggleLeftDock
    ]
);

pub mod welcome {
    use gpui::Action;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    /// Open the recent project at the given index.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Action)]
    #[action(namespace = welcome)]
    #[serde(transparent)]
    pub struct OpenRecentProject {
        pub index: usize,
    }
}
