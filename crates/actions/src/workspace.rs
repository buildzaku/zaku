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
        /// Open a new window.
        NewWindow,
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

pub mod project_panel {
    gpui::actions!(
        project_panel,
        [
            /// Toggle focus on the project panel.
            ToggleFocus
        ]
    );
}

pub mod response_panel {
    gpui::actions!(
        response_panel,
        [
            /// Toggle focus on the response panel.
            ToggleFocus
        ]
    );
}

pub mod welcome {
    use gpui::Action;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    /// Open the recent project at the given index.
    #[derive(PartialEq, Clone, Debug, Deserialize, Serialize, JsonSchema, Action)]
    #[action(namespace = welcome)]
    #[serde(transparent)]
    pub struct OpenRecentProject {
        pub index: usize,
    }
}
