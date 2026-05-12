use futures::{FutureExt, io::AsyncReadExt};
use gpui::{
    Anchor, App, Context, Div, Entity, EntityId, EventEmitter, FocusHandle, Focusable, FontWeight,
    ScrollHandle, SharedString, Subscription, Task, WeakEntity, Window, prelude::*,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use actions::workspace::SendRequest;
use http_client::{AsyncBody, Builder, HttpClient, HttpRequestExt, Method, RedirectPolicy, Url};
use input::{ErasedEditorEvent, InputField};
use project::{Project, ProjectEntryId, ProjectPath, RequestFile, RequestFileState};
use reqwest_client::ReqwestClient;
use theme::ActiveTheme;
use ui::{
    Button, ButtonCommon, ButtonSize, ButtonVariant, Clickable, Color, ContextMenu, DropdownMenu,
    DropdownStyle, FixedWidth, IconButton, IconButtonShape, IconName, IconPosition, IconSize,
    Label, LabelCommon, LabelSize, ScrollAxes, Scrollbars, ToggleState, Tooltip, TrackLayout,
    WithScrollbar,
};

use crate::{
    Item, ItemBufferKind, ItemEvent, ProjectItem, Workspace, pane::Pane,
    panel::response::ResponseState,
};

fn normalize_url(url: &str) -> Option<Url> {
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

pub struct RequestBuffer {
    entry_id: Option<ProjectEntryId>,
    project_path: Option<ProjectPath>,
    request: RequestFileState,
    is_dirty: bool,
}

impl RequestBuffer {
    fn request(&self) -> &RequestFileState {
        &self.request
    }

    fn mark_edited(&mut self) -> bool {
        let was_dirty = self.is_dirty;
        self.is_dirty = true;
        !was_dirty
    }

    fn is_dirty(&self) -> bool {
        self.is_dirty
    }
}

impl project::ProjectItem for RequestBuffer {
    fn try_open(
        project: &Entity<Project>,
        path: &ProjectPath,
        cx: &mut App,
    ) -> Option<Task<anyhow::Result<Entity<Self>>>> {
        let entry = project.read(cx).entry_for_path(path, cx)?.clone();
        if !entry.is_file() {
            return None;
        }

        let request = entry.request.clone()?;
        let entry_id = Some(entry.id);
        let project_path = Some(path.clone());
        Some(Task::ready(Ok(cx.new(|_| Self {
            entry_id,
            project_path,
            request,
            is_dirty: false,
        }))))
    }

    fn entry_id(&self, _cx: &App) -> Option<ProjectEntryId> {
        self.entry_id
    }

    fn project_path(&self, _cx: &App) -> Option<ProjectPath> {
        self.project_path.clone()
    }

    fn is_dirty(&self) -> bool {
        self.is_dirty
    }
}

enum RequestEditorState {
    Ready(RequestConfig),
    Invalid(String),
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

    fn from_request_file(
        request_file: &RequestFile,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Self, String> {
        let method =
            Method::from_bytes(request_file.config.method.as_bytes()).map_err(|error| {
                format!(
                    "Invalid request method `{}`: {error}",
                    request_file.config.method
                )
            })?;
        let url = cx.new(|cx| InputField::new(window, cx, "https://example.com"));
        url.update(cx, |url, cx| {
            url.set_text(&request_file.config.url, window, cx);
        });

        Ok(Self {
            method,
            url,
            params: Vec::new(),
        })
    }

    fn delete_param(&mut self, index: usize) -> bool {
        if index < self.params.len() {
            self.params.remove(index);
            true
        } else {
            false
        }
    }
}

struct RequestParam {
    name: Entity<InputField>,
    value: Entity<InputField>,
    disabled: bool,
}

impl RequestParam {
    fn new(window: &mut Window, cx: &mut App) -> Self {
        Self {
            name: cx.new(|cx| InputField::new(window, cx, "Key")),
            value: cx.new(|cx| InputField::new(window, cx, "Value")),
            disabled: false,
        }
    }

    fn set_disabled(&mut self, disabled: bool, window: &mut Window, cx: &mut App) {
        self.disabled = disabled;
        self.name
            .update(cx, |name, cx| name.set_muted(disabled, window, cx));
        self.value
            .update(cx, |value, cx| value.set_muted(disabled, window, cx));
    }
}

pub struct RequestEditor {
    focus_handle: FocusHandle,
    workspace: WeakEntity<Workspace>,
    project: Option<Entity<Project>>,
    buffer: Option<Entity<RequestBuffer>>,
    request: RequestEditorState,
    http_client: Arc<dyn HttpClient>,
    scroll_handle: ScrollHandle,
    is_dirty: bool,
    subscriptions: Vec<Subscription>,
}

impl RequestEditor {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let request = RequestConfig::new(window, cx);
        let subscriptions = Self::subscribe_to_request_config(&request, window, cx);

        Self {
            focus_handle,
            workspace,
            project: None,
            buffer: None,
            request: RequestEditorState::Ready(request),
            http_client: Arc::new(ReqwestClient::new()),
            scroll_handle: ScrollHandle::new(),
            is_dirty: false,
            subscriptions,
        }
    }

    fn for_buffer(
        workspace: WeakEntity<Workspace>,
        project: Entity<Project>,
        buffer: Entity<RequestBuffer>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let request = match buffer.read(cx).request().clone() {
            RequestFileState::Parsed(request_file) => {
                match RequestConfig::from_request_file(&request_file, window, cx) {
                    Ok(request) => RequestEditorState::Ready(request),
                    Err(error) => RequestEditorState::Invalid(error),
                }
            }
            RequestFileState::Invalid(error) => RequestEditorState::Invalid(error),
        };

        let subscriptions = match &request {
            RequestEditorState::Ready(request) => {
                Self::subscribe_to_request_config(request, window, cx)
            }
            RequestEditorState::Invalid(_) => Vec::new(),
        };

        Self {
            focus_handle,
            workspace,
            project: Some(project),
            buffer: Some(buffer),
            request,
            http_client: Arc::new(ReqwestClient::new()),
            scroll_handle: ScrollHandle::new(),
            is_dirty: false,
            subscriptions,
        }
    }

    fn subscribe_to_request_config(
        request: &RequestConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Subscription> {
        let mut subscriptions = Vec::new();
        subscriptions.push(Self::subscribe_to_input(&request.url, window, cx));
        for request_param in &request.params {
            subscriptions.push(Self::subscribe_to_input(&request_param.name, window, cx));
            subscriptions.push(Self::subscribe_to_input(&request_param.value, window, cx));
        }
        subscriptions
    }

    fn subscribe_to_input(
        input: &Entity<InputField>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Subscription {
        let request_editor = cx.weak_entity();
        let editor = input.read(cx).editor().clone();
        editor.subscribe(
            Box::new(move |event, _window, cx| {
                if event == ErasedEditorEvent::BufferEdited
                    && let Err(error) = request_editor.update(cx, |request_editor, cx| {
                        request_editor.mark_edited(cx);
                    })
                {
                    log::debug!("Failed to update request editor edit state: {error:?}");
                }
            }),
            window,
            cx,
        )
    }

    fn mark_edited(&mut self, cx: &mut Context<Self>) {
        let dirty_changed = if let Some(buffer) = self.buffer.as_ref() {
            buffer.update(cx, |buffer, _| buffer.mark_edited())
        } else {
            let was_dirty = self.is_dirty;
            self.is_dirty = true;
            !was_dirty
        };

        cx.emit(ItemEvent::Edit);
        if dirty_changed {
            cx.emit(ItemEvent::UpdateTab);
        }
        cx.notify();
    }

    fn add_param(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(&self.request, RequestEditorState::Ready(_)) {
            return;
        }

        let request_param = RequestParam::new(window, cx);
        let name_subscription = Self::subscribe_to_input(&request_param.name, window, cx);
        let value_subscription = Self::subscribe_to_input(&request_param.value, window, cx);
        if let RequestEditorState::Ready(request) = &mut self.request {
            request.params.push(request_param);
        }
        self.subscriptions.push(name_subscription);
        self.subscriptions.push(value_subscription);
        self.mark_edited(cx);
    }

    pub fn send_request(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let RequestEditorState::Ready(request) = &self.request else {
            return;
        };

        let request_method = request.method.clone();
        let request_url = request.url.read(cx).text(cx);
        let request_params = request
            .params
            .iter()
            .filter_map(|request_param| {
                if request_param.disabled {
                    return None;
                }

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
                    let Some(mut request_url) = normalize_url(&request_url) else {
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
                            if let Err(error) =
                                response_panel.update_in(cx, |response_panel, _, cx| {
                                    response_panel.set_state(
                                        request_id,
                                        ResponseState::Error {
                                            bytes_received: 0,
                                            elapsed_duration: request_started_at.elapsed(),
                                        },
                                        cx,
                                    );
                                    response_panel.set_payload(
                                        request_id,
                                        format!("Error: {error}"),
                                        cx,
                                    );
                                })
                            {
                                log::debug!("Failed to update response panel: {error:?}");
                            }
                            return;
                        }
                    };

                    let progress_timer = cx
                        .background_executor()
                        .timer(Duration::from_millis(50))
                        .fuse();
                    futures::pin_mut!(progress_timer);

                    let send_request = http_client.send(request).fuse();
                    futures::pin_mut!(send_request);

                    let mut response = loop {
                        futures::select_biased! {
                            response = send_request => {
                                match response {
                                    Ok(response) => break response,
                                    Err(error) => {
                                        if let Err(error) =
                                            response_panel.update_in(cx, |response_panel, _, cx| {
                                                response_panel.set_state(
                                                    request_id,
                                                    ResponseState::Error {
                                                        bytes_received: 0,
                                                        elapsed_duration: request_started_at
                                                            .elapsed(),
                                                    },
                                                    cx,
                                                );
                                                response_panel.set_payload(
                                                    request_id,
                                                    format!("Error: {error}"),
                                                    cx,
                                                );
                                            })
                                        {
                                            log::debug!(
                                                "Failed to update response panel: {error:?}"
                                            );
                                        }
                                        return;
                                    }
                                }
                            }
                            () = progress_timer => {
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
                                    cx.background_executor()
                                        .timer(Duration::from_millis(50))
                                        .fuse(),
                                );
                            }
                        }
                    };

                    let status_code = response.status();
                    let mut bytes_received = 0_u64;
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
                                        if let Ok(chunk_len) = u64::try_from(chunk) {
                                            bytes_received =
                                                bytes_received.saturating_add(chunk_len);
                                        } else {
                                            bytes_received = u64::MAX;
                                        }
                                        payload.extend_from_slice(&buffer[..chunk]);
                                    }
                                    Err(error) => {
                                        read_error = Some(error);
                                        break;
                                    }
                                }
                            }
                            () = progress_timer => {
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
                                    cx.background_executor()
                                        .timer(Duration::from_millis(50))
                                        .fuse(),
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

    fn render_invalid(&self, error: &str, cx: &mut Context<Self>) -> Div {
        ui::v_flex()
            .track_focus(&self.focus_handle)
            .size_full()
            .gap_2()
            .p_3()
            .bg(cx.theme().colors().panel_background)
            .child(
                Label::new("Invalid Request")
                    .size(LabelSize::Large)
                    .color(Color::Error),
            )
            .child(Label::new(error.to_string()).color(Color::Muted))
    }

    fn render_request(
        &self,
        request: &RequestConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let url = request.url.clone();
        let request_params = request
            .params
            .iter()
            .enumerate()
            .map(|(index, request_param)| {
                let name = request_param.name.clone();
                let value = request_param.value.clone();

                ui::h_flex()
                    .id(("request-param-row", index))
                    .w_full()
                    .child(
                        gpui::div().pr_1p5().child(
                            ui::checkbox(
                                ("request-param-disabled", index),
                                ToggleState::from(!request_param.disabled),
                            )
                            .on_click(cx.listener(
                                move |request_editor, new_state: &ToggleState, window, cx| {
                                    let mut edited = false;
                                    if let RequestEditorState::Ready(request) =
                                        &mut request_editor.request
                                        && let Some(request_param) = request.params.get_mut(index)
                                    {
                                        let disabled = !new_state.selected();
                                        if request_param.disabled != disabled {
                                            request_param.set_disabled(disabled, window, cx);
                                            edited = true;
                                        }
                                    }

                                    if edited {
                                        request_editor.mark_edited(cx);
                                    }
                                },
                            )),
                        ),
                    )
                    .child(
                        ui::h_flex()
                            .flex_1()
                            .gap_2p5()
                            .child(gpui::div().flex_1().child(name))
                            .child(gpui::div().flex_1().child(value))
                            .child(
                                IconButton::new(("request-param-delete", index), IconName::Trash)
                                    .shape(IconButtonShape::Square)
                                    .variant(ButtonVariant::Outline)
                                    .icon_color(Color::Muted)
                                    .tooltip(Tooltip::text("Delete"))
                                    .on_click(cx.listener(move |request_editor, _, _, cx| {
                                        let mut edited = false;
                                        if let RequestEditorState::Ready(request) =
                                            &mut request_editor.request
                                        {
                                            edited = request.delete_param(index);
                                        }

                                        if edited {
                                            request_editor.mark_edited(cx);
                                        }
                                    })),
                            ),
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
            let selected_request_method = request.method.clone();
            let request_editor = cx.weak_entity();

            ContextMenu::build(window, cx, move |menu, _, _| {
                let mut menu = menu;
                for request_method in available_request_methods {
                    let toggled = request_method == selected_request_method;
                    let request_editor = request_editor.clone();
                    let request_method_for_handler = request_method.clone();
                    menu = menu.toggleable_entry(
                        request_method.as_str().to_owned(),
                        toggled,
                        IconPosition::End,
                        None,
                        move |_, cx| {
                            if let Err(error) = request_editor.update(cx, |request_editor, cx| {
                                let mut edited = false;
                                if let RequestEditorState::Ready(request) =
                                    &mut request_editor.request
                                    && request.method != request_method_for_handler
                                {
                                    request.method = request_method_for_handler.clone();
                                    edited = true;
                                }

                                if edited {
                                    request_editor.mark_edited(cx);
                                }
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
            .child(
                ui::h_flex()
                    .w_full()
                    .px_3()
                    .pt_3()
                    .child(Label::new("HTTP Request")),
            )
            .child(
                ui::h_flex()
                    .w_full()
                    .px_3()
                    .py_2()
                    .gap_2()
                    .key_context("RequestUrl")
                    .on_action(
                        cx.listener(move |request_editor, _: &SendRequest, window, cx| {
                            request_editor.send_request(window, cx);
                        }),
                    )
                    .child(
                        DropdownMenu::new(
                            "request-method",
                            request.method.as_str().to_owned(),
                            request_method_menu,
                        )
                        .style(DropdownStyle::Outlined)
                        .attach(Anchor::BottomLeft)
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
                            .on_click(cx.listener(move |request_editor, _, window, cx| {
                                request_editor.send_request(window, cx);
                            })),
                    ),
            )
            .child(
                ui::v_flex()
                    .id("request-params")
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .child(
                        gpui::div()
                            .id("request-params-scroll")
                            .w_full()
                            .flex_1()
                            .min_h_0()
                            .child(
                                ui::v_flex()
                                    .id("request-params-content")
                                    .track_scroll(&self.scroll_handle)
                                    .size_full()
                                    .min_w_0()
                                    .overflow_y_scroll()
                                    .child(
                                        ui::v_flex()
                                            .w_full()
                                            .min_w_0()
                                            .pl_2()
                                            .pr(gpui::px(10.0))
                                            .gap_2()
                                            .pb_3()
                                            .child(
                                                ui::h_flex()
                                                    .w_full()
                                                    .pl_1()
                                                    .child(Label::new("Query Parameters")),
                                            )
                                            .children(request_params)
                                            .child(
                                                ui::h_flex().pl_1().child(
                                                    Button::new(
                                                        "request-param-add",
                                                        "Add Parameter",
                                                    )
                                                    .icon(IconName::Plus)
                                                    .icon_size(IconSize::Small)
                                                    .icon_color(Color::Muted)
                                                    .variant(ButtonVariant::Outline)
                                                    .size(ButtonSize::Medium)
                                                    .on_click(cx.listener(
                                                        move |request_editor, _, window, cx| {
                                                            request_editor.add_param(window, cx);
                                                        },
                                                    )),
                                                ),
                                            ),
                                    ),
                            )
                            .custom_scrollbars(
                                Scrollbars::new(ScrollAxes::Vertical)
                                    .tracked_scroll_handle(&self.scroll_handle)
                                    .with_track_along(
                                        ScrollAxes::Vertical,
                                        theme_colors.scrollbar_track_background,
                                        TrackLayout::Overlay,
                                    ),
                                window,
                                cx,
                            ),
                    ),
            )
    }
}

impl EventEmitter<ItemEvent> for RequestEditor {}

impl Focusable for RequestEditor {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for RequestEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match &self.request {
            RequestEditorState::Ready(request) => self.render_request(request, window, cx),
            RequestEditorState::Invalid(error) => self.render_invalid(error, cx),
        }
    }
}

impl Item for RequestEditor {
    type Event = ItemEvent;

    fn to_item_events(event: &Self::Event, f: &mut dyn FnMut(ItemEvent)) {
        f(*event);
    }

    fn tab_content_text(&self, _detail: usize, cx: &App) -> SharedString {
        self.buffer
            .as_ref()
            .and_then(|buffer| project::ProjectItem::project_path(buffer.read(cx), cx))
            .and_then(|project_path| {
                project_path
                    .path
                    .file_name()
                    .map(|file_name| file_name.strip_suffix(".toml").unwrap_or(file_name))
                    .map(SharedString::from)
            })
            .unwrap_or_else(|| SharedString::from("HTTP Request"))
    }

    fn tab_tooltip_text(&self, cx: &App) -> Option<SharedString> {
        let project = self.project.as_ref()?;
        let project_path = self
            .buffer
            .as_ref()
            .and_then(|buffer| project::ProjectItem::project_path(buffer.read(cx), cx))?;
        project
            .read(cx)
            .absolute_path(&project_path, cx)
            .map(|path| path.to_string_lossy().into_owned().into())
    }

    fn for_each_project_item(
        &self,
        cx: &App,
        f: &mut dyn FnMut(EntityId, &dyn project::ProjectItem),
    ) {
        if let Some(buffer) = self.buffer.as_ref() {
            f(Entity::entity_id(buffer), buffer.read(cx));
        }
    }

    fn buffer_kind(&self, _cx: &App) -> ItemBufferKind {
        ItemBufferKind::Singleton
    }

    fn is_dirty(&self, cx: &App) -> bool {
        self.buffer
            .as_ref()
            .map_or(self.is_dirty, |buffer| buffer.read(cx).is_dirty())
    }
}

impl ProjectItem for RequestEditor {
    type Item = RequestBuffer;

    fn for_project_item(
        project: Entity<Project>,
        pane: Option<&Pane>,
        item: Entity<Self::Item>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self
    where
        Self: Sized,
    {
        let workspace = pane.map_or_else(WeakEntity::new_invalid, Pane::workspace);
        Self::for_buffer(workspace, project, item, window, cx)
    }
}
