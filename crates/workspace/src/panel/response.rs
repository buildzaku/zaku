use gpui::{
    Action, App, Context, Entity, FocusHandle, Focusable, Pixels, Render, SharedString, Window,
    prelude::*,
};

use editor::Editor;
use theme::ActiveTheme;
use ui::{Color, Label, LabelCommon, LabelSize};

use crate::{
    DockPosition,
    panel::{Panel, response_panel},
};

pub struct ResponsePanel {
    position: DockPosition,
    size: Pixels,
    response: Option<SharedString>,
    response_status: SharedString,
    response_editor: Entity<Editor>,
}

impl ResponsePanel {
    const DEFAULT_SIZE: Pixels = gpui::px(250.0);

    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let response_editor = cx.new(|cx| Editor::full(window, cx));
        Self {
            position: DockPosition::Bottom,
            size: Self::DEFAULT_SIZE,
            response: None,
            response_status: "Status: Idle".into(),
            response_editor,
        }
    }

    pub(crate) fn set_response(
        &mut self,
        response: SharedString,
        response_status: SharedString,
        cx: &mut Context<Self>,
    ) {
        self.response = Some(response);
        self.response_status = response_status;

        let response = self.response.clone().unwrap_or_else(|| "".into());
        self.response_editor.update(cx, |editor, cx| {
            editor.set_text(response.as_str(), cx);
        });
        cx.notify();
    }
}

impl Focusable for ResponsePanel {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.response_editor.read(cx).focus_handle(cx)
    }
}

impl Panel for ResponsePanel {
    fn persistent_name() -> &'static str {
        "ResponsePanel"
    }

    fn position(&self, _window: &Window, _: &App) -> DockPosition {
        self.position
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        position == DockPosition::Bottom
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
        Some(ui::IconName::Response)
    }

    fn icon_tooltip(&self, _window: &Window, _: &App) -> Option<&'static str> {
        Some("Response Panel")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        response_panel::ToggleFocus.boxed_clone()
    }
}

impl Render for ResponsePanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let response_status = self.response_status.clone();

        let theme_colors = cx.theme().colors();
        let response_editor = self.response_editor.clone();

        let focus_handle = self.focus_handle(cx);
        gpui::div()
            .track_focus(&focus_handle)
            .flex()
            .flex_col()
            .size_full()
            .bg(theme_colors.surface_background)
            .child(
                gpui::div()
                    .w_full()
                    .h(gpui::px(26.0))
                    .px_3()
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(theme_colors.border_variant)
                    .child(
                        Label::new(response_status)
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    ),
            )
            .child(response_editor)
    }
}
