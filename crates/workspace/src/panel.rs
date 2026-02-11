pub mod project;
pub mod response;

pub mod project_panel {
    use gpui::actions;

    actions!(project_panel, [ToggleFocus]);
}

pub mod response_panel {
    use gpui::actions;

    actions!(response_panel, [ToggleFocus]);
}

pub use project::ProjectPanel;
pub use response::ResponsePanel;
