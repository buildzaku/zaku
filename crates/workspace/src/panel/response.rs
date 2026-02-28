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
    focus_handle: FocusHandle,
    position: DockPosition,
    size: Pixels,
    response_status: SharedString,
    response_editor: Option<Entity<Editor>>,
}

impl ResponsePanel {
    const DEFAULT_SIZE: Pixels = gpui::px(440.0);

    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            position: DockPosition::Bottom,
            size: Self::DEFAULT_SIZE,
            response_status: "Status: Idle".into(),
            response_editor: None,
        }
    }

    pub(crate) fn begin_response(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.response_editor.take();
        self.response_editor = Some(cx.new(|cx| Editor::full(window, cx)));
    }

    pub(crate) fn set_response_status(
        &mut self,
        response_status: SharedString,
        cx: &mut Context<Self>,
    ) {
        self.response_status = response_status;
        cx.notify();
    }

    pub(crate) fn set_response_payload(&mut self, response: SharedString, cx: &mut Context<Self>) {
        let Some(response_editor) = self.response_editor.as_ref() else {
            return;
        };

        response_editor.update(cx, |editor, cx| {
            editor.set_text(response.as_str(), cx);
            let transaction_id = editor.last_transaction_id(cx);
            if let Some(transaction_id) = transaction_id {
                editor.forget_transaction(transaction_id, cx);
            }
        });
        cx.notify();
    }
}

impl Focusable for ResponsePanel {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        if let Some(editor) = &self.response_editor {
            return editor.read(cx).focus_handle(cx);
        }
        self.focus_handle.clone()
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
        let theme_colors = cx.theme().colors();
        let focus_handle = self.focus_handle(cx);
        let response_status = self.response_status.clone();
        let response_editor = self.response_editor.clone();

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
            .when_some(response_editor, |container, response_editor| {
                container.child(response_editor)
            })
    }
}
