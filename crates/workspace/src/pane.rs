use gpui::{Entity, SharedString, Window, prelude::*};
use std::sync::Arc;

use http_client::{AsyncBody, HttpClient};
use input::InputField;
use reqwest_client::ReqwestClient;
use theme::ActiveTheme;
use ui::{Button, ButtonCommon, ButtonSize, ButtonVariant, Clickable, FixedWidth, rems_from_px};

use crate::SendRequest;

pub struct Pane {
    input: Option<Entity<InputField>>,
    http_client: Arc<dyn HttpClient>,
    response_status: Option<SharedString>,
}

impl Pane {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            input: None,
            http_client: Arc::new(ReqwestClient::new()),
            response_status: None,
        }
    }

    fn send_request(&mut self, request_url: String, window: &mut Window, cx: &mut Context<Self>) {
        let request_url = request_url.trim().to_string();
        if request_url.is_empty() {
            self.response_status = None;
            cx.notify();
            return;
        }

        let request_url =
            if request_url.starts_with("http://") || request_url.starts_with("https://") {
                request_url
            } else {
                format!("https://{request_url}")
            };

        self.response_status = Some("...".into());
        cx.notify();

        let http_client = self.http_client.clone();
        let pane_handle = cx.weak_entity();

        window
            .spawn(cx, async move |cx| {
                let response = http_client
                    .get(&request_url, AsyncBody::empty(), true)
                    .await;
                let response_status = match response {
                    Ok(response) => format!("Response {}", response.status().as_u16()),
                    Err(error) => format!("Error: {error}"),
                };

                if let Err(error) = pane_handle.update(cx, |pane, cx| {
                    pane.response_status = Some(response_status.into());
                    cx.notify();
                }) {
                    eprintln!("failed to update pane response status: {error}");
                }
            })
            .detach();
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
            .flex()
            .flex_col()
            .size_full()
            .bg(theme_colors.panel_background)
            .p_3()
            .child("HTTP Request")
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
                            .width(rems_from_px(70.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .on_click(cx.listener(move |pane, _, window, cx| {
                                let request_url = input_handle.read(cx).text(cx);
                                pane.send_request(request_url, window, cx);
                            })),
                    ),
            )
            .when_some(self.response_status.clone(), |this, response_status| {
                this.child(
                    gpui::div()
                        .text_xs()
                        .text_color(theme_colors.text_muted)
                        .child(response_status),
                )
            })
            .child(gpui::div().flex_1())
    }
}
