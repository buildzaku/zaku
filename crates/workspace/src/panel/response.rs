use gpui::{
    Action, App, Context, Entity, FocusHandle, Focusable, Pixels, Render, SharedString,
    Subscription, Window, prelude::*,
};
use std::time::Duration;

use editor::Editor;
use http_client::StatusCode;
use multi_buffer::MultiBuffer;
use theme::ActiveTheme;
use ui::{Color, IconName, Label, LabelCommon, LabelSize};

use crate::{
    DockPosition,
    panel::{Panel, response_panel},
};

#[derive(Clone, Default)]
pub(crate) enum ResponseStatus {
    #[default]
    Idle,
    Fetching {
        bytes_received: usize,
        elapsed_duration: Duration,
    },
    Completed {
        status_code: StatusCode,
        bytes_received: usize,
        elapsed_duration: Duration,
    },
    Error {
        bytes_received: usize,
        elapsed_duration: Duration,
    },
}

impl ResponseStatus {
    fn label(&self) -> Option<SharedString> {
        match self {
            ResponseStatus::Idle => None,
            ResponseStatus::Fetching { .. } => Some("Fetching".into()),
            ResponseStatus::Completed { status_code, .. } => {
                if let Some(reason_phrase) = status_code.canonical_reason() {
                    Some(format!("{} {}", status_code.as_u16(), reason_phrase).into())
                } else {
                    Some(status_code.as_u16().to_string().into())
                }
            }
            ResponseStatus::Error { .. } => Some("Error".into()),
        }
    }

    fn elapsed_duration_label(&self) -> Option<SharedString> {
        let elapsed_duration = match self {
            ResponseStatus::Idle => return None,
            ResponseStatus::Fetching {
                elapsed_duration, ..
            }
            | ResponseStatus::Completed {
                elapsed_duration, ..
            }
            | ResponseStatus::Error {
                elapsed_duration, ..
            } => elapsed_duration,
        };

        Some(if *elapsed_duration < Duration::from_secs(1) {
            format!("{} ms", elapsed_duration.as_millis()).into()
        } else {
            format!("{:.2} s", elapsed_duration.as_secs_f64()).into()
        })
    }

    fn bytes_received_label(&self) -> Option<SharedString> {
        const DECIMAL_BYTE_UNIT: f64 = 1000.0;

        let bytes_received = match self {
            ResponseStatus::Idle => return None,
            ResponseStatus::Fetching { bytes_received, .. }
            | ResponseStatus::Completed { bytes_received, .. }
            | ResponseStatus::Error { bytes_received, .. } => *bytes_received as f64,
        };

        Some(if bytes_received < DECIMAL_BYTE_UNIT {
            format!("{} B", bytes_received as usize).into()
        } else if bytes_received < DECIMAL_BYTE_UNIT * DECIMAL_BYTE_UNIT {
            format!("{:.2} KB", bytes_received / DECIMAL_BYTE_UNIT).into()
        } else if bytes_received < DECIMAL_BYTE_UNIT * DECIMAL_BYTE_UNIT * DECIMAL_BYTE_UNIT {
            format!(
                "{:.2} MB",
                bytes_received / (DECIMAL_BYTE_UNIT * DECIMAL_BYTE_UNIT)
            )
            .into()
        } else {
            format!(
                "{:.2} GB",
                bytes_received / (DECIMAL_BYTE_UNIT * DECIMAL_BYTE_UNIT * DECIMAL_BYTE_UNIT)
            )
            .into()
        })
    }
}

#[derive(Clone, Default)]
struct ResponseState {
    request_id: usize,
    status: ResponseStatus,
    payload: Option<Entity<MultiBuffer>>,
}

pub struct ResponsePanel {
    focus_handle: FocusHandle,
    position: DockPosition,
    size: Pixels,
    response_state: ResponseState,
    editor: Entity<Editor>,
    _subscriptions: Vec<Subscription>,
}

impl ResponsePanel {
    const DEFAULT_SIZE: Pixels = gpui::px(440.0);

    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let payload = cx.new(move |cx| MultiBuffer::singleton(editor::local_buffer("", cx), cx));
        let editor = cx.new(|cx| {
            let mut editor = Editor::for_multibuffer(payload.clone(), window, cx);
            editor.set_read_only(true);
            editor
        });
        let focus_handle = cx.focus_handle();
        let focus_subscription =
            cx.on_focus(&focus_handle, window, |response_panel, window, cx| {
                window.focus(&response_panel.editor.focus_handle(cx), cx);
            });

        Self {
            focus_handle,
            position: DockPosition::Bottom,
            size: Self::DEFAULT_SIZE,
            response_state: ResponseState {
                payload: Some(payload),
                ..Default::default()
            },
            editor,
            _subscriptions: vec![focus_subscription],
        }
    }

    pub(crate) fn begin_response(&mut self, window: &mut Window, cx: &mut Context<Self>) -> usize {
        let was_focused = self.focus_handle.is_focused(window)
            || self.editor.focus_handle(cx).contains_focused(window, cx);
        let payload = cx.new(move |cx| MultiBuffer::singleton(editor::local_buffer("", cx), cx));
        let editor = cx.new(|cx| {
            let mut editor = Editor::for_multibuffer(payload.clone(), window, cx);
            editor.set_read_only(true);
            editor
        });
        let request_id = self.response_state.request_id.wrapping_add(1);

        self.response_state = ResponseState {
            request_id,
            status: ResponseStatus::default(),
            payload: Some(payload),
        };
        self.editor = editor;
        if was_focused {
            let focus_handle = self.editor.focus_handle(cx);
            window.focus(&focus_handle, cx);
        }
        request_id
    }

    pub(crate) fn set_status(
        &mut self,
        request_id: usize,
        status: ResponseStatus,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.response_state.request_id != request_id {
            return false;
        }

        self.response_state.status = status;
        cx.notify();
        true
    }

    pub(crate) fn set_payload<T: Into<String>>(
        &mut self,
        request_id: usize,
        payload: T,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.response_state.request_id != request_id {
            return false;
        }

        let Some(payload_buffer) = self.response_state.payload.as_ref() else {
            return false;
        };

        payload_buffer.update(cx, |payload_buffer, cx| {
            payload_buffer.set_text(payload.into(), cx);
        });
        cx.notify();
        true
    }
}

impl Focusable for ResponsePanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
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
        Some(IconName::Network)
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
        let label = self.response_state.status.label();
        let elapsed_duration = self.response_state.status.elapsed_duration_label();
        let bytes_received = self.response_state.status.bytes_received_label();
        let show_elapsed_duration_separator = label.is_some();
        let show_bytes_received_separator = label.is_some() || elapsed_duration.is_some();
        let editor = self.editor.clone();

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
                    .justify_between()
                    .border_b_1()
                    .border_color(theme_colors.border_variant)
                    .child(Label::new("Response").size(LabelSize::Small))
                    .child(
                        gpui::div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .when_some(label, |container, label| {
                                container.child(
                                    gpui::div()
                                        .min_w(gpui::px(40.0))
                                        .flex()
                                        .justify_center()
                                        .items_center()
                                        .child(
                                            Label::new(label)
                                                .size(LabelSize::Small)
                                                .color(Color::Muted)
                                                .single_line(),
                                        ),
                                )
                            })
                            .when_some(elapsed_duration, |container, elapsed_duration| {
                                let container = if show_elapsed_duration_separator {
                                    container.child(
                                        Label::new("·").size(LabelSize::Small).color(Color::Muted),
                                    )
                                } else {
                                    container
                                };

                                container.child(
                                    gpui::div()
                                        .min_w(gpui::px(40.0))
                                        .flex()
                                        .justify_center()
                                        .items_center()
                                        .child(
                                            Label::new(elapsed_duration)
                                                .size(LabelSize::Small)
                                                .color(Color::Muted)
                                                .single_line(),
                                        ),
                                )
                            })
                            .when_some(bytes_received, |container, bytes_received| {
                                let container = if show_bytes_received_separator {
                                    container.child(
                                        Label::new("·").size(LabelSize::Small).color(Color::Muted),
                                    )
                                } else {
                                    container
                                };

                                container.child(
                                    gpui::div()
                                        .min_w(gpui::px(40.0))
                                        .flex()
                                        .justify_center()
                                        .items_center()
                                        .child(
                                            Label::new(bytes_received)
                                                .size(LabelSize::Small)
                                                .color(Color::Muted)
                                                .single_line(),
                                        ),
                                )
                            }),
                    ),
            )
            .child(editor)
    }
}
