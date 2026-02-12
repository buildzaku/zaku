use futures::io::AsyncReadExt;
use gpui::{App, Context, Entity, EntityId, FocusHandle, Focusable, Window, prelude::*};
use std::sync::Arc;

use http_client::{AsyncBody, HttpClient};
use input::InputField;
use reqwest_client::ReqwestClient;
use theme::ActiveTheme;
use ui::{Button, ButtonCommon, ButtonSize, ButtonVariant, Clickable, FixedWidth, Label};

use crate::{SendRequest, dock::Dock, panel::ResponsePanel};

pub struct Pane {
    focus_handle: FocusHandle,
    input: Option<Entity<InputField>>,
    http_client: Arc<dyn HttpClient>,
    bottom_dock: Option<Entity<Dock>>,
    response_panel_id: Option<EntityId>,
    response_panel: Option<Entity<ResponsePanel>>,
}

impl Pane {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            input: None,
            http_client: Arc::new(ReqwestClient::new()),
            bottom_dock: None,
            response_panel_id: None,
            response_panel: None,
        }
    }

    pub fn set_response_targets(
        &mut self,
        bottom_dock: Entity<Dock>,
        response_panel: Entity<ResponsePanel>,
        cx: &mut Context<Self>,
    ) {
        self.bottom_dock = Some(bottom_dock);
        self.response_panel_id = Some(Entity::entity_id(&response_panel));
        self.response_panel = Some(response_panel);
        cx.notify();
    }

    fn send_request(&mut self, request_url: String, window: &mut Window, cx: &mut Context<Self>) {
        let request_url = request_url.trim().to_string();
        if request_url.is_empty() {
            return;
        }

        let request_url =
            if request_url.starts_with("http://") || request_url.starts_with("https://") {
                request_url
            } else {
                format!("https://{request_url}")
            };

        if let (Some(bottom_dock), Some(response_panel_id)) =
            (self.bottom_dock.as_ref(), self.response_panel_id)
        {
            bottom_dock.update(cx, |dock, cx| {
                dock.activate_panel(response_panel_id, cx);
            });
        }

        if let Some(response_panel) = self.response_panel.as_ref() {
            response_panel.update(cx, |panel, cx| {
                panel.set_response("...".into(), "Status: Sending...".into(), cx);
            });
        }
        cx.notify();

        let http_client = self.http_client.clone();
        let response_panel = self.response_panel.clone();

        window
            .spawn(cx, async move |cx| {
                let response = http_client
                    .get(&request_url, AsyncBody::empty(), true)
                    .await;
                let (response_text, response_status) = match response {
                    Ok(mut response) => {
                        let status = response.status();
                        let response_status = if let Some(reason) = status.canonical_reason() {
                            format!("Status: {} {}", status.as_u16(), reason)
                        } else {
                            format!("Status: {}", status.as_u16())
                        };

                        let body = response.body_mut();
                        let mut bytes = Vec::new();
                        let read_result = body.read_to_end(&mut bytes).await;
                        let body_text = match read_result {
                            Ok(_) => String::from_utf8_lossy(&bytes).to_string(),
                            Err(error) => format!("(failed to read response body: {error})"),
                        };

                        (body_text, response_status)
                    }
                    Err(error) => {
                        let error_text = format!("Error: {error}");
                        (error_text.clone(), "Status: Error".to_string())
                    }
                };

                if let Some(response_panel) = response_panel.as_ref() {
                    response_panel.update(cx, |panel, cx| {
                        panel.set_response(response_text.into(), response_status.into(), cx);
                    });
                }
            })
            .detach();
    }
}

impl Focusable for Pane {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Pane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.input.is_none() {
            let input = cx.new(|cx| InputField::new(window, cx, "https://example.com"));
            self.input = Some(input);
        }

        let theme_colors = cx.theme().colors();
        let input = self
            .input
            .clone()
            .expect("InputField should be initialized");
        let input_handle = input.clone();
        let input_handle_for_action = input.clone();

        gpui::div()
            .track_focus(&self.focus_handle)
            .flex()
            .flex_col()
            .size_full()
            .bg(theme_colors.panel_background)
            .p_3()
            .child(Label::new("HTTP Request"))
            .child(
                gpui::div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .w_full()
                    .py_2()
                    .gap_2()
                    .key_context("RequestUrl")
                    .on_action(cx.listener(move |pane, _: &SendRequest, window, cx| {
                        let request_url = input_handle_for_action.read(cx).text(cx);
                        pane.send_request(request_url, window, cx);
                    }))
                    .child(gpui::div().flex_1().child(input))
                    .child(
                        Button::new("request-send", "Send")
                            .variant(ButtonVariant::Accent)
                            .size(ButtonSize::Large)
                            .width(ui::rems_from_px(60.))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .on_click(cx.listener(move |pane, _, window, cx| {
                                let request_url = input_handle.read(cx).text(cx);
                                pane.send_request(request_url, window, cx);
                            })),
                    ),
            )
            .child(
                gpui::div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .overflow_hidden()
                    .child(gpui::div().flex_1()),
            )
    }
}
