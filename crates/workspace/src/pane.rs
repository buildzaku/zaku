use std::sync::Arc;

use gpui::{Entity, SharedString, Window, div, prelude::*, rgb};

use http_client::{AsyncBody, HttpClient};
use input::InputField;
use reqwest_client::ReqwestClient;
use ui::{Button, ButtonCommon, ButtonSize, ButtonVariant, Clickable, FixedWidth, rems_from_px};

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
}

impl Render for Pane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.input.is_none() {
            let input = cx.new(|cx| InputField::new(window, cx, "https://example.com"));
            self.input = Some(input);
        }

        let input = self
            .input
            .clone()
            .expect("InputField should be initialized");
        let input_handle = input.clone();
        let http_client = self.http_client.clone();
        let pane_handle = cx.weak_entity();

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x1a1a1a))
            .p_3()
            .child("HTTP Request")
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .w_full()
                    .py_2()
                    .gap_2()
                    .child(div().flex_1().child(input))
                    .child(
                        Button::new("request-send", "Send")
                            .variant(ButtonVariant::Accent)
                            .size(ButtonSize::Large)
                            .width(rems_from_px(68.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .on_click(move |_, window, cx| {
                                let pane_handle = pane_handle.clone();
                                let request_url = input_handle.read(cx).text(cx).trim().to_string();
                                let http_client = http_client.clone();

                                if request_url.is_empty() {
                                    if let Err(error) = pane_handle.update(cx, |pane, cx| {
                                        pane.response_status = Some("Response -".into());
                                        cx.notify();
                                    }) {
                                        eprintln!("failed to update pane response status: {error}");
                                    }
                                    return;
                                }

                                window
                                    .spawn(cx, async move |cx| {
                                        let response = http_client
                                            .get(&request_url, AsyncBody::empty(), true)
                                            .await;
                                        let response_status = match response {
                                            Ok(response) => {
                                                format!("Response {}", response.status().as_u16())
                                            }
                                            Err(error) => format!("Response Error: {error}"),
                                        };

                                        if let Err(error) = pane_handle.update(cx, |pane, cx| {
                                            pane.response_status = Some(response_status.into());
                                            cx.notify();
                                        }) {
                                            eprintln!(
                                                "failed to update pane response status: {error}"
                                            );
                                        }
                                    })
                                    .detach();
                            }),
                    ),
            )
            .when_some(self.response_status.clone(), |this, response_status| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x9a9a9a))
                        .child(response_status),
                )
            })
            .child(div().flex_1())
    }
}
