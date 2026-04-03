use futures::{FutureExt, io::AsyncReadExt};
use gpui::{
    App, Context, Corner, Entity, FocusHandle, FocusOutEvent, Focusable, FontWeight, Subscription,
    WeakEntity, Window, prelude::*,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use http_client::{AsyncBody, Builder, HttpClient, HttpRequestExt, Method, RedirectPolicy, Url};
use input::InputField;
use reqwest_client::ReqwestClient;
use theme::ActiveTheme;
use ui::{
    Button, ButtonCommon, ButtonSize, ButtonVariant, Clickable, Color, ContextMenu, DropdownMenu,
    DropdownStyle, FixedWidth, IconButton, IconButtonShape, IconName, IconPosition, IconSize,
    Label, Tooltip,
};

use crate::{SendRequest, Workspace, panel::response::ResponseState, welcome::WelcomePage};

fn normalize_url(url: String) -> Option<Url> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }

    let url = if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("http://{url}")
    };

    Url::parse(&url).ok()
}

struct RequestConfig {
    method: Method,
    url: Entity<InputField>,
    params: Vec<RequestParam>,
}

impl RequestConfig {
    fn new(window: &mut Window, cx: &mut App) -> Self {
        Self {
            method: Method::GET,
            url: cx.new(|cx| InputField::new(window, cx, "https://example.com")),
            params: Vec::new(),
        }
    }

    fn add_param(&mut self, window: &mut Window, cx: &mut App) {
        self.params.push(RequestParam::new(window, cx));
    }

    fn delete_param(&mut self, index: usize) {
        if index < self.params.len() {
            self.params.remove(index);
        }
    }
}

struct RequestParam {
    name: Entity<InputField>,
    value: Entity<InputField>,
}

impl RequestParam {
    fn new(window: &mut Window, cx: &mut App) -> Self {
        Self {
            name: cx.new(|cx| InputField::new(window, cx, "Key")),
            value: cx.new(|cx| InputField::new(window, cx, "Value")),
        }
    }
}

pub struct Pane {
    focus_handle: FocusHandle,
    was_focused: bool,
    should_display_welcome_page: bool,
    welcome_page: Option<Entity<WelcomePage>>,
    workspace: WeakEntity<Workspace>,
    http_client: Arc<dyn HttpClient>,
    request: RequestConfig,
    _subscriptions: Vec<Subscription>,
}

impl Pane {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let subscriptions = vec![
            cx.on_focus(&focus_handle, window, Pane::focus_in),
            cx.on_focus_in(&focus_handle, window, Pane::focus_in),
            cx.on_focus_out(&focus_handle, window, Pane::focus_out),
        ];

        Self {
            focus_handle,
            was_focused: false,
            should_display_welcome_page: false,
            welcome_page: None,
            workspace,
            http_client: Arc::new(ReqwestClient::new()),
            request: RequestConfig::new(window, cx),
            _subscriptions: subscriptions,
        }
    }

    pub fn set_should_display_welcome_page(
        &mut self,
        should_display_welcome_page: bool,
        cx: &mut Context<Self>,
    ) {
        if should_display_welcome_page && !self.should_display_welcome_page {
            self.welcome_page = None;
        }
        self.should_display_welcome_page = should_display_welcome_page;
        cx.notify();
    }

    pub fn should_display_welcome_page(&self) -> bool {
        self.should_display_welcome_page
    }

    fn focus_in(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.was_focused {
            self.was_focused = true;
            cx.notify();
        }

        if self.focus_handle.is_focused(window) {
            cx.on_next_frame(window, |_, _, cx| {
                cx.notify();
            });
        }

        if self.should_display_welcome_page()
            && let Some(welcome_page) = self.welcome_page.as_ref()
            && self.focus_handle.is_focused(window)
        {
            welcome_page.read(cx).focus_handle(cx).focus(window, cx);
        }
    }

    fn focus_out(&mut self, _event: FocusOutEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.was_focused = false;
        cx.notify();
    }

    pub fn send_request(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let request_method = self.request.method.clone();
        let request_url = self.request.url.read(cx).text(cx);
        let request_params = self
            .request
            .params
            .iter()
            .filter_map(|request_param| {
                let name = request_param.name.read(cx).text(cx).trim().to_string();
                if name.is_empty() {
                    return None;
                }

                let value = request_param.value.read(cx).text(cx);
                Some((name, value))
            })
            .collect::<Vec<_>>();

        let Ok(response_panel) = self.workspace.update(cx, |workspace, cx| {
            workspace.open_response_panel(window, cx)
        }) else {
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
                let response_panel = response_panel.clone();
                async move |cx| {
                    let mut request_url = match normalize_url(request_url) {
                        Some(request_url) => request_url,
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
                                log::debug!("Failed to update response panel: {error:?}");
                            }
                            return;
                        }
                    };

                    {
                        let mut query_pairs = request_url.query_pairs_mut();
                        for (name, value) in request_params {
                            query_pairs.append_pair(&name, &value);
                        }
                    }

                    let request = match Builder::new()
                        .method(request_method)
                        .uri(request_url.as_str())
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
                                log::debug!("Failed to update response panel: {error:?}");
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
                                            log::debug!("Failed to update response panel: {error:?}");
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

                    let elapsed_duration = request_started_at.elapsed();
                    let (payload, response_state) = match read_error {
                        Some(ref error) => (
                            format!("(failed to read response body: {error})"),
                            ResponseState::Error {
                                bytes_received,
                                elapsed_duration,
                            },
                        ),
                        None => (
                            String::from_utf8_lossy(&payload).into_owned(),
                            ResponseState::Completed {
                                status_code,
                                bytes_received,
                                elapsed_duration,
                            },
                        ),
                    };

                    if let Err(error) = response_panel.update_in(cx, |response_panel, _, cx| {
                        response_panel.set_state(request_id, response_state, cx);
                        response_panel.set_payload(request_id, payload, cx);
                    }) {
                        log::debug!("Failed to update response panel: {error:?}");
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
        if self.should_display_welcome_page() {
            if self.welcome_page.is_none() {
                let workspace = self.workspace.clone();
                self.welcome_page = Some(cx.new(|cx| WelcomePage::new(workspace, window, cx)));
            }

            return gpui::div()
                .track_focus(&self.focus_handle)
                .size_full()
                .overflow_hidden()
                .bg(cx.theme().colors().panel_background)
                .child(
                    ui::h_flex()
                        .size_full()
                        .justify_center()
                        .when_some(self.welcome_page.clone(), |container, welcome_page| {
                            container.child(welcome_page)
                        }),
                );
        }

        let url = self.request.url.clone();
        let request_params = self
            .request
            .params
            .iter()
            .enumerate()
            .map(|(index, request_param)| {
                let name = request_param.name.clone();
                let value = request_param.value.clone();

                ui::h_flex()
                    .id(("request-param-row", index))
                    .w_full()
                    .gap_2()
                    .child(gpui::div().flex_1().child(name))
                    .child(gpui::div().flex_1().child(value))
                    .child(
                        IconButton::new(("request-param-delete", index), IconName::Trash)
                            .shape(IconButtonShape::Square)
                            .variant(ButtonVariant::Outline)
                            .icon_color(Color::Muted)
                            .tooltip(Tooltip::text("Delete"))
                            .on_click(cx.listener(move |pane, _, _, cx| {
                                pane.request.delete_param(index);
                                cx.notify();
                            })),
                    )
            })
            .collect::<Vec<_>>();
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
                                log::debug!("Failed to update request method: {error:?}");
                            }
                        },
                    );
                }
                menu
            })
        };

        let theme_colors = cx.theme().colors();

        ui::v_flex()
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(theme_colors.panel_background)
            .p_3()
            .child(Label::new("HTTP Request"))
            .child(
                ui::h_flex()
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
                    .child(gpui::div().flex_1().child(url))
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
                ui::v_flex()
                    .w_full()
                    .gap_2()
                    .pb_2()
                    .child(ui::h_flex().w_full().child(Label::new("Query Parameters")))
                    .children(request_params)
                    .child(
                        ui::h_flex().child(
                            Button::new("request-param-add", "Add Parameter")
                                .icon(IconName::Plus)
                                .icon_size(IconSize::Small)
                                .icon_color(Color::Muted)
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Medium)
                                .on_click(cx.listener(move |pane, _, window, cx| {
                                    pane.request.add_param(window, cx);
                                    cx.notify();
                                })),
                        ),
                    ),
            )
            .child(
                ui::v_flex()
                    .flex_1()
                    .overflow_hidden()
                    .child(gpui::div().flex_1()),
            )
    }
}
