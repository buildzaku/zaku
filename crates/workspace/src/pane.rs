use futures::{FutureExt, io::AsyncReadExt};
use gpui::{
    App, Context, Corner, Entity, FocusHandle, Focusable, FontWeight, WeakEntity, Window,
    prelude::*,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use http_client::{AsyncBody, Builder, HttpClient, HttpRequestExt, Method, RedirectPolicy};
use input::InputField;
use reqwest_client::ReqwestClient;
use theme::ActiveTheme;
use ui::{
    Button, ButtonCommon, ButtonSize, ButtonVariant, Clickable, ContextMenu, DropdownMenu,
    DropdownStyle, FixedWidth, IconPosition, Label,
};

use crate::{SendRequest, Workspace, panel::response::ResponseState};

fn normalize_url(url: String) -> Option<String> {
    let url = url.trim().to_string();
    if url.is_empty() {
        return None;
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        Some(url)
    } else {
        Some(format!("http://{url}"))
    }
}

#[derive(Clone)]
struct Request {
    method: Method,
    url: String,
}

impl Default for Request {
    fn default() -> Self {
        Self {
            method: Method::GET,
            url: String::new(),
        }
    }
}

pub struct Pane {
    focus_handle: FocusHandle,
    workspace: WeakEntity<Workspace>,
    http_client: Arc<dyn HttpClient>,
    request: Request,
    input_field: Entity<InputField>,
}

impl Pane {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            workspace,
            http_client: Arc::new(ReqwestClient::new()),
            request: Request::default(),
            input_field: cx.new(|cx| InputField::new(window, cx, "https://example.com")),
        }
    }

    fn send_request(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.request.url = self.input_field.read(cx).text(cx);

        let Ok(response_panel) = self
            .workspace
            .update(cx, |workspace, cx| workspace.open_response_panel(cx))
        else {
            return;
        };

        let request_id = response_panel.update(cx, |response_panel, cx| {
            let request_id = response_panel.begin_response(window, cx);
            response_panel.set_state(
                request_id,
                ResponseState::Fetching {
                    bytes_received: 0,
                    elapsed_duration: Duration::default(),
                },
                cx,
            );
            request_id
        });

        let request_started_at = Instant::now();
        let http_client = self.http_client.clone();

        window
            .spawn(cx, {
                let request = self.request.clone();
                let response_panel = response_panel.clone();
                async move |cx| {
                    let normalized_url = match normalize_url(request.url) {
                        Some(normalized_url) => normalized_url,
                        None => {
                            if let Err(error) = response_panel.update_in(cx, |response_panel, _, cx| {
                                response_panel.set_state(
                                    request_id,
                                    ResponseState::Error {
                                        bytes_received: 0,
                                        elapsed_duration: request_started_at.elapsed(),
                                    },
                                    cx,
                                );
                                response_panel.set_payload(request_id, "Error: invalid URL", cx);
                            }) {
                                eprintln!("failed to update response panel: {error:?}");
                            }
                            return;
                        }
                    };
                    let request = match Builder::new()
                        .method(request.method)
                        .uri(normalized_url)
                        .follow_redirects(RedirectPolicy::FollowAll)
                        .body(AsyncBody::empty())
                    {
                        Ok(request) => request,
                        Err(error) => {
                            if let Err(error) = response_panel.update_in(cx, |response_panel, _, cx| {
                                response_panel.set_state(
                                    request_id,
                                    ResponseState::Error {
                                        bytes_received: 0,
                                        elapsed_duration: request_started_at.elapsed(),
                                    },
                                    cx,
                                );
                                response_panel.set_payload(request_id, format!("Error: {error}"), cx);
                            }) {
                                eprintln!("failed to update response panel: {error:?}");
                            }
                            return;
                        }
                    };

                    let progress_timer =
                        cx.background_executor().timer(Duration::from_millis(50)).fuse();
                    futures::pin_mut!(progress_timer);

                    let send_request = http_client.send(request).fuse();
                    futures::pin_mut!(send_request);

                    let mut response = loop {
                        futures::select_biased! {
                            response = send_request => {
                                match response {
                                    Ok(response) => break response,
                                    Err(error) => {
                                        if let Err(error) = response_panel.update_in(cx, |response_panel, _, cx| {
                                            response_panel.set_state(request_id, ResponseState::Error {
                                                bytes_received: 0,
                                                elapsed_duration: request_started_at.elapsed(),
                                            }, cx);
                                            response_panel.set_payload(request_id, format!("Error: {error}"), cx);
                                        }) {
                                            eprintln!("failed to update response panel: {error:?}");
                                        }
                                        return;
                                    }
                                }
                            }
                            _ = progress_timer => {
                                let still_active = response_panel.update(cx, |response_panel, cx| {
                                    response_panel.set_state(
                                        request_id,
                                        ResponseState::Fetching {
                                            bytes_received: 0,
                                            elapsed_duration: request_started_at.elapsed(),
                                        },
                                        cx,
                                    )
                                });
                                if !still_active {
                                    return;
                                }

                                progress_timer.set(
                                    cx.background_executor().timer(Duration::from_millis(50)).fuse(),
                                );
                            }
                        }
                    };

                    let status_code = response.status();
                    let mut bytes_received = 0;
                    let mut payload = Vec::new();
                    let mut buffer = [0; 8192];
                    let mut read_error = None;

                    loop {
                        let read_response_body = response.body_mut().read(&mut buffer).fuse();
                        futures::pin_mut!(read_response_body);

                        futures::select_biased! {
                            read_result = read_response_body => {
                                match read_result {
                                    Ok(0) => break,
                                    Ok(chunk) => {
                                        bytes_received += chunk;
                                        payload.extend_from_slice(&buffer[..chunk]);
                                    }
                                    Err(error) => {
                                        read_error = Some(error);
                                        break;
                                    }
                                }
                            }
                            _ = progress_timer => {
                                let still_active = response_panel.update(cx, |response_panel, cx| {
                                    response_panel.set_state(
                                        request_id,
                                        ResponseState::Fetching {
                                            bytes_received,
                                            elapsed_duration: request_started_at.elapsed(),
                                        },
                                        cx,
                                    )
                                });
                                if !still_active {
                                    return;
                                }

                                progress_timer.set(
                                    cx.background_executor().timer(Duration::from_millis(50)).fuse(),
                                );
                            }
                        }
                    }

                    let payload = match read_error {
                        Some(error) => format!("(failed to read response body: {error})"),
                        None => String::from_utf8_lossy(&payload).into_owned(),
                    };
                    let response_state = ResponseState::Completed {
                        status_code,
                        bytes_received,
                        elapsed_duration: request_started_at.elapsed(),
                    };

                    if let Err(error) = response_panel.update_in(cx, |response_panel, _, cx| {
                        response_panel.set_state(request_id, response_state, cx);
                        response_panel.set_payload(request_id, payload, cx);
                    }) {
                        eprintln!("failed to update response panel: {error:?}");
                    }
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
        let input_field = self.input_field.clone();
        let request_method_menu = {
            let available_request_methods = [
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::PATCH,
                Method::DELETE,
                Method::HEAD,
                Method::OPTIONS,
            ];
            let selected_request_method = self.request.method.clone();
            let pane = cx.weak_entity();

            ContextMenu::build(window, cx, move |menu, _, _| {
                let mut menu = menu;
                for request_method in available_request_methods {
                    let toggled = request_method == selected_request_method;
                    let pane = pane.clone();
                    let request_method_for_handler = request_method.clone();
                    menu = menu.toggleable_entry(
                        request_method.as_str().to_owned(),
                        toggled,
                        IconPosition::End,
                        None,
                        move |_, cx| {
                            if let Err(error) = pane.update(cx, |pane, cx| {
                                pane.request.method = request_method_for_handler.clone();
                                cx.notify();
                            }) {
                                eprintln!("failed to update request method: {error:?}");
                            }
                        },
                    );
                }
                menu
            })
        };

        let theme_colors = cx.theme().colors();

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
                        pane.send_request(window, cx);
                    }))
                    .child(
                        DropdownMenu::new(
                            "request-method",
                            self.request.method.as_str().to_owned(),
                            request_method_menu,
                        )
                        .style(DropdownStyle::Outlined)
                        .attach(Corner::BottomLeft)
                        .offset(gpui::point(gpui::px(0.0), gpui::px(0.5)))
                        .trigger_size(ButtonSize::Large),
                    )
                    .child(gpui::div().flex_1().child(input_field))
                    .child(
                        Button::new("request-send", "Send")
                            .variant(ButtonVariant::Accent)
                            .size(ButtonSize::Large)
                            .width(ui::rems_from_px(60.0))
                            .font_weight(FontWeight::MEDIUM)
                            .on_click(cx.listener(move |pane, _, window, cx| {
                                pane.send_request(window, cx);
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
