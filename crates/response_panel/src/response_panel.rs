use gpui::{
    Action, AnyElement, App, Context, DefiniteLength, ElementId, Entity, FocusHandle, Focusable,
    FontWeight, ListAlignment, ListState, Pixels, Render, SharedString, Subscription, WeakEntity,
    Window, prelude::*,
};
use num_traits::ToPrimitive;
use std::{rc::Rc, sync::Arc, time::Duration};

use editor::Editor;
use http_client::StatusCode;
use language::{Buffer, Language, PLAIN_TEXT};
use multi_buffer::MultiBuffer;
use theme::ActiveTheme;
use ui::{
    Color, ColumnWidthConfig, DynamicSpacing, IconName, KeyBinding, Label, LabelCommon, LabelSize,
    LineHeightStyle, ScrollAxes, Scrollbars, Table, TableCell, TableInteractionState, TextSize,
};
use workspace::{Panel, Workspace, pane::Pane};

const NAME_COLUMN_INDEX: usize = 0;
const VALUE_COLUMN_INDEX: usize = 1;

pub fn init(cx: &mut App) {
    cx.observe_new(
        |workspace: &mut Workspace, _window, _: &mut Context<Workspace>| {
            workspace.register_action(
                |workspace, _: &actions::response_panel::ToggleFocus, window, cx| {
                    if let Some(response_panel) = workspace.panel::<ResponsePanel>(cx) {
                        response_panel.update(cx, |response_panel, _| {
                            response_panel.take_auto_hidden();
                        });
                    }
                    workspace.toggle_panel_focus::<ResponsePanel>(window, cx);
                },
            );
        },
    )
    .detach();
}

fn format_bytes_received(bytes_received: u64) -> SharedString {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    const DECIMAL_BYTE_UNIT: f64 = 1000.0;

    let mut value = bytes_received
        .to_f64()
        .expect("bytes received should fit in f64");
    let mut unit_index = 0;

    while value >= DECIMAL_BYTE_UNIT && unit_index < UNITS.len() - 1 {
        value /= DECIMAL_BYTE_UNIT;
        unit_index += 1;
    }

    let unit = UNITS
        .get(unit_index)
        .expect("bytes received unit should exist");
    if unit_index == 0 {
        format!("{bytes_received} {unit}").into()
    } else {
        format!("{value:.2} {unit}").into()
    }
}

fn format_elapsed_duration(elapsed_duration: Duration) -> SharedString {
    let total_seconds = elapsed_duration.as_secs();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = elapsed_duration.as_secs_f64() % 60.0;

    if elapsed_duration.as_millis() < 1000 {
        format!("{} ms", elapsed_duration.as_millis())
    } else if hours == 0 && minutes == 0 {
        format!("{:.2} s", elapsed_duration.as_secs_f64())
    } else if hours == 0 {
        format!("{minutes} m {seconds:.2} s")
    } else {
        format!("{hours} h {minutes} m {seconds:.2} s")
    }
    .into()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ResponsePanelTab {
    Body,
    Headers,
    Cookies,
}

#[derive(Clone)]
struct ResponseSummary {
    label: SharedString,
    elapsed_duration: SharedString,
    bytes_received: SharedString,
}

#[derive(Clone)]
pub struct ResponseHeader {
    name: SharedString,
    value: SharedString,
}

impl ResponseHeader {
    pub fn new(name: impl Into<SharedString>, value: impl Into<SharedString>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

#[derive(Clone)]
pub struct ResponseCookie {
    name: SharedString,
    value: SharedString,
    domain: Option<SharedString>,
    path: Option<SharedString>,
    expires: Option<SharedString>,
    max_age: Option<SharedString>,
    secure: Option<bool>,
    http_only: Option<bool>,
    same_site: Option<SharedString>,
}

impl ResponseCookie {
    pub fn new(name: impl Into<SharedString>, value: impl Into<SharedString>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            domain: None,
            path: None,
            expires: None,
            max_age: None,
            secure: None,
            http_only: None,
            same_site: None,
        }
    }

    pub fn domain(mut self, domain: Option<impl Into<SharedString>>) -> Self {
        self.domain = domain.map(Into::into);
        self
    }

    pub fn path(mut self, path: Option<impl Into<SharedString>>) -> Self {
        self.path = path.map(Into::into);
        self
    }

    pub fn expires(mut self, expires: Option<impl Into<SharedString>>) -> Self {
        self.expires = expires.map(Into::into);
        self
    }

    pub fn max_age(mut self, max_age: Option<impl Into<SharedString>>) -> Self {
        self.max_age = max_age.map(Into::into);
        self
    }

    pub fn secure(mut self, secure: Option<bool>) -> Self {
        self.secure = secure;
        self
    }

    pub fn http_only(mut self, http_only: Option<bool>) -> Self {
        self.http_only = http_only;
        self
    }

    pub fn same_site(mut self, same_site: Option<impl Into<SharedString>>) -> Self {
        self.same_site = same_site.map(Into::into);
        self
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CookieTableRowKind {
    Header,
    Attribute,
}

#[derive(Clone)]
struct CookieTableRow {
    kind: CookieTableRowKind,
    name: SharedString,
    value: SharedString,
}

#[derive(Clone, Default)]
pub enum ResponseState {
    #[default]
    Idle,
    Fetching {
        bytes_received: u64,
        elapsed_duration: Duration,
    },
    Completed {
        status_code: StatusCode,
        bytes_received: u64,
        elapsed_duration: Duration,
    },
    Error {
        bytes_received: u64,
        elapsed_duration: Duration,
    },
}

impl ResponseState {
    fn summary(&self) -> Option<ResponseSummary> {
        match self {
            ResponseState::Idle => None,
            ResponseState::Fetching {
                bytes_received,
                elapsed_duration,
            } => Some(ResponseSummary {
                label: "Fetching".into(),
                elapsed_duration: format_elapsed_duration(*elapsed_duration),
                bytes_received: format_bytes_received(*bytes_received),
            }),
            ResponseState::Completed {
                status_code,
                bytes_received,
                elapsed_duration,
            } => {
                let label = if let Some(reason_phrase) = status_code.canonical_reason() {
                    format!("{} {reason_phrase}", status_code.as_u16()).into()
                } else {
                    status_code.as_u16().to_string().into()
                };

                Some(ResponseSummary {
                    label,
                    elapsed_duration: format_elapsed_duration(*elapsed_duration),
                    bytes_received: format_bytes_received(*bytes_received),
                })
            }
            ResponseState::Error {
                bytes_received,
                elapsed_duration,
            } => Some(ResponseSummary {
                label: "Error".into(),
                elapsed_duration: format_elapsed_duration(*elapsed_duration),
                bytes_received: format_bytes_received(*bytes_received),
            }),
        }
    }
}

pub struct Response {
    request_id: usize,
    state: ResponseState,
    editor: Entity<Editor>,
    payload: Entity<MultiBuffer>,
    headers: Vec<ResponseHeader>,
    headers_table: Entity<TableInteractionState>,
    headers_list_state: ListState,
    cookies: Vec<ResponseCookie>,
    cookies_table: Entity<TableInteractionState>,
    cookies_list_state: ListState,
}

impl Response {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (editor, payload) = Self::new_editor(window, cx);
        let response_id = cx.entity_id();
        let headers_table = cx.new(move |cx| {
            TableInteractionState::new(cx).with_custom_scrollbar(
                Scrollbars::new(ScrollAxes::Vertical)
                    .id(("response-headers-scrollbar", response_id)),
            )
        });
        let headers_list_state = ListState::new(0, ListAlignment::Top, gpui::px(1.0)).measure_all();
        let cookies_table = cx.new(move |cx| {
            TableInteractionState::new(cx).with_custom_scrollbar(
                Scrollbars::new(ScrollAxes::Vertical)
                    .id(("response-cookies-scrollbar", response_id)),
            )
        });
        let cookies_list_state = ListState::new(0, ListAlignment::Top, gpui::px(1.0)).measure_all();

        Self {
            request_id: 0,
            state: ResponseState::default(),
            editor,
            payload,
            headers: Vec::new(),
            headers_table,
            headers_list_state,
            cookies: Vec::new(),
            cookies_table,
            cookies_list_state,
        }
    }

    fn new_editor(window: &mut Window, cx: &mut App) -> (Entity<Editor>, Entity<MultiBuffer>) {
        let payload = cx.new(move |cx| {
            let buffer = cx.new(|cx| Buffer::local("", cx).with_language(PLAIN_TEXT.clone(), cx));
            MultiBuffer::singleton(buffer, cx)
        });
        let editor = cx.new(|cx| {
            let mut editor = Editor::for_multibuffer(payload.clone(), window, cx);
            editor.set_read_only(true);
            editor
        });

        (editor, payload)
    }

    fn summary(&self) -> Option<ResponseSummary> {
        self.state.summary()
    }

    fn editor(&self) -> Entity<Editor> {
        self.editor.clone()
    }

    fn headers(&self) -> &[ResponseHeader] {
        &self.headers
    }

    fn sync_headers_row_count(&self) {
        let headers_row_count = self.headers.len();
        if self.headers_list_state.item_count() != headers_row_count {
            self.headers_list_state.reset(headers_row_count);
        }
    }

    fn clear_headers_text_selection(&self, cx: &mut App) {
        self.headers_table.update(cx, |table, cx| {
            table.clear_text_selection();
            cx.notify();
        });
    }

    pub fn set_headers(
        &mut self,
        request_id: usize,
        headers: Vec<ResponseHeader>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.request_id != request_id {
            return false;
        }

        self.headers = headers;
        self.sync_headers_row_count();
        self.clear_headers_text_selection(cx);
        cx.notify();
        true
    }

    fn cookies(&self) -> &[ResponseCookie] {
        &self.cookies
    }

    fn cookies_row_count(&self) -> usize {
        self.cookies
            .iter()
            .map(|cookie| {
                let attribute_row_count = [
                    cookie.domain.is_some(),
                    cookie.path.is_some(),
                    cookie.expires.is_some(),
                    cookie.max_age.is_some(),
                    cookie.secure.is_some(),
                    cookie.http_only.is_some(),
                    cookie.same_site.is_some(),
                ]
                .into_iter()
                .filter(|has_attribute| *has_attribute)
                .count();

                1 + attribute_row_count
            })
            .sum()
    }

    fn sync_cookies_row_count(&self) {
        let cookies_row_count = self.cookies_row_count();
        if self.cookies_list_state.item_count() != cookies_row_count {
            self.cookies_list_state.reset(cookies_row_count);
        }
    }

    fn clear_cookies_text_selection(&self, cx: &mut App) {
        self.cookies_table.update(cx, |table, cx| {
            table.clear_text_selection();
            cx.notify();
        });
    }

    pub fn set_cookies(
        &mut self,
        request_id: usize,
        cookies: Vec<ResponseCookie>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.request_id != request_id {
            return false;
        }

        self.cookies = cookies;
        self.sync_cookies_row_count();
        self.clear_cookies_text_selection(cx);
        cx.notify();
        true
    }

    pub fn text(&self, cx: &App) -> String {
        self.payload.read(cx).snapshot(cx).text()
    }

    pub fn begin_response(&mut self, window: &mut Window, cx: &mut Context<Self>) -> usize {
        let was_focused = self.editor.focus_handle(cx).contains_focused(window, cx);
        let (editor, payload) = Self::new_editor(window, cx);
        let request_id = self.request_id.wrapping_add(1);

        self.request_id = request_id;
        self.state = ResponseState::default();
        self.headers.clear();
        self.cookies.clear();
        self.headers_list_state.reset(0);
        self.cookies_list_state.reset(0);
        self.clear_headers_text_selection(cx);
        self.clear_cookies_text_selection(cx);
        self.editor = editor;
        self.payload = payload;
        if was_focused {
            let focus_handle = self.editor.focus_handle(cx);
            window.focus(&focus_handle, cx);
        }
        cx.notify();
        request_id
    }

    pub fn state(&self) -> &ResponseState {
        &self.state
    }

    pub fn set_state(
        &mut self,
        request_id: usize,
        state: ResponseState,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.request_id != request_id {
            return false;
        }

        self.state = state;
        cx.notify();
        true
    }

    pub fn set_payload<T: Into<String>>(
        &mut self,
        request_id: usize,
        payload: T,
        language: Option<Arc<Language>>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.request_id != request_id {
            return false;
        }

        let language = language.unwrap_or_else(|| PLAIN_TEXT.clone());
        self.payload.update(cx, |payload_buffer, cx| {
            let payload = payload.into();
            if let Some(buffer) = payload_buffer.as_singleton() {
                let language = language.clone();
                buffer.update(cx, |buffer, cx| {
                    buffer.set_language(Some(language), cx);
                });
            }
            payload_buffer.set_text(payload, cx);
        });
        cx.notify();
        true
    }
}

pub struct ResponsePanel {
    focus_handle: FocusHandle,
    pane: WeakEntity<Pane>,
    response: Option<Entity<Response>>,
    active_tab: ResponsePanelTab,
    on_active_tab_change: Option<Rc<dyn Fn(ResponsePanelTab, &mut Context<ResponsePanel>)>>,
    has_response_context: bool,
    auto_hidden: bool,
    response_subscription: Option<Subscription>,
    _focus_subscription: Subscription,
}

impl ResponsePanel {
    const PANEL_KEY: &str = "ResponsePanel";
    const DEFAULT_SIZE: Pixels = gpui::px(440.0);

    pub fn new(pane: WeakEntity<Pane>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let focus_subscription = cx.on_focus(&focus_handle, window, |_, window, cx| {
            cx.on_next_frame(window, |response_panel, window, cx| {
                let editor = response_panel.response.as_ref().and_then(|response| {
                    let response = response.read(cx);
                    match response.state() {
                        ResponseState::Idle => None,
                        ResponseState::Fetching { .. }
                        | ResponseState::Completed { .. }
                        | ResponseState::Error { .. } => Some(response.editor()),
                    }
                });

                if response_panel.focus_handle.is_focused(window)
                    && response_panel.active_tab() == ResponsePanelTab::Body
                    && let Some(editor) = editor
                {
                    editor.focus_handle(cx).focus(window, cx);
                }
            });
        });

        Self {
            focus_handle,
            pane,
            response: None,
            active_tab: ResponsePanelTab::Body,
            on_active_tab_change: None,
            has_response_context: false,
            auto_hidden: false,
            response_subscription: None,
            _focus_subscription: focus_subscription,
        }
    }

    #[cfg(any(test, feature = "test"))]
    pub fn headers_list_state(&self, cx: &App) -> Option<ListState> {
        self.response
            .as_ref()
            .map(|response| response.read(cx).headers_list_state.clone())
    }

    #[cfg(any(test, feature = "test"))]
    pub fn cookies_list_state(&self, cx: &App) -> Option<ListState> {
        self.response
            .as_ref()
            .map(|response| response.read(cx).cookies_list_state.clone())
    }

    fn active_tab(&self) -> ResponsePanelTab {
        self.active_tab
    }

    pub fn mark_auto_hidden(&mut self) {
        self.auto_hidden = true;
    }

    pub fn take_auto_hidden(&mut self) -> bool {
        let auto_hidden = self.auto_hidden;
        self.auto_hidden = false;
        auto_hidden
    }

    fn set_active_tab(&mut self, active_tab: ResponsePanelTab, cx: &mut Context<Self>) {
        if self.active_tab == active_tab {
            return;
        }

        self.active_tab = active_tab;
        if let Some(on_active_tab_change) = self.on_active_tab_change.clone() {
            on_active_tab_change(active_tab, cx);
        }
        if let Some(response) = self.response.clone() {
            response.update(cx, |response, cx| {
                response.clear_headers_text_selection(cx);
                response.clear_cookies_text_selection(cx);
            });
        }
        cx.notify();
    }

    fn cookie_table_rows(cookies: &[ResponseCookie]) -> Vec<CookieTableRow> {
        let mut rows = Vec::new();
        for cookie in cookies {
            rows.push(CookieTableRow {
                kind: CookieTableRowKind::Header,
                name: cookie.name.clone(),
                value: cookie.value.clone(),
            });

            for (attribute_name, attribute_value) in [
                ("Domain", cookie.domain.clone()),
                ("Path", cookie.path.clone()),
                ("Expires", cookie.expires.clone()),
                ("Max-Age", cookie.max_age.clone()),
                (
                    "Secure",
                    cookie
                        .secure
                        .map(|secure| SharedString::from(secure.to_string())),
                ),
                (
                    "HttpOnly",
                    cookie
                        .http_only
                        .map(|http_only| SharedString::from(http_only.to_string())),
                ),
                ("SameSite", cookie.same_site.clone()),
            ]
            .into_iter()
            .filter_map(|(attribute_name, attribute_value)| {
                attribute_value.map(|attribute_value| (attribute_name, attribute_value))
            }) {
                rows.push(CookieTableRow {
                    kind: CookieTableRowKind::Attribute,
                    name: attribute_name.into(),
                    value: attribute_value,
                });
            }
        }

        rows
    }

    pub fn set_response(
        &mut self,
        response: Option<Entity<Response>>,
        active_tab: ResponsePanelTab,
        on_active_tab_change: Option<Rc<dyn Fn(ResponsePanelTab, &mut Context<ResponsePanel>)>>,
        has_response_context: bool,
        cx: &mut Context<Self>,
    ) {
        self.on_active_tab_change = on_active_tab_change;
        let response_context_changed = self.has_response_context != has_response_context;
        self.has_response_context = has_response_context;

        let response_changed = match (&self.response, &response) {
            (Some(old_response), Some(new_response)) => old_response != new_response,
            (None, None) => false,
            _ => true,
        };
        let active_tab_changed = self.active_tab != active_tab;

        if response_changed {
            let _previous_subscription = self.response_subscription.take();
            self.response_subscription = response.as_ref().map(|response| {
                cx.observe(response, |_, _, cx| {
                    cx.notify();
                })
            });
            self.response = response;
        }

        if response_changed || active_tab_changed || response_context_changed {
            self.active_tab = active_tab;
            if let Some(response) = self.response.clone() {
                response.update(cx, |response, cx| {
                    response.clear_headers_text_selection(cx);
                    response.clear_cookies_text_selection(cx);
                });
            }
            cx.notify();
        }
    }

    pub fn text(&self, cx: &App) -> String {
        self.response
            .as_ref()
            .map_or_else(String::new, |response| response.read(cx).text(cx))
    }

    fn render_send_request_hint(&self, cx: &App) -> AnyElement {
        gpui::div()
            .flex_1()
            .min_h_0()
            .flex()
            .items_center()
            .justify_center()
            .gap_1()
            .child(
                Label::new("Send Request")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(KeyBinding::for_action_in(
                &actions::workspace::SendRequest,
                &self.focus_handle,
                cx,
            ))
            .into_any_element()
    }

    fn render_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(response) = self.response.as_ref() else {
            return self.render_send_request_hint(cx);
        };

        let colors = cx.theme().colors();
        let editor = {
            let response = response.read(cx);
            match response.state() {
                ResponseState::Idle => return self.render_send_request_hint(cx),
                ResponseState::Fetching { .. }
                | ResponseState::Completed { .. }
                | ResponseState::Error { .. } => response.editor(),
            }
        };

        gpui::div()
            .flex_1()
            .min_h_0()
            .bg(colors.panel_background)
            .child(editor)
            .into_any_element()
    }

    fn render_headers(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(response) = self.response.as_ref() else {
            return self.render_send_request_hint(cx);
        };

        let (is_fetching, headers, headers_table, headers_list_state) = {
            let response = response.read(cx);
            let is_fetching = match response.state() {
                ResponseState::Idle => return self.render_send_request_hint(cx),
                ResponseState::Fetching { .. } => true,
                ResponseState::Completed { .. } | ResponseState::Error { .. } => false,
            };

            (
                is_fetching,
                response.headers().to_vec(),
                response.headers_table.clone(),
                response.headers_list_state.clone(),
            )
        };

        if headers.is_empty() {
            let empty_content = gpui::div()
                .flex_1()
                .min_h_0()
                .flex()
                .items_center()
                .justify_center();
            if is_fetching {
                return empty_content.into_any_element();
            }

            return empty_content
                .child(
                    Label::new("No headers received.")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .into_any_element();
        }

        let headers = Rc::new(headers);
        let row_count = headers.len();
        let column_count = 2;
        let headers_for_text = headers.clone();
        let headers_for_rows = headers.clone();
        let table = Table::new(column_count)
            .interactable(&headers_table)
            .width_config(ColumnWidthConfig::explicit(vec![
                DefiniteLength::Fraction(0.24),
                DefiniteLength::Fraction(0.76),
            ]))
            .disable_base_style()
            .hide_row_hover()
            .text_for_selection(move |row_index, column_index, _, _| {
                let header = headers_for_text.get(row_index)?;
                match column_index {
                    NAME_COLUMN_INDEX => Some(header.name.clone()),
                    VALUE_COLUMN_INDEX => Some(header.value.clone()),
                    _ => None,
                }
            })
            .variable_row_height_list(row_count, headers_list_state, {
                move |header_index, _, _| {
                    let header = headers_for_rows
                        .get(header_index)
                        .expect("response header row should exist");

                    vec![
                        TableCell::text(header.name.clone())
                            .size(TextSize::Small)
                            .color(Color::Accent)
                            .alpha(0.85),
                        TableCell::text(header.value.clone())
                            .size(TextSize::Small)
                            .color(Color::Default),
                    ]
                }
            });

        gpui::div()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(table)
            .into_any_element()
    }

    fn render_cookies(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(response) = self.response.as_ref() else {
            return self.render_send_request_hint(cx);
        };

        let (is_fetching, cookies, cookies_table, cookies_list_state) = {
            let response = response.read(cx);
            let is_fetching = match response.state() {
                ResponseState::Idle => return self.render_send_request_hint(cx),
                ResponseState::Fetching { .. } => true,
                ResponseState::Completed { .. } | ResponseState::Error { .. } => false,
            };

            (
                is_fetching,
                response.cookies().to_vec(),
                response.cookies_table.clone(),
                response.cookies_list_state.clone(),
            )
        };

        if cookies.is_empty() {
            let empty_content = gpui::div()
                .flex_1()
                .min_h_0()
                .flex()
                .items_center()
                .justify_center();
            if is_fetching {
                return empty_content.into_any_element();
            }

            return empty_content
                .child(
                    Label::new("No cookies received.")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .into_any_element();
        }

        let rows = Rc::new(Self::cookie_table_rows(&cookies));
        let cookie_header_background = cx.theme().colors().panel_tab_bar_background;
        let row_count = rows.len();
        let column_count = 2;
        let rows_for_style = rows.clone();
        let rows_for_text = rows.clone();
        let rows_for_render = rows.clone();
        let table = Table::new(column_count)
            .interactable(&cookies_table)
            .width_config(ColumnWidthConfig::explicit(vec![
                DefiniteLength::Fraction(0.24),
                DefiniteLength::Fraction(0.76),
            ]))
            .disable_base_style()
            .hide_row_hover()
            .map_row(move |(row_index, row), _, _| {
                if rows_for_style
                    .get(row_index)
                    .expect("response cookie row should exist")
                    .kind
                    == CookieTableRowKind::Header
                {
                    row.bg(cookie_header_background).py_0p5().into_any_element()
                } else {
                    row.into_any_element()
                }
            })
            .text_for_selection(move |row_index, column_index, _, _| {
                let row = rows_for_text.get(row_index)?;
                match column_index {
                    NAME_COLUMN_INDEX => Some(row.name.clone()),
                    VALUE_COLUMN_INDEX => Some(row.value.clone()),
                    _ => None,
                }
            })
            .variable_row_height_list(row_count, cookies_list_state, {
                move |row_index, _, _| {
                    let row = rows_for_render
                        .get(row_index)
                        .expect("response cookie row should exist");

                    vec![
                        TableCell::text(row.name.clone())
                            .size(TextSize::Small)
                            .color(Color::Accent)
                            .alpha(0.85)
                            .when(row.kind == CookieTableRowKind::Header, |this| {
                                this.weight(FontWeight::MEDIUM)
                            }),
                        TableCell::text(row.value.clone())
                            .size(TextSize::Small)
                            .color(Color::Default),
                    ]
                }
            });

        gpui::div()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(table)
            .into_any_element()
    }

    fn render_response_summary(response_summary: ResponseSummary, cx: &App) -> impl IntoElement {
        gpui::div()
            .flex()
            .items_center()
            .gap_1()
            .child(
                gpui::div()
                    .min_w(DynamicSpacing::Base40.px(cx))
                    .flex()
                    .justify_center()
                    .items_center()
                    .child(
                        Label::new(response_summary.label)
                            .size(LabelSize::Small)
                            .color(Color::Muted)
                            .single_line(),
                    ),
            )
            .child(Label::new("·").size(LabelSize::Small).color(Color::Muted))
            .child(
                gpui::div()
                    .min_w(DynamicSpacing::Base40.px(cx))
                    .flex()
                    .justify_center()
                    .items_center()
                    .child(
                        Label::new(response_summary.elapsed_duration)
                            .size(LabelSize::Small)
                            .color(Color::Muted)
                            .single_line(),
                    ),
            )
            .child(Label::new("·").size(LabelSize::Small).color(Color::Muted))
            .child(
                gpui::div()
                    .min_w(DynamicSpacing::Base40.px(cx))
                    .flex()
                    .justify_center()
                    .items_center()
                    .child(
                        Label::new(response_summary.bytes_received)
                            .size(LabelSize::Small)
                            .color(Color::Muted)
                            .single_line(),
                    ),
            )
    }

    fn render_tab_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let active_tab = self.active_tab();
        let response_summary = self
            .response
            .as_ref()
            .and_then(|response| response.read(cx).summary());
        let colors = cx.theme().colors();

        let render_tab =
            |id: ElementId, active: bool, label: SharedString, set_active_tab: ResponsePanelTab| {
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
                    .on_click(cx.listener(move |response_panel, _, _, cx| {
                        cx.stop_propagation();
                        response_panel.set_active_tab(set_active_tab, cx);
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
                                Label::new(label)
                                    .size(LabelSize::Small)
                                    .line_height_style(LineHeightStyle::UiLabel)
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
            .id("response-panel-tabs")
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .h(DynamicSpacing::Base36.px(cx))
            .border_b_1()
            .border_color(colors.border)
            .bg(colors.panel_tab_bar_background)
            .child(
                gpui::div()
                    .flex()
                    .items_center()
                    .h_full()
                    .px_1()
                    .child(render_tab(
                        ElementId::Name("response-body-tab".into()),
                        active_tab == ResponsePanelTab::Body,
                        "Body".into(),
                        ResponsePanelTab::Body,
                    ))
                    .child(render_tab(
                        ElementId::Name("response-headers-tab".into()),
                        active_tab == ResponsePanelTab::Headers,
                        "Headers".into(),
                        ResponsePanelTab::Headers,
                    ))
                    .child(render_tab(
                        ElementId::Name("response-cookies-tab".into()),
                        active_tab == ResponsePanelTab::Cookies,
                        "Cookies".into(),
                        ResponsePanelTab::Cookies,
                    )),
            )
            .when_some(response_summary, |this, response_summary| {
                this.pr_3()
                    .child(Self::render_response_summary(response_summary, cx))
            })
            .into_any_element()
    }
}

impl Focusable for ResponsePanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for ResponsePanel {
    fn persistent_name() -> &'static str {
        Self::PANEL_KEY
    }

    fn panel_key() -> &'static str {
        Self::PANEL_KEY
    }

    fn default_size(&self, _window: &Window, _: &App) -> Pixels {
        Self::DEFAULT_SIZE
    }

    fn icon(&self, _window: &Window, _: &App) -> Option<ui::IconName> {
        Some(IconName::Network)
    }

    fn icon_tooltip(&self, _window: &Window, _: &App) -> Option<&'static str> {
        Some("Response Panel")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        actions::response_panel::ToggleFocus.boxed_clone()
    }

    fn activation_priority(&self) -> u32 {
        2
    }

    fn enabled(&self, cx: &App) -> bool {
        self.pane
            .upgrade()
            .is_some_and(|pane| !pane.read(cx).should_display_welcome_page())
    }
}

impl Render for ResponsePanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus_handle = self.focus_handle(cx);
        let tab_bar = self.has_response_context.then(|| self.render_tab_bar(cx));
        let tab_content = if self.has_response_context {
            match self.active_tab() {
                ResponsePanelTab::Body => self.render_body(cx),
                ResponsePanelTab::Headers => self.render_headers(cx),
                ResponsePanelTab::Cookies => self.render_cookies(cx),
            }
        } else {
            gpui::div()
                .flex_1()
                .min_h_0()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Label::new("No response available.")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .into_any_element()
        };
        let colors = cx.theme().colors();

        gpui::div()
            .track_focus(&focus_handle)
            .key_context(Self::PANEL_KEY)
            .flex()
            .flex_col()
            .size_full()
            .bg(colors.panel_background)
            .when_some(tab_bar, |this, tab_bar| this.child(tab_bar))
            .child(tab_content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Duration;

    #[test]
    fn test_format_bytes_received() {
        assert_eq!(format_bytes_received(0).to_string(), "0 B");
        assert_eq!(format_bytes_received(999).to_string(), "999 B");
        assert_eq!(format_bytes_received(1000).to_string(), "1.00 KB");
        assert_eq!(format_bytes_received(1545).to_string(), "1.54 KB");
        assert_eq!(format_bytes_received(1_047_100).to_string(), "1.05 MB");
        assert_eq!(format_bytes_received(1_384_900_000).to_string(), "1.38 GB");
    }

    #[test]
    fn test_format_elapsed_duration() {
        assert_eq!(
            format_elapsed_duration(Duration::from_millis(570)).to_string(),
            "570 ms"
        );
        assert_eq!(
            format_elapsed_duration(Duration::from_millis(999)).to_string(),
            "999 ms"
        );
        assert_eq!(
            format_elapsed_duration(Duration::from_secs(1)).to_string(),
            "1.00 s"
        );
        assert_eq!(
            format_elapsed_duration(Duration::from_millis(3865)).to_string(),
            "3.87 s"
        );
        assert_eq!(
            format_elapsed_duration(Duration::from_millis(63_430)).to_string(),
            "1 m 3.43 s"
        );
        assert_eq!(
            format_elapsed_duration(Duration::from_millis(3_723_430)).to_string(),
            "1 h 2 m 3.43 s"
        );
    }
}
