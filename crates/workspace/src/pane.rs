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

use http_client::{
    AsyncBody, Builder, HttpClient, HttpRequestExt, Method, RedirectPolicy, Request,
};
use input::InputField;
use reqwest_client::ReqwestClient;
use theme::ActiveTheme;
use ui::{
    Button, ButtonCommon, ButtonSize, ButtonVariant, Clickable, ContextMenu, DropdownMenu,
    DropdownStyle, FixedWidth, IconPosition, Label,
};

use crate::{SendRequest, Workspace, panel::response::ResponseStatus};

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

    fn push_bytes(&self, bytes: usize) {
        self.bytes_received.fetch_add(bytes, Ordering::Relaxed);
    }
}

pub struct Pane {
    focus_handle: FocusHandle,
    input: Option<Entity<InputField>>,
    request_method: Method,
    http_client: Arc<dyn HttpClient>,
    workspace: WeakEntity<Workspace>,
}

impl Pane {
    pub fn new(workspace: WeakEntity<Workspace>, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            input: None,
            request_method: Method::GET,
            http_client: Arc::new(ReqwestClient::new()),
            workspace,
        }
    }

    fn normalize_url(url: String) -> Option<String> {
        let url = url.trim().to_string();
        if url.is_empty() {
            return None;
        }
        if url.starts_with("http://") || url.starts_with("https://") {
            Some(url)
        } else {
            Some(format!("https://{url}"))
        }
    }

    async fn fetch(
        http_client: Arc<dyn HttpClient>,
        request: Request<AsyncBody>,
        state: &RequestState,
        started_at: Instant,
    ) -> (String, ResponseStatus) {
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
                            state.push_bytes(bytes_received);
                            bytes.extend_from_slice(&buffer[..bytes_received]);
                        }
                        Err(error) => {
                            read_error = Some(error);
                            break;
                        }
                    }
                }

                let payload = match read_error {
                    Some(e) => format!("(failed to read response body: {e})"),
                    None => String::from_utf8_lossy(&bytes).into_owned(),
                };
                let status = ResponseStatus::Completed {
                    status_code,
                    bytes_received: state.bytes_received(),
                    elapsed_duration: started_at.elapsed(),
                };
                (payload, status)
            }
            Err(error) => (
                format!("Error: {error}"),
                ResponseStatus::Error {
                    bytes_received: state.bytes_received(),
                    elapsed_duration: started_at.elapsed(),
                },
            ),
        }
    }

    fn send_request(
        &mut self,
        method: Method,
        url: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(url) = Self::normalize_url(url) else {
            return;
        };

        let Ok(response_panel) = self
            .workspace
            .update(cx, |workspace, cx| workspace.open_response_panel(cx))
        else {
            return;
        };

        let request_id = response_panel.update(cx, |panel, cx| {
            let request_id = panel.begin_response(window, cx);
            panel.set_status(
                request_id,
                ResponseStatus::Fetching {
                    bytes_received: 0,
                    elapsed_duration: Duration::default(),
                },
                cx,
            );
            request_id
        });

        let request_started_at = Instant::now();
        let request = match Builder::new()
            .method(method)
            .uri(url.as_str())
            .follow_redirects(RedirectPolicy::FollowAll)
            .body(AsyncBody::empty())
        {
            Ok(request) => request,
            Err(error) => {
                response_panel.update(cx, |panel, cx| {
                    panel.set_status(
                        request_id,
                        ResponseStatus::Error {
                            bytes_received: 0,
                            elapsed_duration: request_started_at.elapsed(),
                        },
                        cx,
                    );
                    panel.set_payload(request_id, format!("Error: {error}"), cx);
                });
                cx.notify();
                return;
            }
        };

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

                        let status = ResponseStatus::Fetching {
                            bytes_received: request_state.bytes_received(),
                            elapsed_duration: request_started_at.elapsed(),
                        };
                        let still_active = response_panel
                            .update(cx, |panel, cx| panel.set_status(request_id, status, cx));
                        if !still_active {
                            break;
                        }
                    }
                }
            })
            .detach();

        window
            .spawn(cx, {
                let response_panel = response_panel.clone();
                async move |cx| {
                    let (payload, status) =
                        Self::fetch(http_client, request, &request_state, request_started_at).await;
                    request_state.set_completed();

                    if let Err(error) = response_panel.update_in(cx, |panel, _, cx| {
                        panel.set_status(request_id, status, cx);
                        panel.set_payload(request_id, payload, cx);
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
        if self.input.is_none() {
            let input = cx.new(|cx| InputField::new(window, cx, "https://example.com"));
            self.input = Some(input);
        }

        let input = self
            .input
            .clone()
            .expect("InputField should be initialized");
        let input_handle = input.clone();
        let input_handle_for_action = input.clone();
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
            let selected_request_method = self.request_method.clone();
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
                                pane.request_method = request_method_for_handler.clone();
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
                        let request_method = pane.request_method.clone();
                        let request_url = input_handle_for_action.read(cx).text(cx);
                        pane.send_request(request_method, request_url, window, cx);
                    }))
                    .child(
                        DropdownMenu::new(
                            "request-method",
                            self.request_method.as_str().to_owned(),
                            request_method_menu,
                        )
                        .style(DropdownStyle::Outlined)
                        .attach(Corner::BottomLeft)
                        .offset(gpui::point(gpui::px(0.0), gpui::px(0.5)))
                        .trigger_size(ButtonSize::Large),
                    )
                    .child(gpui::div().flex_1().child(input))
                    .child(
                        Button::new("request-send", "Send")
                            .variant(ButtonVariant::Accent)
                            .size(ButtonSize::Large)
                            .width(ui::rems_from_px(60.0))
                            .font_weight(FontWeight::MEDIUM)
                            .on_click(cx.listener(move |pane, _, window, cx| {
                                let request_method = pane.request_method.clone();
                                let request_url = input_handle.read(cx).text(cx);
                                pane.send_request(request_method, request_url, window, cx);
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
