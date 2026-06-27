use gpui::{
    Action, AnyElement, App, Context, ElementId, Entity, FocusHandle, Focusable, FontWeight,
    Pixels, Render, SharedString, Subscription, WeakEntity, Window, prelude::*,
};
use num_traits::ToPrimitive;
use std::{sync::Arc, time::Duration};

use editor::Editor;
use http_client::StatusCode;
use language::{Buffer, Language, PLAIN_TEXT};
use multi_buffer::MultiBuffer;
use theme::ActiveTheme;
use ui::{Color, DynamicSpacing, IconName, Label, LabelCommon, LabelSize, LineHeightStyle};
use workspace::{Panel, Workspace, pane::Pane};

pub fn init(cx: &mut App) {
    cx.observe_new(
        |workspace: &mut Workspace, _window, _: &mut Context<Workspace>| {
            workspace.register_action(
                |workspace, _: &actions::response_panel::ToggleFocus, window, cx| {
                    workspace.toggle_panel_focus::<ResponsePanel>(window, cx);
                },
            );
        },
    )
    .detach();
}

fn format_bytes_received(bytes_received: u64) -> SharedString {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    const DECIMAL_BYTE_UNIT: f64 = 1000.0;

    let mut value = bytes_received
        .to_f64()
        .expect("bytes received should fit in f64");
    let mut unit_index = 0;

    while value >= DECIMAL_BYTE_UNIT && unit_index < UNITS.len() - 1 {
        value /= DECIMAL_BYTE_UNIT;
        unit_index += 1;
    }

    let unit = UNITS
        .get(unit_index)
        .expect("bytes received unit should exist");
    if unit_index == 0 {
        format!("{bytes_received} {unit}").into()
    } else {
        format!("{value:.2} {unit}").into()
    }
}

fn format_elapsed_duration(elapsed_duration: Duration) -> SharedString {
    let total_seconds = elapsed_duration.as_secs();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = elapsed_duration.as_secs_f64() % 60.0;

    if elapsed_duration.as_millis() < 1000 {
        format!("{} ms", elapsed_duration.as_millis())
    } else if hours == 0 && minutes == 0 {
        format!("{:.2} s", elapsed_duration.as_secs_f64())
    } else if hours == 0 {
        format!("{minutes} m {seconds:.2} s")
    } else {
        format!("{hours} h {minutes} m {seconds:.2} s")
    }
    .into()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ResponsePanelTab {
    Body,
}

#[derive(Clone)]
struct ResponseSummary {
    label: SharedString,
    elapsed_duration: SharedString,
    bytes_received: SharedString,
}

#[derive(Clone, Default)]
pub enum ResponseState {
    #[default]
    Idle,
    Fetching {
        bytes_received: u64,
        elapsed_duration: Duration,
    },
    Completed {
        status_code: StatusCode,
        bytes_received: u64,
        elapsed_duration: Duration,
    },
    Error {
        bytes_received: u64,
        elapsed_duration: Duration,
    },
}

impl ResponseState {
    fn summary(&self) -> Option<ResponseSummary> {
        match self {
            ResponseState::Idle => None,
            ResponseState::Fetching {
                bytes_received,
                elapsed_duration,
            } => Some(ResponseSummary {
                label: "Fetching".into(),
                elapsed_duration: format_elapsed_duration(*elapsed_duration),
                bytes_received: format_bytes_received(*bytes_received),
            }),
            ResponseState::Completed {
                status_code,
                bytes_received,
                elapsed_duration,
            } => {
                let label = if let Some(reason_phrase) = status_code.canonical_reason() {
                    format!("{} {reason_phrase}", status_code.as_u16()).into()
                } else {
                    status_code.as_u16().to_string().into()
                };

                Some(ResponseSummary {
                    label,
                    elapsed_duration: format_elapsed_duration(*elapsed_duration),
                    bytes_received: format_bytes_received(*bytes_received),
                })
            }
            ResponseState::Error {
                bytes_received,
                elapsed_duration,
            } => Some(ResponseSummary {
                label: "Error".into(),
                elapsed_duration: format_elapsed_duration(*elapsed_duration),
                bytes_received: format_bytes_received(*bytes_received),
            }),
        }
    }
}

pub struct Response {
    request_id: usize,
    state: ResponseState,
    editor: Entity<Editor>,
    payload: Entity<MultiBuffer>,
}

impl Response {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (payload, editor) = Self::new_editor(window, cx);

        Self {
            request_id: 0,
            state: ResponseState::default(),
            editor,
            payload,
        }
    }

    fn new_editor(window: &mut Window, cx: &mut App) -> (Entity<MultiBuffer>, Entity<Editor>) {
        let payload = cx.new(move |cx| {
            let buffer = cx.new(|cx| Buffer::local("", cx).with_language(PLAIN_TEXT.clone(), cx));
            MultiBuffer::singleton(buffer, cx)
        });
        let editor = cx.new(|cx| {
            let mut editor = Editor::for_multibuffer(payload.clone(), window, cx);
            editor.set_read_only(true);
            editor
        });

        (payload, editor)
    }

    fn summary(&self) -> Option<ResponseSummary> {
        self.state.summary()
    }

    fn editor(&self) -> Entity<Editor> {
        self.editor.clone()
    }

    pub fn text(&self, cx: &App) -> String {
        self.payload.read(cx).snapshot(cx).text()
    }

    pub fn begin_response(&mut self, window: &mut Window, cx: &mut Context<Self>) -> usize {
        let was_focused = self.editor.focus_handle(cx).contains_focused(window, cx);
        let (payload, editor) = Self::new_editor(window, cx);
        let request_id = self.request_id.wrapping_add(1);

        self.request_id = request_id;
        self.state = ResponseState::default();
        self.editor = editor;
        self.payload = payload;
        if was_focused {
            let focus_handle = self.editor.focus_handle(cx);
            window.focus(&focus_handle, cx);
        }
        cx.notify();
        request_id
    }

    pub fn set_state(
        &mut self,
        request_id: usize,
        state: ResponseState,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.request_id != request_id {
            return false;
        }

        self.state = state;
        cx.notify();
        true
    }

    pub fn set_payload<T: Into<String>>(
        &mut self,
        request_id: usize,
        payload: T,
        language: Option<Arc<Language>>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.request_id != request_id {
            return false;
        }

        let language = language.unwrap_or_else(|| PLAIN_TEXT.clone());
        self.payload.update(cx, |payload_buffer, cx| {
            let payload = payload.into();
            if let Some(buffer) = payload_buffer.as_singleton() {
                let language = language.clone();
                buffer.update(cx, |buffer, cx| {
                    buffer.set_language(Some(language), cx);
                });
            }
            payload_buffer.set_text(payload, cx);
        });
        cx.notify();
        true
    }
}

pub struct ResponsePanel {
    focus_handle: FocusHandle,
    pane: WeakEntity<Pane>,
    active_tab: ResponsePanelTab,
    response: Option<Entity<Response>>,
    response_subscription: Option<Subscription>,
    _focus_subscription: Subscription,
}

impl ResponsePanel {
    const PANEL_KEY: &str = "ResponsePanel";
    const DEFAULT_SIZE: Pixels = gpui::px(440.0);

    pub fn new(pane: WeakEntity<Pane>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let focus_subscription = cx.on_focus(&focus_handle, window, |_, window, cx| {
            cx.on_next_frame(window, |response_panel, window, cx| {
                if response_panel.focus_handle.is_focused(window)
                    && let Some(editor) = response_panel.editor(cx)
                {
                    editor.focus_handle(cx).focus(window, cx);
                }
            });
        });

        Self {
            focus_handle,
            pane,
            active_tab: ResponsePanelTab::Body,
            response: None,
            response_subscription: None,
            _focus_subscription: focus_subscription,
        }
    }

    pub fn set_response(&mut self, response: Option<Entity<Response>>, cx: &mut Context<Self>) {
        let unchanged = match (&self.response, &response) {
            (Some(old_response), Some(new_response)) => old_response == new_response,
            (None, None) => true,
            _ => false,
        };
        if unchanged {
            return;
        }

        let _previous_subscription = self.response_subscription.take();
        self.response_subscription = response
            .as_ref()
            .map(|response| cx.observe(response, |_, _, cx| cx.notify()));
        self.response = response;
        cx.notify();
    }

    fn editor(&self, cx: &App) -> Option<Entity<Editor>> {
        self.response
            .as_ref()
            .map(|response| response.read(cx).editor())
    }

    pub fn text(&self, cx: &App) -> String {
        self.response
            .as_ref()
            .map_or_else(String::new, |response| response.read(cx).text(cx))
    }

    fn render_response_summary(response_summary: ResponseSummary) -> impl IntoElement {
        gpui::div()
            .flex()
            .items_center()
            .gap_1()
            .child(
                gpui::div()
                    .min_w(gpui::px(40.0))
                    .flex()
                    .justify_center()
                    .items_center()
                    .child(
                        Label::new(response_summary.label)
                            .size(LabelSize::Small)
                            .color(Color::Muted)
                            .single_line(),
                    ),
            )
            .child(Label::new("·").size(LabelSize::Small).color(Color::Muted))
            .child(
                gpui::div()
                    .min_w(gpui::px(40.0))
                    .flex()
                    .justify_center()
                    .items_center()
                    .child(
                        Label::new(response_summary.elapsed_duration)
                            .size(LabelSize::Small)
                            .color(Color::Muted)
                            .single_line(),
                    ),
            )
            .child(Label::new("·").size(LabelSize::Small).color(Color::Muted))
            .child(
                gpui::div()
                    .min_w(gpui::px(40.0))
                    .flex()
                    .justify_center()
                    .items_center()
                    .child(
                        Label::new(response_summary.bytes_received)
                            .size(LabelSize::Small)
                            .color(Color::Muted)
                            .single_line(),
                    ),
            )
    }

    fn render_tab_bar(
        &self,
        response_summary: Option<ResponseSummary>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active_tab = self.active_tab;
        let colors = cx.theme().colors();

        let tab =
            |id: ElementId, active: bool, label: SharedString, set_active_tab: ResponsePanelTab| {
                let colors = cx.theme().colors();

                gpui::div()
                    .id(id)
                    .relative()
                    .flex_none()
                    .flex()
                    .items_center()
                    .h(DynamicSpacing::Base24.px(cx))
                    .px(DynamicSpacing::Base08.px(cx))
                    .rounded_sm()
                    .border_1()
                    .when(active, |this| {
                        this.border_color(colors.border.opacity(0.25))
                            .bg(colors.panel_tab_active_background)
                    })
                    .when(!active, |this| {
                        this.border_color(gpui::transparent_black())
                            .bg(gpui::transparent_black())
                    })
                    .cursor_pointer()
                    .on_click(cx.listener(move |response_panel, _, _, cx| {
                        cx.stop_propagation();
                        if response_panel.active_tab != set_active_tab {
                            response_panel.active_tab = set_active_tab;
                            cx.notify();
                        }
                    }))
                    .child(
                        Label::new(label)
                            .size(LabelSize::Small)
                            .line_height_style(LineHeightStyle::UiLabel)
                            .weight(FontWeight::MEDIUM)
                            .color(if active {
                                Color::Custom(colors.panel_tab_active_foreground)
                            } else {
                                Color::Custom(colors.panel_tab_inactive_foreground)
                            })
                            .single_line(),
                    )
            };

        gpui::div()
            .id("response-panel-tabs")
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .h(DynamicSpacing::Base36.px(cx))
            .gap_1()
            .pl_1()
            .pr_3()
            .border_b_1()
            .border_color(colors.border)
            .bg(colors.panel_tab_bar_background)
            .child(tab(
                ElementId::Name("response-body-tab".into()),
                active_tab == ResponsePanelTab::Body,
                "Body".into(),
                ResponsePanelTab::Body,
            ))
            .when_some(response_summary, |this, response_summary| {
                this.child(Self::render_response_summary(response_summary))
            })
            .into_any_element()
    }
}

impl Focusable for ResponsePanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for ResponsePanel {
    fn persistent_name() -> &'static str {
        Self::PANEL_KEY
    }

    fn panel_key() -> &'static str {
        Self::PANEL_KEY
    }

    fn default_size(&self, _window: &Window, _: &App) -> Pixels {
        Self::DEFAULT_SIZE
    }

    fn icon(&self, _window: &Window, _: &App) -> Option<ui::IconName> {
        Some(IconName::Network)
    }

    fn icon_tooltip(&self, _window: &Window, _: &App) -> Option<&'static str> {
        Some("Response Panel")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        actions::response_panel::ToggleFocus.boxed_clone()
    }

    fn activation_priority(&self) -> u32 {
        2
    }

    fn enabled(&self, cx: &App) -> bool {
        self.pane
            .upgrade()
            .is_some_and(|pane| !pane.read(cx).should_display_welcome_page())
    }
}

impl Render for ResponsePanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let panel_background = cx.theme().colors().panel_background;
        let focus_handle = self.focus_handle(cx);
        let response_summary = self
            .response
            .as_ref()
            .and_then(|response| response.read(cx).summary());
        let editor = self.editor(cx);

        gpui::div()
            .track_focus(&focus_handle)
            .flex()
            .flex_col()
            .size_full()
            .bg(panel_background)
            .child(self.render_tab_bar(response_summary, cx))
            .child(
                gpui::div()
                    .flex_1()
                    .min_h_0()
                    .bg(panel_background)
                    .when_some(editor, |this, editor| this.child(editor)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Duration;

    #[test]
    fn test_format_bytes_received() {
        assert_eq!(format_bytes_received(0).to_string(), "0 B");
        assert_eq!(format_bytes_received(999).to_string(), "999 B");
        assert_eq!(format_bytes_received(1000).to_string(), "1.00 KB");
        assert_eq!(format_bytes_received(1545).to_string(), "1.54 KB");
        assert_eq!(format_bytes_received(1_047_100).to_string(), "1.05 MB");
        assert_eq!(format_bytes_received(1_384_900_000).to_string(), "1.38 GB");
    }

    #[test]
    fn test_format_elapsed_duration() {
        assert_eq!(
            format_elapsed_duration(Duration::from_millis(570)).to_string(),
            "570 ms"
        );
        assert_eq!(
            format_elapsed_duration(Duration::from_millis(999)).to_string(),
            "999 ms"
        );
        assert_eq!(
            format_elapsed_duration(Duration::from_secs(1)).to_string(),
            "1.00 s"
        );
        assert_eq!(
            format_elapsed_duration(Duration::from_millis(3865)).to_string(),
            "3.87 s"
        );
        assert_eq!(
            format_elapsed_duration(Duration::from_millis(63_430)).to_string(),
            "1 m 3.43 s"
        );
        assert_eq!(
            format_elapsed_duration(Duration::from_millis(3_723_430)).to_string(),
            "1 h 2 m 3.43 s"
        );
    }
}
