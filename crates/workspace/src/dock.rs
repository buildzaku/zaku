use gpui::{
    Action, AnyView, App, Axis, Context, Entity, EntityId, FocusHandle, Focusable, KeyContext,
    MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Render, Styled, WeakEntity, Window,
    prelude::*,
};
use std::sync::Arc;

use theme::ActiveTheme;
use ui::{
    ButtonCommon, ButtonShape, ButtonSize, Clickable, Color, IconButton, IconName,
    StyledTypography, Tooltip,
};

use crate::{DockPosition, DraggedDock, Workspace, status_bar::StatusItemView};

pub(crate) const RESIZE_HANDLE_SIZE: Pixels = gpui::px(6.);

pub trait Panel: Focusable + Render + Sized {
    fn persistent_name() -> &'static str;
    fn position(&self, window: &Window, cx: &App) -> DockPosition;
    fn position_is_valid(&self, position: DockPosition) -> bool;
    fn set_position(&mut self, position: DockPosition, window: &mut Window, cx: &mut Context<Self>);
    fn size(&self, window: &Window, cx: &App) -> Pixels;
    fn set_size(&mut self, size: Option<Pixels>, window: &mut Window, cx: &mut Context<Self>);
    fn icon(&self, window: &Window, cx: &App) -> Option<IconName>;
    fn icon_tooltip(&self, window: &Window, cx: &App) -> Option<&'static str>;
    fn toggle_action(&self) -> Box<dyn Action>;
}

pub trait PanelHandle: Send + Sync {
    fn panel_id(&self) -> EntityId;
    fn persistent_name(&self) -> &'static str;
    fn position(&self, window: &Window, cx: &App) -> DockPosition;
    fn position_is_valid(&self, position: DockPosition, cx: &App) -> bool;
    fn set_position(&self, position: DockPosition, window: &mut Window, cx: &mut App);
    fn size(&self, window: &Window, cx: &App) -> Pixels;
    fn set_size(&self, size: Option<Pixels>, window: &mut Window, cx: &mut App);
    fn icon(&self, window: &Window, cx: &App) -> Option<IconName>;
    fn icon_tooltip(&self, window: &Window, cx: &App) -> Option<&'static str>;
    fn toggle_action(&self, window: &Window, cx: &App) -> Box<dyn Action>;
    fn panel_focus_handle(&self, cx: &App) -> FocusHandle;
    fn to_any(&self) -> AnyView;
}

impl<T> PanelHandle for Entity<T>
where
    T: Panel,
{
    fn panel_id(&self) -> EntityId {
        Entity::entity_id(self)
    }

    fn persistent_name(&self) -> &'static str {
        T::persistent_name()
    }

    fn position(&self, window: &Window, cx: &App) -> DockPosition {
        self.read(cx).position(window, cx)
    }

    fn position_is_valid(&self, position: DockPosition, cx: &App) -> bool {
        self.read(cx).position_is_valid(position)
    }

    fn set_position(&self, position: DockPosition, window: &mut Window, cx: &mut App) {
        self.update(cx, |this, cx| this.set_position(position, window, cx));
    }

    fn size(&self, window: &Window, cx: &App) -> Pixels {
        self.read(cx).size(window, cx)
    }

    fn set_size(&self, size: Option<Pixels>, window: &mut Window, cx: &mut App) {
        self.update(cx, |this, cx| this.set_size(size, window, cx));
    }

    fn icon(&self, window: &Window, cx: &App) -> Option<IconName> {
        self.read(cx).icon(window, cx)
    }

    fn icon_tooltip(&self, window: &Window, cx: &App) -> Option<&'static str> {
        self.read(cx).icon_tooltip(window, cx)
    }

    fn toggle_action(&self, _window: &Window, cx: &App) -> Box<dyn Action> {
        self.read(cx).toggle_action()
    }

    fn panel_focus_handle(&self, cx: &App) -> FocusHandle {
        self.read(cx).focus_handle(cx)
    }

    fn to_any(&self) -> AnyView {
        self.clone().into()
    }
}

impl From<&dyn PanelHandle> for AnyView {
    fn from(val: &dyn PanelHandle) -> Self {
        val.to_any()
    }
}

struct PanelEntry {
    panel: Arc<dyn PanelHandle>,
}

pub struct Dock {
    position: DockPosition,
    panel_entries: Vec<PanelEntry>,
    _workspace: WeakEntity<Workspace>,
    is_open: bool,
    active_panel_index: Option<usize>,
    focus_handle: FocusHandle,
}

impl Dock {
    pub fn new(
        position: DockPosition,
        workspace: WeakEntity<Workspace>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            position,
            panel_entries: Default::default(),
            _workspace: workspace,
            is_open: false,
            active_panel_index: None,
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn position(&self) -> DockPosition {
        self.position
    }

    pub fn is_open(&self) -> bool {
        self.is_open
    }

    pub fn set_open(&mut self, is_open: bool, cx: &mut Context<Self>) {
        self.is_open = is_open;
        cx.notify();
    }

    pub fn toggle_open(&mut self, cx: &mut Context<Self>) {
        self.is_open = !self.is_open;
        cx.notify();
    }

    pub fn active_panel_index(&self) -> Option<usize> {
        self.active_panel_index
    }

    pub fn set_active_panel_index(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        self.active_panel_index = index;
        cx.notify();
    }

    pub fn add_panel<T: Panel>(
        &mut self,
        panel: Entity<T>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let panel_handle: Arc<dyn PanelHandle> = Arc::new(panel);

        if self.active_panel_index.is_none() {
            self.active_panel_index = Some(0);
        }

        self.panel_entries.push(PanelEntry {
            panel: panel_handle,
        });

        cx.notify();
    }

    pub fn panel_index(&self, panel_id: EntityId) -> Option<usize> {
        self.panel_entries
            .iter()
            .position(|entry| entry.panel.panel_id() == panel_id)
    }

    pub fn activate_panel(&mut self, panel_id: EntityId, cx: &mut Context<Self>) {
        let Some(index) = self.panel_index(panel_id) else {
            return;
        };

        self.active_panel_index = Some(index);
        self.is_open = true;
        cx.notify();
    }

    fn active_panel_entry(&self) -> Option<&PanelEntry> {
        let active_panel_index = self.active_panel_index?;
        self.panel_entries.get(active_panel_index)
    }

    pub fn active_panel(&self) -> Option<&Arc<dyn PanelHandle>> {
        let panel_entry = self.active_panel_entry()?;
        Some(&panel_entry.panel)
    }

    fn visible_entry(&self) -> Option<&PanelEntry> {
        if self.is_open {
            self.active_panel_entry()
        } else {
            None
        }
    }

    pub fn toggle_action(&self) -> Box<dyn Action> {
        match self.position {
            DockPosition::Left => crate::ToggleLeftDock.boxed_clone(),
            DockPosition::Bottom => crate::ToggleBottomDock.boxed_clone(),
            DockPosition::Right => crate::ToggleRightDock.boxed_clone(),
        }
    }

    fn dispatch_context() -> KeyContext {
        let mut dispatch_context = KeyContext::new_with_defaults();
        dispatch_context.add("Dock");
        dispatch_context
    }

    pub fn resize_active_panel(
        &mut self,
        size: Option<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.active_panel_entry() else {
            return;
        };

        let size = size.map(|size| size.max(RESIZE_HANDLE_SIZE).round());
        entry.panel.set_size(size, window, cx);
        cx.notify();
    }
}

impl Focusable for Dock {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Dock {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(entry) = self.visible_entry() {
            let size = entry.panel.size(window, cx);
            let position = self.position;

            let create_resize_handle = || {
                let handle = gpui::div()
                    .id("resize-handle")
                    .on_drag(DraggedDock(position), |dock, _, _, cx| {
                        cx.stop_propagation();
                        cx.new(|_| dock.clone())
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_, _: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |dock, e: &MouseUpEvent, window, cx| {
                            if e.click_count == 2 {
                                dock.resize_active_panel(None, window, cx);
                                cx.stop_propagation();
                            }
                        }),
                    )
                    .occlude();

                match position {
                    DockPosition::Left => gpui::deferred(
                        handle
                            .absolute()
                            .right(-RESIZE_HANDLE_SIZE / 2.)
                            .top(gpui::px(0.))
                            .h_full()
                            .w(RESIZE_HANDLE_SIZE)
                            .cursor_col_resize(),
                    ),
                    DockPosition::Bottom => gpui::deferred(
                        handle
                            .absolute()
                            .top(-RESIZE_HANDLE_SIZE / 2.)
                            .left(gpui::px(0.))
                            .w_full()
                            .h(RESIZE_HANDLE_SIZE)
                            .cursor_row_resize(),
                    ),
                    DockPosition::Right => gpui::deferred(
                        handle
                            .absolute()
                            .left(-RESIZE_HANDLE_SIZE / 2.)
                            .top(gpui::px(0.))
                            .h_full()
                            .w(RESIZE_HANDLE_SIZE)
                            .cursor_col_resize(),
                    ),
                }
            };

            let theme_colors = cx.theme().colors();

            gpui::div()
                .key_context(Self::dispatch_context())
                .track_focus(&self.focus_handle)
                .flex()
                .bg(theme_colors.panel_background)
                .border_color(theme_colors.border)
                .overflow_hidden()
                .map(|this| match position.axis() {
                    Axis::Horizontal => this.w(size).h_full().flex_row(),
                    Axis::Vertical => this.h(size).w_full().flex_col(),
                })
                .map(|this| match position {
                    DockPosition::Left => this.border_r_1(),
                    DockPosition::Right => this.border_l_1(),
                    DockPosition::Bottom => this.border_t_1(),
                })
                .child(
                    gpui::div()
                        .map(|this| match position.axis() {
                            Axis::Horizontal => this.min_w(size).h_full(),
                            Axis::Vertical => this.min_h(size).w_full(),
                        })
                        .child(entry.panel.to_any()),
                )
                .child(create_resize_handle())
        } else {
            gpui::div()
                .key_context(Self::dispatch_context())
                .track_focus(&self.focus_handle)
        }
    }
}

pub struct PanelButtons {
    dock: Entity<Dock>,
}

impl PanelButtons {
    pub fn new(dock: Entity<Dock>, cx: &mut Context<Self>) -> Self {
        cx.observe(&dock, |_, _, cx| cx.notify()).detach();
        Self { dock }
    }
}

impl Render for PanelButtons {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dock = self.dock.read(cx);
        let active_index = dock.active_panel_index();
        let is_open = dock.is_open();
        let focus_handle = dock.focus_handle(cx);

        let buttons: Vec<_> = dock
            .panel_entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                let icon = entry.panel.icon(window, cx)?;
                let icon_tooltip = entry.panel.icon_tooltip(window, cx)?;
                let name = entry.panel.persistent_name();

                let is_active_button = Some(index) == active_index && is_open;
                let (action, tooltip) = if is_active_button {
                    let action = dock.toggle_action();
                    (action, format!("Close {} Dock", dock.position().label()))
                } else {
                    let action = entry.panel.toggle_action(window, cx);
                    (action, icon_tooltip.to_string())
                };

                let action = action.boxed_clone();
                let tooltip = tooltip.clone();
                let focus_handle = focus_handle.clone();

                Some(
                    IconButton::new(format!("{name}-button-{is_active_button}"), icon)
                        .variant(ui::ButtonVariant::Subtle)
                        .size(ButtonSize::Compact)
                        .shape(ButtonShape::Square)
                        .icon_color(Color::Muted)
                        .selected_icon_color(Color::Default)
                        .toggle_state(is_active_button)
                        .tooltip(Tooltip::for_action_title(tooltip, action.as_ref()))
                        .on_click(move |_, window, cx| {
                            window.focus(&focus_handle, cx);
                            window.dispatch_action(action.boxed_clone(), cx);
                        }),
                )
            })
            .collect();

        gpui::div()
            .flex()
            .flex_row()
            .gap_1()
            .children(buttons)
            .font_ui(cx)
            .text_ui_sm(cx)
    }
}

impl StatusItemView for PanelButtons {
    fn set_active_pane(
        &mut self,
        _active_pane: &Entity<crate::pane::Pane>,
        _cx: &mut Context<Self>,
    ) {
        // Panel buttons are not dependent on center-pane active item.
    }
}
