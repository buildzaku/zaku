use gpui::{Action, App, Context, FocusHandle, Focusable, Pixels, Render, Window, prelude::*};

use theme::ActiveTheme;

use crate::{DockPosition, dock::Panel, panel::project_panel};

pub struct ProjectPanel {
    focus_handle: FocusHandle,
    position: DockPosition,
    size: Pixels,
}

impl ProjectPanel {
    const DEFAULT_SIZE: Pixels = gpui::px(250.);

    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            position: DockPosition::Left,
            size: Self::DEFAULT_SIZE,
        }
    }
}

impl Focusable for ProjectPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for ProjectPanel {
    fn persistent_name() -> &'static str {
        "ProjectPanel"
    }

    fn position(&self, _window: &Window, _: &App) -> DockPosition {
        self.position
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        position == DockPosition::Left
    }

    fn set_position(&mut self, position: DockPosition, _: &mut Window, cx: &mut Context<Self>) {
        if self.position_is_valid(position) {
            self.position = position;
            cx.notify();
        }
    }

    fn size(&self, _window: &Window, _: &App) -> Pixels {
        self.size
    }

    fn set_size(&mut self, size: Option<Pixels>, _window: &mut Window, cx: &mut Context<Self>) {
        self.size = size.unwrap_or(Self::DEFAULT_SIZE).round();
        cx.notify();
    }

    fn icon(&self, _window: &Window, _: &App) -> Option<ui::IconName> {
        Some(ui::IconName::Tree)
    }

    fn icon_tooltip(&self, _window: &Window, _: &App) -> Option<&'static str> {
        Some("Project Panel")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        project_panel::ToggleFocus.boxed_clone()
    }
}

impl Render for ProjectPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme_colors = cx.theme().colors();

        gpui::div()
            .track_focus(&self.focus_handle)
            .flex()
            .flex_col()
            .size_full()
            .bg(theme_colors.surface_background)
            .child(gpui::div().flex_1())
    }
}
