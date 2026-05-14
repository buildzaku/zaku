use futures::{FutureExt, io::AsyncReadExt};
use gpui::{
    Anchor, AnyElement, App, Context, Div, Entity, EntityId, EventEmitter, FocusHandle, Focusable,
    FontWeight, ScrollHandle, SharedString, Subscription, Task, WeakEntity, Window, prelude::*,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use actions::workspace::SendRequest;
use http_client::{AsyncBody, Builder, HttpClient, HttpRequestExt, Method, RedirectPolicy, Url};
use input::{ErasedEditorEvent, InputField};
use project::{
    Project, ProjectEntryId, ProjectPath, RequestFile, RequestFileConfig, RequestFileMeta,
    RequestFileParam, RequestFileState,
};
use response_panel::{Response, ResponsePanel, ResponseState};
use theme::ActiveTheme;
use ui::{
    Button, ButtonCommon, ButtonSize, ButtonVariant, Clickable, Color, ContextMenu, DropdownMenu,
    DropdownStyle, FixedWidth, Icon, IconButton, IconButtonShape, IconName, IconPosition, IconSize,
    Label, LabelCommon, LabelSize, ScrollAxes, Scrollbars, ToggleState, Tooltip, TrackLayout,
    WithScrollbar,
};
use util::{path::PathStyle, truncate_and_trailoff};

use workspace::{
    Item, ItemBufferKind, ItemEvent, ProjectItem, SharedState, TabContentParams, Workspace,
    pane::Pane,
};

const MAX_TAB_TITLE_LEN: usize = 24;

pub fn init(cx: &mut App) {
    workspace::register_project_item::<RequestEditor>(cx);

    cx.observe_new(
        |workspace: &mut Workspace, _: Option<&mut Window>, cx: &mut Context<Workspace>| {
            let pane = workspace.pane().clone();
            cx.observe(&pane, |workspace, _, cx| {
                update_response_panel(workspace, cx);
            })
            .detach();

            workspace.register_action(|workspace, _: &SendRequest, window, cx| {
                workspace.pane().update(cx, |pane, cx| {
                    pane.send_request(window, cx);
                });
            });
        },
    )
    .detach();
}

fn update_response_panel(workspace: &mut Workspace, cx: &mut Context<Workspace>) {
    let response = workspace
        .active_item_as::<RequestEditor>(cx)
        .and_then(|request_editor| request_editor.read(cx).response());

    if let Some(response_panel) = workspace.panel::<ResponsePanel>(cx) {
        response_panel.update(cx, |response_panel, cx| {
            response_panel.set_response(response, cx);
        });
    }
}

pub trait RequestPaneExt: Sized {
    fn send_request(&mut self, window: &mut Window, cx: &mut Context<Self>);
}

impl RequestPaneExt for Pane {
    fn send_request(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(active_item) = self.active_item()
            && let Some(request_editor) = active_item.downcast::<RequestEditor>()
        {
            let item_id = active_item.item_id();
            self.unpreview_item_if_preview(item_id);
            cx.notify();
            request_editor.update(cx, |request_editor, cx| {
                request_editor.send_request(window, cx);
            });
        }
    }
}

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

fn request_method_label(method: &Method) -> String {
    let method = method.as_str().trim().to_ascii_uppercase();
    match method.as_str() {
        "GET" => "GET".to_string(),
        "POST" => "POST".to_string(),
        "PUT" => "PUT".to_string(),
        "PATCH" => "PATCH".to_string(),
        "DELETE" => "DEL".to_string(),
        "HEAD" => "HEAD".to_string(),
        "OPTIONS" => "OPT".to_string(),
        _ => method.chars().take(5).collect(),
    }
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
    Ready(Request),
    Invalid {
        error: String,
        snapshot: Option<RequestSnapshot>,
    },
}

struct Request {
    meta: RequestMeta,
    config: RequestConfig,
}

type RequestMeta = RequestFileMeta;

struct RequestConfig {
    method: Method,
    url: Entity<InputField>,
    params: Vec<RequestParam>,
}

impl Request {
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
        let mut params = Vec::new();
        for request_file_param in &request_file.config.params {
            let mut request_param = RequestParam::new(window, cx);
            request_param.name.update(cx, |name, cx| {
                name.set_text(&request_file_param.name, window, cx);
            });
            request_param.value.update(cx, |value, cx| {
                value.set_text(&request_file_param.value, window, cx);
            });
            if request_file_param.disabled {
                request_param.set_disabled(true, window, cx);
            }
            params.push(request_param);
        }

        Ok(Self {
            meta: request_file.meta.clone(),
            config: RequestConfig {
                method,
                url,
                params,
            },
        })
    }

    fn delete_param(&mut self, index: usize) -> bool {
        if index < self.config.params.len() {
            self.config.params.remove(index);
            true
        } else {
            false
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct RequestSnapshot(RequestFile);

impl RequestSnapshot {
    fn from_request(request: &Request, cx: &App) -> Self {
        Self(RequestFile {
            meta: request.meta.clone(),
            config: RequestFileConfig {
                method: request.config.method.as_str().to_owned(),
                url: request.config.url.read(cx).text(cx),
                params: request
                    .config
                    .params
                    .iter()
                    .map(|request_param| RequestFileParam {
                        name: request_param.name.read(cx).text(cx),
                        value: request_param.value.read(cx).text(cx),
                        disabled: request_param.disabled,
                    })
                    .collect(),
            },
        })
    }

    fn from_request_file(request_file: &RequestFile) -> Self {
        Self(request_file.clone())
    }

    fn name(&self) -> Option<&str> {
        self.0.meta.name.as_deref().and_then(|name| {
            let name = name.trim();
            if name.is_empty() { None } else { Some(name) }
        })
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
    request_snapshot: Option<RequestSnapshot>,
    response: Option<Entity<Response>>,
    http_client: Arc<dyn HttpClient>,
    scroll_handle: ScrollHandle,
    is_dirty: bool,
    input_subscriptions: Vec<Subscription>,
}

impl RequestEditor {
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
                match Request::from_request_file(&request_file, window, cx) {
                    Ok(request) => RequestEditorState::Ready(request),
                    Err(error) => RequestEditorState::Invalid {
                        error,
                        snapshot: Some(RequestSnapshot::from_request_file(&request_file)),
                    },
                }
            }
            RequestFileState::Invalid(error) => RequestEditorState::Invalid {
                error,
                snapshot: None,
            },
        };
        let request_snapshot = match &request {
            RequestEditorState::Ready(request) => Some(RequestSnapshot::from_request(request, cx)),
            RequestEditorState::Invalid { snapshot, .. } => snapshot.clone(),
        };

        let input_subscriptions = match &request {
            RequestEditorState::Ready(request) => Self::subscribe_to_request(request, window, cx),
            RequestEditorState::Invalid { .. } => Vec::new(),
        };

        Self {
            focus_handle,
            workspace,
            project: Some(project),
            buffer: Some(buffer),
            request,
            request_snapshot,
            response: None,
            http_client: SharedState::global(cx).http_client.clone(),
            scroll_handle: ScrollHandle::new(),
            is_dirty: false,
            input_subscriptions,
        }
    }

    fn project_path(&self, cx: &App) -> Option<ProjectPath> {
        self.buffer
            .as_ref()
            .and_then(|buffer| project::ProjectItem::project_path(buffer.read(cx), cx))
    }

    fn path_style(&self, cx: &App) -> PathStyle {
        self.project
            .as_ref()
            .map_or_else(PathStyle::local, |project| project.read(cx).path_style(cx))
    }

    fn response(&self) -> Option<Entity<Response>> {
        self.response.clone()
    }

    fn unpreview_tab(&self, cx: &mut Context<Self>) {
        let request_editor_id = cx.entity().entity_id();
        if let Err(error) = self.workspace.update(cx, |workspace, cx| {
            workspace.pane().update(cx, |pane, cx| {
                pane.unpreview_item_if_preview(request_editor_id);
                cx.notify();
            });
        }) {
            log::debug!("Failed to unpreview request editor: {error:?}");
        }
    }

    fn title(&self, cx: &App) -> SharedString {
        let display_name = match &self.request {
            RequestEditorState::Ready(request) => request.meta.name.as_deref().and_then(|name| {
                let name = name.trim();
                if name.is_empty() { None } else { Some(name) }
            }),
            RequestEditorState::Invalid { snapshot, .. } => {
                snapshot.as_ref().and_then(RequestSnapshot::name)
            }
        };

        if let Some(display_name) = display_name {
            return SharedString::from(display_name.to_owned());
        }

        self.project_path(cx)
            .and_then(|project_path| {
                project_path.path.file_name().map(|file_name| {
                    let file_name = file_name.strip_suffix(".toml").unwrap_or(file_name);
                    SharedString::from(file_name.to_owned())
                })
            })
            .unwrap_or_else(|| SharedString::from("HTTP Request"))
    }

    fn path_for_request(
        &self,
        height: usize,
        include_filename: bool,
        cx: &App,
    ) -> Option<SharedString> {
        let project_path = self.project_path(cx)?;
        let path_style = self.path_style(cx);
        let height = height.saturating_add(1);
        let mut components = project_path
            .path
            .components()
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();

        if components.is_empty() {
            return None;
        }

        let start = components.len().saturating_sub(height);
        let mut components = components.split_off(start);

        if include_filename {
            if let Some(file_name) = components.last_mut() {
                *file_name = self.title(cx).to_string();
            }
        } else {
            components.pop()?;
        }

        if components.is_empty() {
            return None;
        }

        Some(SharedString::from(
            components.join(path_style.primary_separator()),
        ))
    }

    fn subscribe_to_request(
        request: &Request,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Subscription> {
        let mut subscriptions = Vec::new();
        subscriptions.push(Self::subscribe_to_input(&request.config.url, window, cx));
        for request_param in &request.config.params {
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
        let is_dirty = if let (Some(request_snapshot), RequestEditorState::Ready(request)) =
            (self.request_snapshot.as_ref(), &self.request)
        {
            RequestSnapshot::from_request(request, cx) != *request_snapshot
        } else {
            false
        };
        let dirty_changed = if let Some(buffer) = self.buffer.as_ref() {
            buffer.update(cx, |buffer, cx| {
                let dirty_changed = buffer.is_dirty != is_dirty;
                if dirty_changed {
                    buffer.is_dirty = is_dirty;
                    cx.notify();
                }
                dirty_changed
            })
        } else if self.is_dirty == is_dirty {
            false
        } else {
            self.is_dirty = is_dirty;
            true
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
            request.config.params.push(request_param);
        }
        self.input_subscriptions.push(name_subscription);
        self.input_subscriptions.push(value_subscription);
        self.mark_edited(cx);
    }

    pub fn send_request(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let RequestEditorState::Ready(request) = &self.request else {
            return;
        };

        let request_method = request.config.method.clone();
        let request_url = request.config.url.read(cx).text(cx);
        let request_params = request
            .config
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

        let Ok(Some(response_panel)) = self.workspace.update(cx, |workspace, cx| {
            workspace.open_panel::<ResponsePanel>(window, cx);
            workspace.panel::<ResponsePanel>(cx)
        }) else {
            return;
        };
        let response = self
            .response
            .get_or_insert_with(|| cx.new(|cx| Response::new(window, cx)))
            .clone();
        response_panel.update(cx, |panel, cx| {
            panel.set_response(Some(response.clone()), cx);
        });

        let request_id = response.update(cx, |response, cx| response.begin_response(window, cx));
        response.update(cx, |response, cx| {
            response.set_state(
                request_id,
                ResponseState::Fetching {
                    bytes_received: 0,
                    elapsed_duration: Duration::default(),
                },
                cx,
            );
        });

        let request_started_at = Instant::now();
        let http_client = self.http_client.clone();

        window
            .spawn(cx, {
                async move |cx| {
                    let Some(mut request_url) = normalize_url(&request_url) else {
                        response.update(cx, |response, cx| {
                            response.set_state(
                                request_id,
                                ResponseState::Error {
                                    bytes_received: 0,
                                    elapsed_duration: request_started_at.elapsed(),
                                },
                                cx,
                            );
                            response.set_payload(request_id, "Error: invalid URL", cx);
                        });
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
                            response.update(cx, |response, cx| {
                                response.set_state(
                                    request_id,
                                    ResponseState::Error {
                                        bytes_received: 0,
                                        elapsed_duration: request_started_at.elapsed(),
                                    },
                                    cx,
                                );
                                response.set_payload(request_id, format!("Error: {error}"), cx);
                            });
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

                    let mut received = loop {
                        futures::select_biased! {
                            send_result = send_request => {
                                match send_result {
                                    Ok(response) => break response,
                                    Err(error) => {
                                        response.update(cx, |response, cx| {
                                            response.set_state(
                                                request_id,
                                                ResponseState::Error {
                                                    bytes_received: 0,
                                                    elapsed_duration: request_started_at.elapsed(),
                                                },
                                                cx,
                                            );
                                            response.set_payload(
                                                request_id,
                                                format!("Error: {error}"),
                                                cx,
                                            );
                                        });
                                        return;
                                    }
                                }
                            }
                            () = progress_timer => {
                                let still_active = response.update(cx, |response, cx| {
                                    response.set_state(
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

                    let status_code = received.status();
                    let mut bytes_received = 0_u64;
                    let mut payload = Vec::new();
                    let mut buffer = [0; 8192];
                    let mut read_error = None;

                    loop {
                        let read_response_body = received.body_mut().read(&mut buffer).fuse();
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
                                let still_active = response.update(cx, |response, cx| {
                                    response.set_state(
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

                    response.update(cx, |response, cx| {
                        response.set_state(request_id, response_state, cx);
                        response.set_payload(request_id, payload, cx);
                    });
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
        request: &Request,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let request_relative_path = self.project_path(cx).map(|project_path| {
            SharedString::from(project_path.path.display(self.path_style(cx)).into_owned())
        });

        let url = request.config.url.clone();
        let request_params = request
            .config
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
                                        && let Some(request_param) =
                                            request.config.params.get_mut(index)
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
            let selected_request_method = request.config.method.clone();
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
                                    && request.config.method != request_method_for_handler
                                {
                                    request.config.method = request_method_for_handler.clone();
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
            .when_some(request_relative_path, |this, request_relative_path| {
                this.child(
                    ui::h_flex()
                        .w_full()
                        .px_3()
                        .pt_2()
                        .child(Label::new(request_relative_path)),
                )
            })
            .child(
                ui::h_flex()
                    .w_full()
                    .px_3()
                    .py_2()
                    .gap_2()
                    .key_context("RequestUrl")
                    .on_action(
                        cx.listener(move |request_editor, _: &SendRequest, window, cx| {
                            request_editor.unpreview_tab(cx);
                            request_editor.send_request(window, cx);
                        }),
                    )
                    .child(
                        DropdownMenu::new(
                            "request-method",
                            request.config.method.as_str().to_owned(),
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
                                request_editor.unpreview_tab(cx);
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
            RequestEditorState::Invalid { error, .. } => self.render_invalid(error, cx),
        }
    }
}

impl Item for RequestEditor {
    type Event = ItemEvent;

    fn to_item_events(event: &Self::Event, f: &mut dyn FnMut(ItemEvent)) {
        f(*event);
    }

    fn tab_content_text(&self, detail: usize, cx: &App) -> SharedString {
        self.path_for_request(detail, true, cx)
            .unwrap_or_else(|| self.title(cx))
    }

    fn tab_content(&self, params: TabContentParams, _window: &Window, cx: &App) -> AnyElement {
        let method_label = match &self.request {
            RequestEditorState::Ready(request) => {
                Some(request_method_label(&request.config.method))
            }
            RequestEditorState::Invalid { .. } => None,
        };
        let title = Label::new(truncate_and_trailoff(&self.title(cx), MAX_TAB_TITLE_LEN))
            .color(params.text_color())
            .when(params.preview, |this| this.italic());
        let description = params.detail.and_then(|detail| {
            let path = self.path_for_request(detail, false, cx)?;
            let description = path.trim();

            if description.is_empty() {
                return None;
            }

            Some(truncate_and_trailoff(description, MAX_TAB_TITLE_LEN))
        });

        ui::h_flex()
            .min_w_0()
            .gap_2()
            .when(
                matches!(&self.request, RequestEditorState::Invalid { .. }),
                |this| {
                    this.child(
                        ui::h_flex().flex_none().items_center().child(
                            Icon::new(IconName::WarningCircle)
                                .size(IconSize::Small)
                                .color(Color::Error),
                        ),
                    )
                },
            )
            .when_some(method_label, |this, method| {
                this.child(
                    ui::h_flex().flex_none().items_center().child(
                        Label::new(method)
                            .size(LabelSize::Small)
                            .weight(FontWeight::MEDIUM)
                            .color(Color::Muted)
                            .alpha(0.7)
                            .single_line(),
                    ),
                )
            })
            .child(title)
            .when_some(description, |this, description| {
                this.child(
                    Label::new(description)
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                )
            })
            .into_any_element()
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

#[cfg(test)]
mod tests {
    use super::*;

    use futures::channel::oneshot;
    use gpui::{TestAppContext, VisualTestContext};
    use indoc::indoc;
    use parking_lot::Mutex;
    use serde_json::json;

    use http_client::{Response, StatusCode};
    use settings::SettingsStore;
    use theme::LoadThemes;
    use util::rel_path::rel_path;
    use util_macros::path;
    use workspace::{DockPosition, Root, SharedState};

    fn init_test(shared_state: Arc<SharedState>, cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
            theme::init(LoadThemes::JustBase, cx);
            workspace::init(shared_state, cx);
            editor::init(cx);
            crate::init(cx);
            response_panel::init(cx);
        });
    }

    fn build_workspace<'a>(
        project: &Entity<Project>,
        cx: &'a mut TestAppContext,
    ) -> (
        Entity<Workspace>,
        Entity<ResponsePanel>,
        &'a mut VisualTestContext,
    ) {
        let project = project.clone();
        let (root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(cx.new(|cx| Workspace::test_new(project, window, cx)))
        });
        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());
        let response_panel = workspace.update_in(cx, |workspace, window, cx| {
            let pane = workspace.pane().downgrade();
            let response_panel = cx.new(|cx| ResponsePanel::new(pane, window, cx));
            workspace.add_panel(response_panel.clone(), DockPosition::Bottom, window, cx);
            response_panel
        });

        (workspace, response_panel, cx)
    }

    #[gpui::test]
    async fn test_send_request_opens_response_panel(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        let http_client = shared_state.http_client.as_fake();
        let (tx, rx) = oneshot::channel();
        let rx = Arc::new(Mutex::new(Some(rx)));

        http_client.replace_handler({
            move |_, request| {
                assert_eq!(request.uri().path(), "/me");
                let rx = rx.lock().take().unwrap();

                async move {
                    rx.await
                        .map_err(|_| anyhow::anyhow!("Response sender dropped"))?
                }
            }
        });

        init_test(shared_state, cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                "collection": {
                    "request.toml": indoc! {r#"
                        [meta]
                        version = 1

                        [config]
                        method = "GET"
                        url = "https://api.zaku.dev/me"
                    "#}
                }
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let worktree_id = cx.update(|cx| project.read(cx).worktree(cx).unwrap().read(cx).id());
        let (workspace, response_panel, cx) = build_workspace(&project, cx);
        let pane = workspace.update_in(cx, |workspace, _, _| workspace.pane().clone());

        let request_path = ProjectPath {
            worktree_id,
            path: Arc::from(rel_path(path!("collection/request.toml"))),
        };

        workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.open_path(request_path, None, true, window, cx)
            })
            .await
            .unwrap()
            .downcast::<RequestEditor>()
            .unwrap();
        pane.update_in(cx, |pane, window, cx| {
            pane.send_request(window, cx);
        });
        workspace.update_in(cx, |workspace, _, cx| {
            let response_panel_id = Entity::entity_id(&response_panel);
            let active_panel_id = workspace
                .bottom_dock()
                .read(cx)
                .active_panel()
                .map(|panel| panel.panel_id());

            assert!(workspace.bottom_dock().read(cx).is_open());
            assert_eq!(active_panel_id, Some(response_panel_id));
        });
        cx.run_until_parked();

        let response = Response::builder()
            .status(StatusCode::OK)
            .body(AsyncBody::from("response"))
            .unwrap();
        assert!(tx.send(Ok(response)).is_ok());
    }
}
