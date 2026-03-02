use futures::io::AsyncReadExt;
use gpui::{
    App, Context, Corner, Entity, FocusHandle, Focusable, FontWeight, WeakEntity, Window,
    prelude::*,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
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

struct RequestState {
    completed: AtomicBool,
    bytes_received: AtomicUsize,
}

impl RequestState {
    fn new() -> Self {
        Self {
            completed: AtomicBool::new(false),
            bytes_received: AtomicUsize::new(0),
        }
    }

    fn completed(&self) -> bool {
        self.completed.load(Ordering::Relaxed)
    }

    fn set_completed(&self) {
        self.completed.store(true, Ordering::Relaxed);
    }

    fn bytes_received(&self) -> usize {
        self.bytes_received.load(Ordering::Relaxed)
    }

    fn append_bytes_received(&self, bytes: usize) {
        self.bytes_received.fetch_add(bytes, Ordering::Relaxed);
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

impl Request {
    async fn send(
        &self,
        http_client: Arc<dyn HttpClient>,
        request_state: &RequestState,
        started_at: Instant,
    ) -> (String, ResponseState) {
        let Some(normalized_url) = normalize_url(self.url.clone()) else {
            return (
                "Error: invalid URL".to_string(),
                ResponseState::Error {
                    bytes_received: request_state.bytes_received(),
                    elapsed_duration: started_at.elapsed(),
                },
            );
        };

        let request = match Builder::new()
            .method(self.method.clone())
            .uri(normalized_url)
            .follow_redirects(RedirectPolicy::FollowAll)
            .body(AsyncBody::empty())
        {
            Ok(request) => request,
            Err(error) => {
                return (
                    format!("Error: {error}"),
                    ResponseState::Error {
                        bytes_received: request_state.bytes_received(),
                        elapsed_duration: started_at.elapsed(),
                    },
                );
            }
        };

        match http_client.send(request).await {
            Ok(mut response) => {
                let status_code = response.status();
                let mut bytes = Vec::new();
                let mut buffer = [0; 8192];
                let mut read_error = None;

                loop {
                    match response.body_mut().read(&mut buffer).await {
                        Ok(0) => break,
                        Ok(bytes_received) => {
                            request_state.append_bytes_received(bytes_received);
                            bytes.extend_from_slice(&buffer[..bytes_received]);
                        }
                        Err(error) => {
                            read_error = Some(error);
                            break;
                        }
                    }
                }

                let payload = match read_error {
                    Some(error) => format!("(failed to read response body: {error})"),
                    None => String::from_utf8_lossy(&bytes).into_owned(),
                };
                let response_state = ResponseState::Completed {
                    status_code,
                    bytes_received: request_state.bytes_received(),
                    elapsed_duration: started_at.elapsed(),
                };
                (payload, response_state)
            }
            Err(error) => (
                format!("Error: {error}"),
                ResponseState::Error {
                    bytes_received: request_state.bytes_received(),
                    elapsed_duration: started_at.elapsed(),
                },
            ),
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
        let request_state = Arc::new(RequestState::new());

        window
            .spawn(cx, {
                let request_state = request_state.clone();
                let response_panel = response_panel.clone();
                async move |cx| {
                    loop {
                        cx.background_executor()
                            .timer(Duration::from_millis(50))
                            .await;
                        if request_state.completed() {
                            break;
                        }

                        let response_state = ResponseState::Fetching {
                            bytes_received: request_state.bytes_received(),
                            elapsed_duration: request_started_at.elapsed(),
                        };
                        let still_active = response_panel.update(cx, |response_panel, cx| {
                            response_panel.set_state(request_id, response_state, cx)
                        });
                        if !still_active {
                            break;
                        }
                    }
                }
            })
            .detach();

        window
            .spawn(cx, {
                let request = self.request.clone();
                let response_panel = response_panel.clone();
                async move |cx| {
                    let (payload, response_state) = request
                        .send(http_client, &request_state, request_started_at)
                        .await;
                    request_state.set_completed();

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
