mod items;
mod persistence;

use futures::{FutureExt, io::AsyncReadExt};
use gpui::{
    Anchor, AnyElement, App, Context, Div, ElementId, Entity, EventEmitter, FocusHandle, Focusable,
    FontWeight, ScrollHandle, SharedString, Subscription, TextStyleRefinement, WeakEntity, Window,
    prelude::*,
};
use std::{
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use editor::{Editor, EditorEvent};
use http_client::{
    AsyncBody, Builder, HttpClient, HttpRequestExt, Method, RedirectPolicy, Url, http,
};
use input::{ErasedEditorEvent, InputField};
use language::{Buffer, PLAIN_TEXT};
use multi_buffer::MultiBuffer;
use path::PathStyle;
use project::{
    Project, ProjectPath, RequestBuffer, RequestBufferEvent, RequestFile, RequestFileBody,
    RequestFileBodyType, RequestFileFormField, RequestFileHeader, RequestFileHttp, RequestFileMeta,
    RequestFileParam, RequestFileState,
};
use response_panel::{
    Response, ResponseCookie, ResponseHeader, ResponsePanel, ResponsePanelTab, ResponseState,
    format_json,
};
use theme::ActiveTheme;
use ui::{
    Button, ButtonCommon, ButtonSize, ButtonVariant, Clickable, Color, ContextMenu, DropdownMenu,
    DropdownVariant, DynamicSpacing, FixedWidth, IconAsset, IconButton, IconButtonShape,
    IconPosition, IconSize, LineHeightStyle, ScrollAxes, Scrollbars, Text, TextCommon, TextSize,
    ToggleState, Tooltip, TrackLayout, WithScrollbar,
};
use workspace::{AppState, Workspace, WorkspaceEvent, pane::Pane};

pub fn init(cx: &mut App) {
    workspace::register_project_item::<RequestEditor>(cx);
    workspace::register_serializable_item::<RequestEditor>(cx);

    cx.observe_new(
        |workspace: &mut Workspace, window: Option<&mut Window>, cx: &mut Context<Workspace>| {
            if let Some(window) = window {
                let workspace_handle = cx.entity();

                cx.subscribe_in(
                    &workspace_handle,
                    window,
                    move |workspace, _, event, window, cx| match event {
                        WorkspaceEvent::ActiveItemChanged => {
                            update_response_panel(workspace, window, cx);
                        }
                        WorkspaceEvent::PaneAdded(_) | WorkspaceEvent::PaneRestored(_) => {}
                    },
                )
                .detach();
            }

            workspace.register_action(
                |workspace, _: &actions::workspace::SendRequest, window, cx| {
                    let pane = workspace.pane().clone();
                    window.defer(cx, move |window, cx| {
                        pane.update(cx, |pane, cx| {
                            pane.send_request(window, cx);
                        });
                    });
                },
            );
        },
    )
    .detach();
}

fn update_response_panel(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let active_request_editor = workspace.active_item_as::<RequestEditor>(cx);
    let (has_response_context, response, active_response_tab, on_active_response_tab_change) =
        if let Some(request_editor) = active_request_editor {
            let (has_response_context, response, active_response_tab) = {
                let request_editor = request_editor.read(cx);
                match &request_editor.request {
                    RequestEditorState::Ready(_) => (
                        true,
                        request_editor.response(),
                        request_editor.active_response_tab(),
                    ),
                    RequestEditorState::Invalid { .. } => (false, None, ResponsePanelTab::Body),
                }
            };
            (
                has_response_context,
                response,
                active_response_tab,
                has_response_context
                    .then(|| on_active_response_tab_change(request_editor.downgrade())),
            )
        } else {
            (false, None, ResponsePanelTab::Body, None)
        };

    let Some(response_panel) = workspace.panel::<ResponsePanel>(cx) else {
        return;
    };

    let should_open_response_panel = response_panel.update(cx, |response_panel, cx| {
        response_panel.set_response(
            response,
            active_response_tab,
            on_active_response_tab_change,
            has_response_context,
            cx,
        );

        if has_response_context {
            response_panel.take_auto_hidden()
        } else {
            false
        }
    });

    if has_response_context && should_open_response_panel {
        workspace.open_panel::<ResponsePanel>(window, cx);
        return;
    }

    if !has_response_context {
        let docks = [
            workspace.left_dock().clone(),
            workspace.bottom_dock().clone(),
        ];

        if let Some(open_dock) = docks.into_iter().find(|dock| {
            let dock = dock.read(cx);
            let Some(response_panel_index) = dock.panel_index_for_type::<ResponsePanel>() else {
                return false;
            };

            dock.is_open() && dock.active_panel_index() == Some(response_panel_index)
        }) {
            response_panel.update(cx, |response_panel, _| {
                response_panel.mark_auto_hidden();
            });

            open_dock.update(cx, |dock, cx| {
                dock.set_open(false, window, cx);
            });
        }
    }
}

fn on_active_response_tab_change(
    request_editor: WeakEntity<RequestEditor>,
) -> Rc<dyn Fn(ResponsePanelTab, &mut Context<ResponsePanel>)> {
    Rc::new(move |active_response_tab, cx| {
        if let Err(error) = request_editor.update(cx, |request_editor, cx| {
            request_editor.set_active_response_tab(active_response_tab, cx);
        }) {
            log::debug!("Failed to update active response tab: {error:?}");
        }
    })
}

fn response_headers(headers: &http::HeaderMap) -> Vec<ResponseHeader> {
    headers
        .iter()
        .map(|(name, value)| {
            ResponseHeader::new(
                name.as_str().to_string(),
                String::from_utf8_lossy(value.as_bytes()).into_owned(),
            )
        })
        .collect()
}

fn response_cookies(headers: &http::HeaderMap) -> Vec<ResponseCookie> {
    headers
        .get_all(http::header::SET_COOKIE)
        .iter()
        .filter_map(|header| {
            let header = String::from_utf8_lossy(header.as_bytes());
            let cookie = cookie::Cookie::parse(header.as_ref()).ok()?;
            Some(
                ResponseCookie::new(cookie.name().to_string(), cookie.value().to_string())
                    .domain(cookie.domain().map(str::to_string))
                    .path(cookie.path().map(str::to_string))
                    .expires(cookie.expires().map(|expires| {
                        expires.datetime().map_or_else(
                            || "session".to_string(),
                            |expires_datetime| expires_datetime.to_string(),
                        )
                    }))
                    .max_age(
                        cookie
                            .max_age()
                            .map(|max_age| max_age.whole_seconds().to_string()),
                    )
                    .secure(cookie.secure())
                    .http_only(cookie.http_only())
                    .same_site(cookie.same_site().map(|same_site| match same_site {
                        cookie::SameSite::Strict => "strict",
                        cookie::SameSite::Lax => "lax",
                        cookie::SameSite::None => "none",
                    })),
            )
        })
        .collect()
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

enum RequestEditorState {
    Ready(Request),
    Invalid {
        error: String,
        snapshot: Option<RequestSnapshot>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RequestEditorTab {
    Parameters,
    Headers,
    Body,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestEditorEvent {
    RequestBufferEdited,
    DirtyChanged,
    Saved,
    TitleChanged,
    FileHandleChanged,
}

type RequestMeta = RequestFileMeta;

type RequestBodyType = RequestFileBodyType;

struct RequestHttp {
    method: Method,
    url: Entity<InputField>,
    params: Vec<KeyValueRow>,
    headers: Vec<KeyValueRow>,
    body_type: Option<RequestBodyType>,
    body: Option<RequestBody>,
    form_url_encoded: Vec<KeyValueRow>,
}

struct Request {
    meta: RequestMeta,
    http: RequestHttp,
}

impl Request {
    fn from_request_file(
        request_file: &RequestFile,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Self, String> {
        let method = Method::from_bytes(request_file.http.method.as_bytes()).map_err(|error| {
            format!(
                "Invalid request method `{}`: {error}",
                request_file.http.method
            )
        })?;
        let url = cx.new(|cx| InputField::new(window, cx, "https://example.com"));
        url.update(cx, |field, cx| {
            field.set_value(&request_file.http.url, window, cx);
        });
        let mut params = Vec::new();
        for param in &request_file.http.params {
            let mut row = self::new_kv_row(
                Some(&param.name),
                Some(&param.value),
                RequestKvKind::Param,
                window,
                cx,
            );
            row.set_disabled(param.disabled, cx);
            params.push(row);
        }
        let mut headers = Vec::new();
        for header in &request_file.http.headers {
            let mut row = self::new_kv_row(
                Some(&header.name),
                Some(&header.value),
                RequestKvKind::Header,
                window,
                cx,
            );
            row.set_disabled(header.disabled, cx);
            headers.push(row);
        }
        let body_type = request_file
            .http
            .body
            .as_ref()
            .map(RequestFileBody::body_type);
        let mut body = None;
        let mut form_url_encoded = Vec::new();
        match &request_file.http.body {
            Some(
                RequestFileBody::Text { data }
                | RequestFileBody::Json { data }
                | RequestFileBody::Html { data }
                | RequestFileBody::Xml { data },
            ) => body = Some(RequestBody::new(data.clone(), window, cx)),
            Some(RequestFileBody::FormUrlEncoded { data }) => {
                for field in data {
                    let mut row = self::new_kv_row(
                        Some(&field.name),
                        Some(&field.value),
                        RequestKvKind::FormUrlEncoded,
                        window,
                        cx,
                    );
                    row.set_disabled(field.disabled, cx);
                    form_url_encoded.push(row);
                }
            }
            None => {}
        }

        Ok(Self {
            meta: request_file.meta.clone(),
            http: RequestHttp {
                method,
                url,
                params,
                headers,
                body_type,
                body,
                form_url_encoded,
            },
        })
    }
}

#[derive(Clone, PartialEq, Eq)]
struct RequestSnapshot(RequestFile);

impl RequestSnapshot {
    fn from_request(request: &Request, cx: &App) -> Self {
        Self(RequestFile {
            meta: request.meta.clone(),
            http: RequestFileHttp {
                method: request.http.method.as_str().to_owned(),
                url: request.http.url.read(cx).value(cx),
                params: request
                    .http
                    .params
                    .iter()
                    .map(|param| RequestFileParam {
                        name: param.key.read(cx).text(cx),
                        value: param.value.read(cx).text(cx),
                        disabled: param.disabled,
                    })
                    .collect(),
                headers: request
                    .http
                    .headers
                    .iter()
                    .map(|header| RequestFileHeader {
                        name: header.key.read(cx).text(cx),
                        value: header.value.read(cx).text(cx),
                        disabled: header.disabled,
                    })
                    .collect(),
                body: request.http.body_type.and_then(|body_type| {
                    let body = request.http.body.as_ref();
                    Some(match body_type {
                        RequestBodyType::Text => RequestFileBody::Text {
                            data: body?.data(cx),
                        },
                        RequestBodyType::Json => RequestFileBody::Json {
                            data: body?.data(cx),
                        },
                        RequestBodyType::Html => RequestFileBody::Html {
                            data: body?.data(cx),
                        },
                        RequestBodyType::Xml => RequestFileBody::Xml {
                            data: body?.data(cx),
                        },
                        RequestBodyType::FormUrlEncoded => RequestFileBody::FormUrlEncoded {
                            data: request
                                .http
                                .form_url_encoded
                                .iter()
                                .map(|row| RequestFileFormField {
                                    name: row.key.read(cx).text(cx),
                                    value: row.value.read(cx).text(cx),
                                    disabled: row.disabled,
                                })
                                .collect(),
                        },
                    })
                }),
            },
        })
    }

    fn from_request_file(request_file: &RequestFile) -> Self {
        Self(request_file.clone())
    }
}

struct RequestBody {
    editor: Entity<Editor>,
    payload: Entity<MultiBuffer>,
}

impl RequestBody {
    fn new(data: impl Into<String>, window: &mut Window, cx: &mut App) -> Self {
        let data = data.into();
        let payload = cx.new(move |cx| {
            let buffer =
                cx.new(move |cx| Buffer::local(data, cx).with_language(PLAIN_TEXT.clone(), cx));
            MultiBuffer::singleton(buffer, cx)
        });
        let editor = cx.new(|cx| Editor::for_multibuffer(payload.clone(), window, cx));

        Self { editor, payload }
    }

    fn data(&self, cx: &App) -> String {
        self.payload.read(cx).snapshot(cx).text()
    }

    fn editor(&self) -> Entity<Editor> {
        self.editor.clone()
    }
}

#[derive(Clone, Copy)]
enum RequestKvKind {
    Param,
    Header,
    FormUrlEncoded,
}

impl RequestKvKind {
    fn rows_mut(self, http: &mut RequestHttp) -> &mut Vec<KeyValueRow> {
        match self {
            RequestKvKind::Param => &mut http.params,
            RequestKvKind::Header => &mut http.headers,
            RequestKvKind::FormUrlEncoded => &mut http.form_url_encoded,
        }
    }

    fn id(self) -> &'static str {
        match self {
            RequestKvKind::Param => "parameters",
            RequestKvKind::Header => "headers",
            RequestKvKind::FormUrlEncoded => "form-urlencoded",
        }
    }

    fn row_id(self) -> &'static str {
        match self {
            RequestKvKind::Param => "param-row",
            RequestKvKind::Header => "header-row",
            RequestKvKind::FormUrlEncoded => "form-urlencoded-row",
        }
    }

    fn checkbox_id(self) -> &'static str {
        match self {
            RequestKvKind::Param => "param-checkbox",
            RequestKvKind::Header => "header-checkbox",
            RequestKvKind::FormUrlEncoded => "form-urlencoded-checkbox",
        }
    }

    fn remove_id(self) -> &'static str {
        match self {
            RequestKvKind::Param => "param-delete",
            RequestKvKind::Header => "header-delete",
            RequestKvKind::FormUrlEncoded => "form-urlencoded-delete",
        }
    }

    fn add_id(self) -> &'static str {
        match self {
            RequestKvKind::Param => "param-add",
            RequestKvKind::Header => "header-add",
            RequestKvKind::FormUrlEncoded => "form-urlencoded-add",
        }
    }

    fn add_label(self) -> &'static str {
        match self {
            RequestKvKind::Param => "Add Parameter",
            RequestKvKind::Header => "Add Header",
            RequestKvKind::FormUrlEncoded => "Add Field",
        }
    }

    fn scrollbar_id(self) -> &'static str {
        match self {
            RequestKvKind::Param => "parameters-scrollbar",
            RequestKvKind::Header => "headers-scrollbar",
            RequestKvKind::FormUrlEncoded => "form-urlencoded-scrollbar",
        }
    }
}

struct KeyValueRow {
    key: Entity<Editor>,
    value: Entity<Editor>,
    disabled: bool,
}

impl KeyValueRow {
    fn set_disabled(&mut self, disabled: bool, cx: &mut App) {
        self.disabled = disabled;
        for editor in [&self.key, &self.value] {
            editor.update(cx, |editor, cx| {
                editor.set_muted(disabled);
                cx.notify();
            });
        }
    }
}

fn new_kv_row(
    key: Option<&str>,
    value: Option<&str>,
    kind: RequestKvKind,
    window: &mut Window,
    cx: &mut App,
) -> KeyValueRow {
    let key = cx.new(|cx| {
        let mut editor = Editor::single_line(window, cx);
        editor.set_placeholder_text("Key", cx);
        if let Some(key) = key {
            editor.set_text(key, cx);
        }
        editor
    });
    let value = cx.new(|cx| {
        let mut editor = match kind {
            RequestKvKind::Param | RequestKvKind::FormUrlEncoded => {
                Editor::auto_height(1, Some(4), window, cx)
            }
            RequestKvKind::Header => Editor::single_line(window, cx),
        };
        editor.set_placeholder_text("Value", cx);
        if let Some(value) = value {
            editor.set_text(value, cx);
        }
        editor
    });
    KeyValueRow {
        key,
        value,
        disabled: false,
    }
}

fn input_box(editor: &Entity<Editor>, disabled: bool, cx: &mut App) -> Div {
    let text_style = TextStyleRefinement {
        color: Some(cx.theme().colors().text),
        line_height: Some(gpui::relative(1.2)),
        ..Default::default()
    };
    editor.update(cx, |editor, _| {
        editor.set_text_style_refinement(text_style);
    });

    let colors = cx.theme().colors();
    let focus_handle = editor.focus_handle(cx).tab_index(0).tab_stop(true);
    gpui::div()
        .flex()
        .flex_1()
        .min_w_0()
        .min_h_8()
        .px_2()
        .py_1p5()
        .rounded_md()
        .border_1()
        .border_color(if disabled {
            colors.border_disabled
        } else {
            colors.border_variant
        })
        .bg(colors.editor_background)
        .track_focus(&focus_handle)
        .focus(|this| {
            this.border_color(if disabled {
                colors.border_disabled
            } else {
                colors.border_focused
            })
        })
        .child(editor.clone())
}

pub struct RequestEditor {
    focus_handle: FocusHandle,
    workspace: WeakEntity<Workspace>,
    project: Entity<Project>,
    buffer: Entity<RequestBuffer>,
    request: RequestEditorState,
    request_snapshot: Option<RequestSnapshot>,
    active_tab: RequestEditorTab,
    active_response_tab: ResponsePanelTab,
    response: Option<Entity<Response>>,
    http_client: Arc<dyn HttpClient>,
    params_scroll_handle: ScrollHandle,
    headers_scroll_handle: ScrollHandle,
    form_url_encoded_scroll_handle: ScrollHandle,
    input_subscriptions: Vec<Subscription>,
    body_subscription: Option<Subscription>,
    _buffer_subscription: Subscription,
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
        let request_file = buffer.read(cx).request_file().clone();
        let (request, request_snapshot, input_subscriptions, body_subscription) =
            Self::state_from_request_file(request_file, window, cx);
        let request_editor = cx.weak_entity();
        let buffer_subscription = window.subscribe(
            &buffer,
            cx,
            move |buffer, event: &RequestBufferEvent, window, cx| match event {
                RequestBufferEvent::DirtyChanged => {
                    if let Err(error) = request_editor.update(cx, |_, cx| {
                        cx.emit(RequestEditorEvent::DirtyChanged);
                        cx.notify();
                    }) {
                        log::debug!("Failed to update request editor tab: {error:?}");
                    }
                }
                RequestBufferEvent::FileHandleChanged => {
                    if let Err(error) = request_editor.update(cx, |_, cx| {
                        cx.emit(RequestEditorEvent::TitleChanged);
                        cx.emit(RequestEditorEvent::FileHandleChanged);
                        cx.notify();
                    }) {
                        log::debug!("Failed to update request editor tab: {error:?}");
                    }
                }
                RequestBufferEvent::Saved => {
                    if let Err(error) = request_editor.update(cx, |_, cx| {
                        cx.emit(RequestEditorEvent::Saved);
                        cx.notify();
                    }) {
                        log::debug!("Failed to update request editor tab: {error:?}");
                    }
                }
                RequestBufferEvent::Reloaded => {
                    let request_file = buffer.read(cx).request_file().clone();
                    if let Err(error) = request_editor.update(cx, |request_editor, cx| {
                        let (request, request_snapshot, input_subscriptions, body_subscription) =
                            Self::state_from_request_file(request_file, window, cx);
                        request_editor.request = request;
                        request_editor.request_snapshot = request_snapshot;
                        request_editor.input_subscriptions = input_subscriptions;
                        request_editor.body_subscription = body_subscription;
                        request_editor.set_language_for_body(cx);
                        cx.emit(RequestEditorEvent::TitleChanged);
                        cx.notify();
                    }) {
                        log::debug!("Failed to reload request editor: {error:?}");
                    }
                }
                RequestBufferEvent::ReloadNeeded => {}
            },
        );

        let this = Self {
            focus_handle,
            workspace,
            project,
            buffer,
            request,
            request_snapshot,
            active_tab: RequestEditorTab::Parameters,
            active_response_tab: ResponsePanelTab::Body,
            response: None,
            http_client: AppState::global(cx).client.http_client(),
            params_scroll_handle: ScrollHandle::new(),
            headers_scroll_handle: ScrollHandle::new(),
            form_url_encoded_scroll_handle: ScrollHandle::new(),
            input_subscriptions,
            body_subscription,
            _buffer_subscription: buffer_subscription,
        };
        this.set_language_for_body(cx);
        this
    }

    fn project_path(&self, cx: &App) -> Option<ProjectPath> {
        project::ProjectItem::project_path(self.buffer.read(cx), cx)
    }

    fn path_style(&self, cx: &App) -> PathStyle {
        self.project.read(cx).path_style(cx)
    }

    fn response(&self) -> Option<Entity<Response>> {
        self.response.clone()
    }

    fn active_response_tab(&self) -> ResponsePanelTab {
        self.active_response_tab
    }

    fn set_active_response_tab(
        &mut self,
        active_response_tab: ResponsePanelTab,
        cx: &mut Context<Self>,
    ) {
        if self.active_response_tab != active_response_tab {
            self.active_response_tab = active_response_tab;
            cx.notify();
        }
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
        include_file_name: bool,
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

        if include_file_name {
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
        subscriptions.push(Self::subscribe_to_input(&request.http.url, window, cx));
        for row in request
            .http
            .params
            .iter()
            .chain(&request.http.headers)
            .chain(&request.http.form_url_encoded)
        {
            subscriptions.push(Self::subscribe_to_editor(&row.key, window, cx));
            subscriptions.push(Self::subscribe_to_editor(&row.value, window, cx));
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

    fn subscribe_to_editor(
        editor: &Entity<Editor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Subscription {
        let request_editor = cx.weak_entity();
        window.subscribe(editor, cx, move |_, event: &EditorEvent, _, cx| {
            if *event == EditorEvent::BufferEdited
                && let Err(error) = request_editor.update(cx, |request_editor, cx| {
                    request_editor.mark_edited(cx);
                })
            {
                log::debug!("Failed to update request editor edit state: {error:?}");
            }
        })
    }

    fn set_language_for_body(&self, cx: &mut Context<Self>) {
        let Some((body_type, payload)) = (match &self.request {
            RequestEditorState::Ready(request) => request
                .http
                .body
                .as_ref()
                .map(|body| (request.http.body_type, body.payload.clone())),
            RequestEditorState::Invalid { .. } => None,
        }) else {
            return;
        };

        let language_name = match body_type {
            Some(RequestBodyType::Text) | None => {
                payload.update(cx, |payload, cx| {
                    if let Some(buffer) = payload.as_singleton() {
                        buffer.update(cx, |buffer, cx| {
                            buffer.set_language(Some(PLAIN_TEXT.clone()), cx);
                        });
                    }
                });
                return;
            }
            Some(RequestBodyType::Json) => "JSON",
            Some(RequestBodyType::Html) => "HTML",
            Some(RequestBodyType::Xml) => "XML",
            Some(RequestBodyType::FormUrlEncoded) => return,
        };

        let payload_id = payload.entity_id();
        let languages = AppState::global(cx).languages.clone();
        cx.spawn(async move |this, cx| {
            let language = match languages.language_for_name(language_name).await {
                Ok(language) => language,
                Err(error) => {
                    log::error!("Failed to load {language_name} language: {error:?}");
                    PLAIN_TEXT.clone()
                }
            };

            if let Err(error) = this.update(cx, |request_editor, cx| {
                let RequestEditorState::Ready(request) = &request_editor.request else {
                    return;
                };
                if request.http.body_type != body_type {
                    return;
                }
                let Some(body) = request.http.body.as_ref() else {
                    return;
                };
                if body.payload.entity_id() != payload_id {
                    return;
                }

                body.payload.update(cx, |payload, cx| {
                    if let Some(buffer) = payload.as_singleton() {
                        let language = language.clone();
                        buffer.update(cx, |buffer, cx| {
                            buffer.set_language(Some(language), cx);
                        });
                    }
                });
            }) {
                log::debug!("Failed to set request body language: {error:?}");
            }
        })
        .detach();
    }

    fn state_from_request_file(
        request_file: RequestFileState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (
        RequestEditorState,
        Option<RequestSnapshot>,
        Vec<Subscription>,
        Option<Subscription>,
    ) {
        let request = match request_file {
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
        let body_subscription = match &request {
            RequestEditorState::Ready(request) => request
                .http
                .body
                .as_ref()
                .map(|body| Self::subscribe_to_editor(&body.editor, window, cx)),
            RequestEditorState::Invalid { .. } => None,
        };

        (
            request,
            request_snapshot,
            input_subscriptions,
            body_subscription,
        )
    }

    fn mark_edited(&mut self, cx: &mut Context<Self>) {
        let request_snapshot = match &self.request {
            RequestEditorState::Ready(request) => Some(RequestSnapshot::from_request(request, cx)),
            RequestEditorState::Invalid { .. } => None,
        };
        let is_dirty = if let (Some(saved_snapshot), Some(snapshot)) =
            (self.request_snapshot.as_ref(), request_snapshot.as_ref())
        {
            snapshot != saved_snapshot
        } else {
            false
        };
        let request = request_snapshot
            .as_ref()
            .map(|snapshot| RequestFileState::Parsed(snapshot.0.clone()));
        let dirty_changed = self.buffer.update(cx, |buffer, cx| {
            if let Some(request) = request {
                buffer.set_request_file(request, cx);
            }
            buffer.set_dirty(is_dirty, cx)
        });

        cx.emit(RequestEditorEvent::RequestBufferEdited);
        if dirty_changed {
            cx.emit(RequestEditorEvent::DirtyChanged);
        }
        cx.notify();
    }

    fn set_body_type(
        &mut self,
        body_type: Option<RequestBodyType>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut edited = false;
        let mut should_set_language_for_body = false;

        if let RequestEditorState::Ready(request) = &mut self.request {
            match body_type {
                Some(
                    RequestBodyType::Text
                    | RequestBodyType::Json
                    | RequestBodyType::Html
                    | RequestBodyType::Xml,
                ) => {
                    if request.http.body.is_none() {
                        let body = RequestBody::new("", window, cx);
                        self.body_subscription =
                            Some(Self::subscribe_to_editor(&body.editor, window, cx));
                        request.http.body = Some(body);
                        should_set_language_for_body = true;
                    }

                    if request.http.body_type != body_type {
                        request.http.body_type = body_type;
                        should_set_language_for_body = true;
                        edited = true;
                    }
                }
                Some(RequestBodyType::FormUrlEncoded) => {
                    if request.http.body_type != Some(RequestBodyType::FormUrlEncoded) {
                        request.http.body_type = Some(RequestBodyType::FormUrlEncoded);
                        edited = true;
                    }
                }
                None => {
                    if request.http.body_type.take().is_some() {
                        should_set_language_for_body = true;
                        edited = true;
                    }
                }
            }
        }

        if should_set_language_for_body {
            self.set_language_for_body(cx);
        }

        if edited {
            self.mark_edited(cx);
        }
    }

    pub fn send_request(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let RequestEditorState::Ready(request) = &self.request else {
            return;
        };

        let request_method = request.http.method.clone();
        let request_url = request.http.url.read(cx).value(cx);
        let request_params = request
            .http
            .params
            .iter()
            .filter_map(|param| {
                if param.disabled {
                    return None;
                }

                let name = param.key.read(cx).text(cx).trim().to_string();
                if name.is_empty() {
                    return None;
                }

                let value = param.value.read(cx).text(cx);
                Some((name, value))
            })
            .collect::<Vec<_>>();
        let mut request_headers = Vec::new();
        let mut content_type = None;
        for header in &request.http.headers {
            if header.disabled {
                continue;
            }

            let name = header.key.read(cx).text(cx).trim().to_string();
            if name.is_empty() {
                continue;
            }

            let value = header.value.read(cx).text(cx);
            if name.eq_ignore_ascii_case("content-type") {
                content_type = Some((name, value));
            } else {
                request_headers.push((name, value));
            }
        }
        let request_body = match request.http.body_type {
            Some(
                RequestBodyType::Text
                | RequestBodyType::Json
                | RequestBodyType::Html
                | RequestBodyType::Xml,
            ) => request
                .http
                .body
                .as_ref()
                .map(|body| body.data(cx))
                .filter(|body| !body.is_empty()),
            Some(RequestBodyType::FormUrlEncoded) => {
                content_type.get_or_insert_with(|| {
                    (
                        "Content-Type".to_string(),
                        "application/x-www-form-urlencoded".to_string(),
                    )
                });
                let fields = request
                    .http
                    .form_url_encoded
                    .iter()
                    .filter(|row| !row.disabled)
                    .map(|row| (row.key.read(cx).text(cx), row.value.read(cx).text(cx)));

                Some(
                    url::form_urlencoded::Serializer::new(String::new())
                        .extend_pairs(fields)
                        .finish(),
                )
            }
            None => None,
        };
        if let Some(content_type) = content_type {
            request_headers.push(content_type);
        }

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
        let active_response_tab = self.active_response_tab;
        let on_active_response_tab_change = on_active_response_tab_change(cx.weak_entity());
        response_panel.update(cx, |panel, cx| {
            panel.set_response(
                Some(response.clone()),
                active_response_tab,
                Some(on_active_response_tab_change),
                true,
                cx,
            );
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
        let languages = AppState::global(cx).languages.clone();

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
                            response.set_payload(
                                request_id,
                                "Error: invalid URL".to_owned(),
                                None,
                                None,
                                cx,
                            );
                        });
                        return;
                    };

                    if !request_params.is_empty() {
                        let mut query_pairs = request_url.query_pairs_mut();
                        for (name, value) in request_params {
                            query_pairs.append_pair(&name, &value);
                        }
                    }

                    let mut builder = Builder::new()
                        .method(request_method)
                        .uri(request_url.as_str())
                        .follow_redirects(RedirectPolicy::FollowAll);

                    if !request_headers.is_empty() {
                        for (name, value) in request_headers {
                            builder = builder.header(name.as_str(), value.as_str());
                        }
                    }

                    let request_body = request_body.map_or_else(AsyncBody::empty, AsyncBody::from);
                    let request = match builder.body(request_body) {
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
                                response.set_payload(
                                    request_id,
                                    format!("Error: {error}"),
                                    None,
                                    None,
                                    cx,
                                );
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
                                        let causes = error
                                            .chain()
                                            .skip(1)
                                            .map(|cause| format!("  - {cause}"))
                                            .collect::<Vec<_>>();
                                        let payload = if causes.is_empty() {
                                            format!("Error: {error}")
                                        } else {
                                            format!(
                                                "Error: {error}\n\nCaused by:\n{}",
                                                causes.join("\n")
                                            )
                                        };
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
                                                payload,
                                                None,
                                                None,
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
                    let response_headers = response_headers(received.headers());
                    let response_cookies = response_cookies(received.headers());
                    let still_active = response.update(cx, |response, cx| {
                        response.set_headers(request_id, response_headers, cx)
                            && response.set_cookies(request_id, response_cookies, cx)
                    });
                    if !still_active {
                        return;
                    }

                    let content_type = received
                        .headers()
                        .get(http::header::CONTENT_TYPE)
                        .and_then(|content_type| content_type.to_str().ok())
                        .map(str::to_owned);
                    let language_name = content_type.as_deref().and_then(|content_type| {
                        let media_type = content_type.split(';').next()?.trim();
                        let media_type_lowercase = media_type.to_ascii_lowercase();

                        if media_type.eq_ignore_ascii_case("application/json")
                            || media_type.eq_ignore_ascii_case("text/json")
                            || media_type_lowercase.ends_with("+json")
                        {
                            Some("JSON")
                        } else if media_type.eq_ignore_ascii_case("text/html") {
                            Some("HTML")
                        } else if media_type.eq_ignore_ascii_case("application/xml")
                            || media_type.eq_ignore_ascii_case("text/xml")
                            || media_type_lowercase.ends_with("+xml")
                        {
                            Some("XML")
                        } else {
                            None
                        }
                    });
                    let language = language_name.map(|language_name| {
                        let languages = languages.clone();
                        cx.background_executor().spawn(async move {
                            match languages.language_for_name(language_name).await {
                                Ok(language) => Some(language),
                                Err(error) => {
                                    log::error!(
                                        "Failed to load {language_name} language: {error:?}"
                                    );
                                    None
                                }
                            }
                        })
                    });
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
                                        payload.extend_from_slice(
                                            buffer
                                                .get(..chunk)
                                                .expect("read chunk should fit in buffer"),
                                        );
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
                    let read_succeeded = read_error.is_none();
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
                    let pretty_payload =
                        (read_succeeded && language_name == Some("JSON")).then(|| {
                            let payload = payload.clone();
                            cx.background_executor()
                                .spawn(async move { format_json(&payload).unwrap_or(payload) })
                        });
                    let language = if read_succeeded {
                        match language {
                            Some(language) => language.await,
                            None => None,
                        }
                    } else {
                        None
                    };
                    let pretty_payload = match pretty_payload {
                        Some(pretty_payload) => Some(pretty_payload.await),
                        None => None,
                    };

                    response.update(cx, |response, cx| {
                        response.set_state(request_id, response_state, cx)
                            && response.set_payload(
                                request_id,
                                payload,
                                pretty_payload,
                                language,
                                cx,
                            )
                    });
                }
            })
            .detach();
    }

    fn render_invalid(&self, error: &str, cx: &mut Context<Self>) -> Div {
        gpui::div()
            .flex()
            .flex_col()
            .track_focus(&self.focus_handle)
            .w_full()
            .flex_1()
            .min_h_0()
            .gap_2()
            .p_3()
            .bg(cx.theme().colors().panel_background)
            .child(
                Text::new("Invalid Request")
                    .size(TextSize::Large)
                    .color(Color::Error),
            )
            .child(Text::new(error.to_string()).color(Color::Muted))
    }

    fn render_tab_bar(
        &self,
        request: &Request,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active_tab = self.active_tab;
        let body_type_dropdown = (active_tab == RequestEditorTab::Body).then(|| {
            let body_type = request.http.body_type;
            let label = body_type.map_or("None", |body_type| body_type.display_name());
            let request_editor = cx.weak_entity();

            let context_menu = ContextMenu::build(window, cx, |mut menu, _, _| {
                for type_option in [
                    None,
                    Some(RequestBodyType::Text),
                    Some(RequestBodyType::Json),
                    Some(RequestBodyType::Html),
                    Some(RequestBodyType::Xml),
                    Some(RequestBodyType::FormUrlEncoded),
                ] {
                    let request_editor = request_editor.clone();
                    let display_name =
                        type_option.map_or("None", |body_type| body_type.display_name());
                    menu = menu.toggleable_entry(
                        display_name,
                        type_option == body_type,
                        IconPosition::End,
                        None,
                        move |window, cx| {
                            if let Err(error) = request_editor.update(cx, |request_editor, cx| {
                                request_editor.set_body_type(type_option, window, cx);
                            }) {
                                log::debug!("Failed to update request body type: {error:?}");
                            }
                        },
                    );
                }
                menu
            });

            DropdownMenu::new("body-type", label, context_menu)
                .variant(DropdownVariant::OutlinedGhost)
                .trigger_text_size(TextSize::Small)
                .trigger_icon(IconAsset::CaretDown)
                .anchor(Anchor::TopRight)
                .offset(gpui::point(gpui::px(0.0), gpui::px(0.5)))
        });
        let colors = cx.theme().colors();

        let render_tab =
            |id: ElementId, active: bool, title: SharedString, set_active_tab: RequestEditorTab| {
                let colors = cx.theme().colors();

                gpui::div()
                    .id(id)
                    .relative()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .h_full()
                    .min_w(DynamicSpacing::Base48.px(cx))
                    .px(DynamicSpacing::Base08.px(cx))
                    .cursor_pointer()
                    .on_click(cx.listener(move |request_editor, _, _, cx| {
                        cx.stop_propagation();
                        if request_editor.active_tab != set_active_tab {
                            request_editor.active_tab = set_active_tab;
                            cx.notify();
                        }
                    }))
                    .child(
                        gpui::div()
                            .relative()
                            .flex()
                            .items_center()
                            .h_full()
                            .when(active, |this| {
                                this.child(
                                    gpui::div()
                                        .absolute()
                                        .left_0()
                                        .right_0()
                                        .bottom_0()
                                        .h(DynamicSpacing::Base01.px(cx))
                                        .bg(colors.panel_tab_active_foreground),
                                )
                            })
                            .child(
                                Text::new(title)
                                    .size(TextSize::Small)
                                    .line_height_style(LineHeightStyle::Compact)
                                    .weight(FontWeight::MEDIUM)
                                    .color(if active {
                                        Color::Custom(colors.panel_tab_active_foreground)
                                    } else {
                                        Color::Custom(colors.panel_tab_inactive_foreground)
                                    })
                                    .single_line(),
                            ),
                    )
            };

        gpui::div()
            .id("request-editor-tabs")
            .flex()
            .flex_none()
            .items_center()
            .w_full()
            .h(DynamicSpacing::Base36.px(cx))
            .pl_1()
            .pr_2()
            .border_y_1()
            .border_color(colors.border)
            .bg(colors.panel_tab_bar_background)
            .child(render_tab(
                ElementId::Name("parameters-tab".into()),
                active_tab == RequestEditorTab::Parameters,
                "Parameters".into(),
                RequestEditorTab::Parameters,
            ))
            .child(render_tab(
                ElementId::Name("headers-tab".into()),
                active_tab == RequestEditorTab::Headers,
                "Headers".into(),
                RequestEditorTab::Headers,
            ))
            .child(render_tab(
                ElementId::Name("body-tab".into()),
                active_tab == RequestEditorTab::Body,
                "Body".into(),
                RequestEditorTab::Body,
            ))
            .when_some(body_type_dropdown, |this, body_type_dropdown| {
                this.child(gpui::div().flex_1().min_w_0())
                    .child(gpui::div().flex_none().child(body_type_dropdown))
            })
            .into_any_element()
    }

    fn render_tab_content(
        &self,
        request: &Request,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match self.active_tab {
            RequestEditorTab::Parameters => {
                self.render_kv_section(&request.http.params, RequestKvKind::Param, window, cx)
            }
            RequestEditorTab::Headers => {
                self.render_kv_section(&request.http.headers, RequestKvKind::Header, window, cx)
            }
            RequestEditorTab::Body => self.render_body(request, window, cx),
        }
    }

    fn render_kv_section(
        &self,
        rows: &[KeyValueRow],
        kind: RequestKvKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let checkbox = ui::checkbox(
                    (kind.checkbox_id(), index),
                    ToggleState::from(!row.disabled),
                )
                .on_click(cx.listener(
                    move |request_editor, new_state: &ToggleState, _, cx| {
                        let disabled = !new_state.selected();
                        if let RequestEditorState::Ready(request) = &mut request_editor.request
                            && let Some(row) = kind.rows_mut(&mut request.http).get_mut(index)
                            && row.disabled != disabled
                        {
                            row.set_disabled(disabled, cx);
                            request_editor.mark_edited(cx);
                        }
                    },
                ));
                let delete_button = IconButton::new((kind.remove_id(), index), IconAsset::Trash)
                    .shape(IconButtonShape::Square)
                    .variant(ButtonVariant::Outline)
                    .icon_color(Color::Muted)
                    .tooltip(Tooltip::text("Delete"))
                    .on_click(cx.listener(move |request_editor, _, window, cx| {
                        let RequestEditorState::Ready(request) = &mut request_editor.request else {
                            return;
                        };
                        let rows = kind.rows_mut(&mut request.http);
                        if index >= rows.len() {
                            return;
                        }
                        rows.remove(index);
                        request_editor.input_subscriptions =
                            Self::subscribe_to_request(request, window, cx);
                        request_editor.mark_edited(cx);
                    }));

                gpui::div()
                    .id((kind.row_id(), index))
                    .flex()
                    .items_center()
                    .w_full()
                    .child(gpui::div().pr_1p5().child(checkbox))
                    .child(
                        gpui::div()
                            .flex()
                            .items_stretch()
                            .flex_1()
                            .min_w_0()
                            .gap_2p5()
                            .child(self::input_box(&row.key, row.disabled, cx).items_center())
                            .child(self::input_box(&row.value, row.disabled, cx))
                            .child(gpui::div().flex().items_center().child(delete_button)),
                    )
            })
            .collect::<Vec<_>>();
        let add_button = Button::new(kind.add_id(), kind.add_label())
            .icon(IconAsset::Plus)
            .icon_size(IconSize::Small)
            .icon_color(Color::Muted)
            .variant(ButtonVariant::OutlinedGhost)
            .size(ButtonSize::Medium)
            .on_click(cx.listener(move |request_editor, _, window, cx| {
                let RequestEditorState::Ready(request) = &mut request_editor.request else {
                    return;
                };

                let row = self::new_kv_row(None, None, kind, window, cx);
                request_editor
                    .input_subscriptions
                    .push(Self::subscribe_to_editor(&row.key, window, cx));
                request_editor
                    .input_subscriptions
                    .push(Self::subscribe_to_editor(&row.value, window, cx));
                kind.rows_mut(&mut request.http).push(row);
                request_editor.mark_edited(cx);
            }));
        let scroll_handle = match kind {
            RequestKvKind::Param => &self.params_scroll_handle,
            RequestKvKind::Header => &self.headers_scroll_handle,
            RequestKvKind::FormUrlEncoded => &self.form_url_encoded_scroll_handle,
        };
        let colors = cx.theme().colors();

        gpui::div()
            .flex()
            .flex_col()
            .w_full()
            .flex_1()
            .min_h_0()
            .child(
                gpui::div()
                    .id(kind.id())
                    .key_context("RequestForm")
                    .flex()
                    .flex_col()
                    .track_scroll(scroll_handle)
                    .size_full()
                    .min_w_0()
                    .overflow_y_scroll()
                    .pl_2()
                    .pr_6()
                    .gap_2()
                    .py_3()
                    .children(rows)
                    .child(gpui::div().flex().items_center().pl_1().child(add_button)),
            )
            .custom_scrollbars(
                Scrollbars::new(ScrollAxes::Vertical)
                    .id(kind.scrollbar_id())
                    .tracked_scroll_handle(scroll_handle)
                    .with_track_along(
                        ScrollAxes::Vertical,
                        colors.scrollbar_track_background,
                        TrackLayout::Overlay,
                    ),
                window,
                cx,
            )
            .into_any_element()
    }

    fn render_body(
        &self,
        request: &Request,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let body_type = request.http.body_type;
        let body = match body_type {
            Some(
                RequestBodyType::Text
                | RequestBodyType::Json
                | RequestBodyType::Html
                | RequestBodyType::Xml,
            ) => request.http.body.as_ref().map(|body| {
                gpui::div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .min_w_0()
                    .child(body.editor())
                    .into_any_element()
            }),
            Some(RequestBodyType::FormUrlEncoded) => Some(self.render_kv_section(
                &request.http.form_url_encoded,
                RequestKvKind::FormUrlEncoded,
                window,
                cx,
            )),
            None => Some(
                gpui::div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        Text::new("No request body")
                            .size(TextSize::Small)
                            .color(Color::Muted),
                    )
                    .into_any_element(),
            ),
        };
        let colors = cx.theme().colors();

        gpui::div()
            .id("body")
            .flex()
            .flex_col()
            .w_full()
            .flex_1()
            .min_h_0()
            .bg(colors.panel_background)
            .children(body)
            .into_any_element()
    }

    fn render_request(
        &self,
        request: &Request,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let url = request.http.url.clone();
        let request_method_menu = {
            let available_request_methods = [
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::PATCH,
                Method::DELETE,
                Method::HEAD,
                Method::OPTIONS,
                Method::QUERY,
            ];
            let selected_request_method = request.http.method.clone();
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
                                    && request.http.method != request_method_for_handler
                                {
                                    request.http.method = request_method_for_handler.clone();
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
        let colors = cx.theme().colors();

        gpui::div()
            .flex()
            .flex_col()
            .track_focus(&self.focus_handle)
            .w_full()
            .flex_1()
            .min_h_0()
            .bg(colors.panel_background)
            .child(
                gpui::div()
                    .flex()
                    .items_center()
                    .w_full()
                    .px_3()
                    .py_3()
                    .gap_2()
                    .key_context("RequestUrl")
                    .on_action(cx.listener(
                        move |request_editor, _: &actions::workspace::SendRequest, window, cx| {
                            request_editor.unpreview_tab(cx);
                            request_editor.send_request(window, cx);
                        },
                    ))
                    .child(
                        DropdownMenu::new(
                            "request-method",
                            request.http.method.as_str().to_owned(),
                            request_method_menu,
                        )
                        .variant(DropdownVariant::OutlinedGhost)
                        .attach(Anchor::BottomLeft)
                        .offset(gpui::point(gpui::px(0.0), gpui::px(0.5)))
                        .trigger_size(ButtonSize::Large),
                    )
                    .child(gpui::div().flex_1().child(url))
                    .child(
                        Button::new("request-send", "Send")
                            .variant(ButtonVariant::Custom {
                                background: colors.text_accent.opacity(0.8),
                                foreground: colors.surface_background,
                                hover_background: colors.text_accent.opacity(0.8),
                                border: gpui::transparent_black(),
                            })
                            .size(ButtonSize::Large)
                            .width(ui::rems_from_px(60.0_f32))
                            .font_weight(FontWeight::MEDIUM)
                            .on_click(cx.listener(move |request_editor, _, window, cx| {
                                request_editor.unpreview_tab(cx);
                                request_editor.send_request(window, cx);
                            })),
                    ),
            )
            .child(self.render_tab_bar(request, window, cx))
            .child(self.render_tab_content(request, window, cx))
    }
}

impl EventEmitter<RequestEditorEvent> for RequestEditor {}

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

#[cfg(test)]
mod tests {
    use super::*;

    use futures::channel::{mpsc, oneshot};
    use gpui::{TestAppContext, VisualTestContext};
    use indoc::indoc;
    use parking_lot::Mutex;
    use serde_json::json;
    use std::{cell::RefCell, rc::Rc};

    use fs::{Fs, TempFs};
    use http_client::{FakeHttpClient, Response, StatusCode};
    use path::rel_path;
    use settings::SettingsStore;
    use theme::LoadThemes;
    use util_macros::path;
    use workspace::{AppState, DockPosition, Item, Root};
    use worktree::WorktreeModelHandle;

    fn init_test(app_state: Arc<AppState>, cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test_new(cx);
            cx.set_global(settings_store);
            theme::init(LoadThemes::JustBase, cx);
            workspace::init(app_state, cx);
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
            let response_panel = cx.new(|cx| ResponsePanel::new(window, cx));
            workspace.add_panel(response_panel.clone(), DockPosition::Bottom, window, cx);
            response_panel
        });

        (workspace, response_panel, cx)
    }

    #[gpui::test]
    async fn test_send_request_respects_disabled(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let (tx, mut rx) = oneshot::channel();
        let tx = Mutex::new(Some(tx));

        let http_client = FakeHttpClient::create(move |request| {
            assert_eq!(request.uri().path(), "/search");
            assert_eq!(request.uri().query(), Some("query=zaku&test=1"));
            assert_eq!(
                request
                    .headers()
                    .get("Content-Type")
                    .and_then(|value| value.to_str().ok()),
                Some("application/json")
            );
            assert!(request.headers().get("X-Debug").is_none());
            let tx = tx.lock().take().unwrap();

            async move {
                let mut body = request.into_body();
                let mut data = String::new();
                body.read_to_string(&mut data).await.unwrap();
                assert_eq!(
                    data,
                    indoc! {r#"
                        {
                          "hello": "world"
                        }"#}
                );
                tx.send(()).unwrap();

                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .body(AsyncBody::empty())
                    .unwrap())
            }
        });
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), Some(http_client), cx));

        init_test(app_state, cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                "collection": {
                    "request.toml": indoc! {r#"
                        [meta]
                        version = 1

                        [http]
                        method = "POST"
                        url = "https://api.zaku.dev/search"
                        params = [{ name = "query", value = "zaku" }, { name = "debug", value = "1", disabled = true }, { name = "test", value = "1" }]
                        headers = [{ name = "Content-Type", value = "application/json" }, { name = "X-Debug", value = "1", disabled = true }]
                        body = {
                          type = "json",
                          data = """
                        {
                          "hello": "world"
                        }"""
                        }
                    "#}
                }
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let worktree_id = cx.update(|cx| project.read(cx).root_worktree(cx).unwrap().read(cx).id());
        let (workspace, _, cx) = build_workspace(&project, cx);
        let pane = workspace.update_in(cx, |workspace, _, _| workspace.pane().clone());

        let request_path = ProjectPath {
            worktree_id,
            path: Arc::from(rel_path("collection/request.toml")),
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
        cx.run_until_parked();
        assert_eq!(rx.try_recv().unwrap(), Some(()));
    }

    #[gpui::test]
    async fn test_send_request_form_url_encoded(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let (tx, mut rx) = oneshot::channel();
        let tx = Mutex::new(Some(tx));

        let http_client = FakeHttpClient::create(move |request| {
            assert_eq!(
                request
                    .headers()
                    .get("Content-Type")
                    .and_then(|value| value.to_str().ok()),
                Some("application/x-www-form-urlencoded")
            );
            let tx = tx.lock().take().unwrap();

            async move {
                let mut body = request.into_body();
                let mut data = String::new();
                body.read_to_string(&mut data).await.unwrap();
                assert_eq!(
                    data,
                    "foo=bar&foo=+baz&+baz+=the+quick+brown+fox%0Ajumps+over+the+lazy+dog&=bar&baz=%09+&%C3%A9=%09%E6%9D%B1%E4%BA%AC&qux=%2B%26%3D%2520&qux="
                );
                tx.send(()).unwrap();

                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .body(AsyncBody::empty())
                    .unwrap())
            }
        });
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), Some(http_client), cx));

        init_test(app_state, cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                "collection": {
                    "request.toml": indoc! {r#"
                        [meta]
                        version = 1

                        [http]
                        method = "POST"
                        url = "https://api.zaku.dev/form-urlencoded"
                        body = {
                          type = "form-urlencoded",
                          data = [
                            { name = "foo", value = "bar" },
                            { name = "foo", value = " baz" },
                            { name = "bar", value = "qux", disabled = true },
                            {
                              name = " baz ",
                              value = """
                        the quick brown fox
                        jumps over the lazy dog"""
                            },
                            { name = "", value = "bar" },
                            { name = "baz", value = "\t " },
                            { name = "é", value = "\t東京" },
                            { name = "qux", value = "+&=%20" },
                            { name = "qux", value = "" }
                          ]
                        }
                    "#}
                }
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let worktree_id = cx.update(|cx| project.read(cx).root_worktree(cx).unwrap().read(cx).id());
        let (workspace, _, cx) = build_workspace(&project, cx);
        let pane = workspace.update_in(cx, |workspace, _, _| workspace.pane().clone());

        let request_path = ProjectPath {
            worktree_id,
            path: Arc::from(rel_path("collection/request.toml")),
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
        cx.run_until_parked();
        assert_eq!(rx.try_recv().unwrap(), Some(()));
    }

    #[gpui::test]
    async fn test_send_request_empty_form_url_encoded(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let (tx, mut rx) = mpsc::unbounded();

        let http_client = FakeHttpClient::create(move |request| {
            assert_eq!(request.headers().get_all("Content-Type").iter().count(), 1);
            assert_eq!(
                request
                    .headers()
                    .get("Content-Type")
                    .and_then(|value| value.to_str().ok()),
                Some("application/x-www-form-urlencoded")
            );
            let tx = tx.clone();

            async move {
                let mut body = request.into_body();
                let mut data = String::new();
                body.read_to_string(&mut data).await.unwrap();
                tx.unbounded_send(data).unwrap();

                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .body(AsyncBody::empty())
                    .unwrap())
            }
        });
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), Some(http_client), cx));

        init_test(app_state, cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                "collection": {
                    "request.toml": indoc! {r#"
                        [meta]
                        version = 1

                        [http]
                        method = "POST"
                        url = "https://api.zaku.dev/form-urlencoded"
                        body = {
                          type = "form-urlencoded",
                          data = [
                            { name = "", value = "" },
                          ]
                        }
                    "#}
                }
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let worktree_id = cx.update(|cx| project.read(cx).root_worktree(cx).unwrap().read(cx).id());
        let (workspace, _, cx) = build_workspace(&project, cx);
        let pane = workspace.update_in(cx, |workspace, _, _| workspace.pane().clone());

        let request_path = ProjectPath {
            worktree_id,
            path: Arc::from(rel_path("collection/request.toml")),
        };

        let request_editor = workspace
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
        cx.run_until_parked();
        let data = rx.try_recv().unwrap();
        assert_eq!(data, "=");

        request_editor.update_in(cx, |editor, _, _| {
            let RequestEditorState::Ready(request) = &mut editor.request else {
                panic!("expected request editor to be ready");
            };
            request.http.form_url_encoded.clear();
        });
        pane.update_in(cx, |pane, window, cx| {
            pane.send_request(window, cx);
        });
        cx.run_until_parked();
        let data = rx.try_recv().unwrap();
        assert_eq!(data, "");
    }

    #[gpui::test]
    async fn test_send_request_form_url_encoded_content_type(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let (tx, mut rx) = mpsc::unbounded();

        let http_client = FakeHttpClient::create(move |request| {
            assert_eq!(request.headers().get_all("Content-Type").iter().count(), 1);
            let headers = request.headers().clone();
            let tx = tx.clone();

            async move {
                let mut body = request.into_body();
                let mut data = String::new();
                body.read_to_string(&mut data).await.unwrap();
                assert_eq!(data, "foo=bar");
                tx.unbounded_send(headers).unwrap();

                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .body(AsyncBody::empty())
                    .unwrap())
            }
        });
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), Some(http_client), cx));

        init_test(app_state, cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                "collection": {
                    "request.toml": indoc! {r#"
                        [meta]
                        version = 1

                        [http]
                        method = "POST"
                        url = "https://api.zaku.dev/form-urlencoded"
                        headers = [
                          { name = "Content-Type", value = "application/x-www-form-urlencoded; charset=UTF-8" },
                          { name = "content-type", value = "text/plain" },
                          { name = "CONTENT-TYPE", value = "application/json", disabled = true },
                        ]
                        body = {
                          type = "form-urlencoded",
                          data = [
                            { name = "foo", value = "bar" },
                          ]
                        }
                    "#}
                }
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let worktree_id = cx.update(|cx| project.read(cx).root_worktree(cx).unwrap().read(cx).id());
        let (workspace, _, cx) = build_workspace(&project, cx);
        let pane = workspace.update_in(cx, |workspace, _, _| workspace.pane().clone());

        let request_path = ProjectPath {
            worktree_id,
            path: Arc::from(rel_path("collection/request.toml")),
        };

        let request_editor = workspace
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
        cx.run_until_parked();
        let headers = rx.try_recv().unwrap();
        assert_eq!(
            headers
                .get("Content-Type")
                .and_then(|value| value.to_str().ok()),
            Some("text/plain")
        );

        request_editor.update_in(cx, |editor, _, cx| {
            let RequestEditorState::Ready(request) = &mut editor.request else {
                panic!("expected request editor to be ready");
            };
            for header in &mut request.http.headers {
                header.set_disabled(true, cx);
            }
        });
        pane.update_in(cx, |pane, window, cx| {
            pane.send_request(window, cx);
        });
        cx.run_until_parked();
        let headers = rx.try_recv().unwrap();
        assert_eq!(
            headers
                .get("Content-Type")
                .and_then(|value| value.to_str().ok()),
            Some("application/x-www-form-urlencoded")
        );

        request_editor.update_in(cx, |editor, _, _| {
            let RequestEditorState::Ready(request) = &mut editor.request else {
                panic!("expected request editor to be ready");
            };
            request.http.headers.clear();
        });
        pane.update_in(cx, |pane, window, cx| {
            pane.send_request(window, cx);
        });
        cx.run_until_parked();
        let headers = rx.try_recv().unwrap();
        assert_eq!(
            headers
                .get("Content-Type")
                .and_then(|value| value.to_str().ok()),
            Some("application/x-www-form-urlencoded")
        );
    }

    #[gpui::test]
    async fn test_save_from_request_editor(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));

        init_test(app_state, cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                "collection": {
                    "request.toml": indoc! {r#"
                        [meta]
                        version = 1

                        [http]
                        method = "GET"
                        url = "https://api.zaku.dev/me"
                        params = [
                            { name = "query", value = "zaku" },
                            { name = "debug", value = "1", disabled = true },
                            { name = "test", value = "1", disabled = false },
                        ]
                        headers = [
                            { name = "Content-Type", value = "application/json" },
                            { name = "X-Debug", value = "1", disabled = true },
                        ]
                    "#}
                }
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let worktree = cx.update(|cx| project.read(cx).root_worktree(cx).unwrap());
        let worktree_id = worktree.read_with(cx, |worktree, _| worktree.id());
        let (workspace, _, cx) = build_workspace(&project, cx);

        let request_path = ProjectPath {
            worktree_id,
            path: Arc::from(rel_path("collection/request.toml")),
        };

        let request_editor = workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.open_path(request_path.clone(), None, true, window, cx)
            })
            .await
            .unwrap()
            .downcast::<RequestEditor>()
            .unwrap();

        request_editor.update_in(cx, |editor, window, cx| {
            let RequestEditorState::Ready(request) = &mut editor.request else {
                panic!("Expected request editor to be ready");
            };
            request.http.url.update(cx, |field, cx| {
                field.set_value("https://api.zaku.dev/me/edit", window, cx);
            });
            editor.mark_edited(cx);
        });

        assert!(request_editor.read_with(cx, |editor, cx| { editor.is_dirty(cx) }));

        workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.save_active_item(actions::pane::SaveIntent::Save, window, cx)
            })
            .await
            .unwrap();
        worktree.flush_fs_events(cx).await;

        assert!(!request_editor.read_with(cx, |editor, cx| { editor.is_dirty(cx) }));

        let saved = temp_fs
            .load("project/collection/request.toml".as_ref())
            .await
            .unwrap();
        let saved_request = toml::from_str::<RequestFile>(&saved).unwrap();
        let expected_request = RequestFile {
            meta: RequestFileMeta { version: 1 },
            http: RequestFileHttp {
                method: "GET".to_string(),
                url: "https://api.zaku.dev/me/edit".to_string(),
                params: vec![
                    RequestFileParam {
                        name: "query".to_string(),
                        value: "zaku".to_string(),
                        disabled: false,
                    },
                    RequestFileParam {
                        name: "debug".to_string(),
                        value: "1".to_string(),
                        disabled: true,
                    },
                    RequestFileParam {
                        name: "test".to_string(),
                        value: "1".to_string(),
                        disabled: false,
                    },
                ],
                headers: vec![
                    RequestFileHeader {
                        name: "Content-Type".to_string(),
                        value: "application/json".to_string(),
                        disabled: false,
                    },
                    RequestFileHeader {
                        name: "X-Debug".to_string(),
                        value: "1".to_string(),
                        disabled: true,
                    },
                ],
                body: None,
            },
        };

        assert_eq!(saved_request, expected_request);
        assert_eq!(
            project.read_with(cx, |project, cx| {
                project
                    .entry_for_path(&request_path, cx)
                    .map(|entry| entry.is_request)
            }),
            Some(true)
        );
    }

    #[gpui::test]
    async fn test_file_handle_changed_on_rename(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));

        init_test(app_state, cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                "collection": {
                    "request.toml": indoc! {r#"
                        [meta]
                        version = 1

                        [http]
                        method = "GET"
                        url = "https://api.zaku.dev/me"
                    "#}
                }
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let worktree_id = cx.update(|cx| project.read(cx).root_worktree(cx).unwrap().read(cx).id());
        let (workspace, _, cx) = build_workspace(&project, cx);

        let request_editor = workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.open_path(
                    (worktree_id, rel_path("collection/request.toml")).into(),
                    None,
                    true,
                    window,
                    cx,
                )
            })
            .await
            .unwrap()
            .downcast::<RequestEditor>()
            .unwrap();

        let buffer = request_editor.read_with(cx, |editor, _| editor.buffer.clone());
        let received_file_handle_changed = Rc::new(RefCell::new(false));
        buffer.update(cx, |_, cx| {
            let received_file_handle_changed = received_file_handle_changed.clone();
            cx.subscribe(&buffer, move |_, _, event, _| {
                if matches!(event, RequestBufferEvent::FileHandleChanged) {
                    *received_file_handle_changed.borrow_mut() = true;
                }
            })
            .detach();
        });
        cx.run_until_parked();

        let entry_id = project
            .read_with(cx, |project, cx| {
                project
                    .entry_for_path(
                        &(worktree_id, rel_path("collection/request.toml")).into(),
                        cx,
                    )
                    .map(|entry| entry.id)
            })
            .unwrap();
        project
            .update(cx, |project, cx| {
                project.rename_entry(
                    entry_id,
                    (worktree_id, rel_path("collection/renamed.toml")).into(),
                    cx,
                )
            })
            .await
            .unwrap();
        cx.run_until_parked();

        assert!(
            *received_file_handle_changed.borrow(),
            "RequestBufferEvent::FileHandleChanged must be emitted when the open request is renamed"
        );
        assert_eq!(
            request_editor.read_with(cx, |editor, cx| editor.project_path(cx)),
            Some((worktree_id, rel_path("collection/renamed.toml")).into())
        );
        buffer.read_with(cx, |buffer, _| {
            assert_eq!(
                buffer.file().path.as_ref(),
                rel_path("collection/renamed.toml")
            );
        });
        assert_eq!(
            request_editor.read_with(cx, |editor, cx| editor.title(cx).to_string()),
            "renamed"
        );
    }
}
