use anyhow::Context as AnyhowContext;
use gpui::{
    Action, AnyView, App, Axis, Context, Empty, Entity, EntityId, FocusHandle, Focusable,
    IntoElement, KeyContext, MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Render,
    Subscription, WeakEntity, Window, prelude::*,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use theme::ActiveTheme;
use ui::{
    ButtonCommon, ButtonVariant, Clickable, Color, Disableable, IconButton, IconButtonShape,
    IconSize, StyledTypography, Toggleable, Tooltip,
};

use crate::{DockData, Workspace, pane::Pane, status_bar::StatusItemView};

pub(crate) const RESIZE_HANDLE_SIZE: Pixels = gpui::px(6.);
pub(crate) const PANEL_SIZE_STATE_KEY: &str = "dock_panel_size";

pub trait Panel: Focusable + Render + Sized {
    fn persistent_name() -> &'static str;
    fn panel_key() -> &'static str;
    fn default_size(&self, window: &Window, cx: &App) -> Pixels;
    fn icon(&self, window: &Window, cx: &App) -> Option<ui::IconName>;
    fn icon_tooltip(&self, window: &Window, cx: &App) -> Option<&'static str>;
    fn toggle_action(&self) -> Box<dyn Action>;
    fn starts_open(&self, _window: &Window, _cx: &App) -> bool {
        false
    }
    fn set_active(&mut self, _active: bool, _window: &mut Window, _cx: &mut Context<Self>) {}
    fn auto_hidden(&self) -> bool {
        false
    }
    fn set_auto_hidden(&mut self, _: bool, _: &mut Context<Self>) {}
    fn activation_priority(&self) -> u32;
    fn enabled(&self, _cx: &App) -> bool {
        true
    }
}

pub trait PanelHandle: Send + Sync {
    fn panel_id(&self) -> EntityId;
    fn persistent_name(&self) -> &'static str;
    fn panel_key(&self) -> &'static str;
    fn default_size(&self, window: &Window, cx: &App) -> Pixels;
    fn icon(&self, window: &Window, cx: &App) -> Option<ui::IconName>;
    fn icon_tooltip(&self, window: &Window, cx: &App) -> Option<&'static str>;
    fn toggle_action(&self, window: &Window, cx: &App) -> Box<dyn Action>;
    fn set_active(&self, active: bool, window: &mut Window, cx: &mut App);
    fn auto_hidden(&self, cx: &App) -> bool;
    fn set_auto_hidden(&self, auto_hidden: bool, cx: &mut App);
    fn activation_priority(&self, cx: &App) -> u32;
    fn enabled(&self, cx: &App) -> bool;
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

    fn panel_key(&self) -> &'static str {
        T::panel_key()
    }

    fn default_size(&self, window: &Window, cx: &App) -> Pixels {
        self.read(cx).default_size(window, cx)
    }

    fn icon(&self, window: &Window, cx: &App) -> Option<ui::IconName> {
        self.read(cx).icon(window, cx)
    }

    fn icon_tooltip(&self, window: &Window, cx: &App) -> Option<&'static str> {
        self.read(cx).icon_tooltip(window, cx)
    }

    fn toggle_action(&self, _window: &Window, cx: &App) -> Box<dyn Action> {
        self.read(cx).toggle_action()
    }

    fn set_active(&self, active: bool, window: &mut Window, cx: &mut App) {
        self.update(cx, |this, cx| this.set_active(active, window, cx));
    }

    fn auto_hidden(&self, cx: &App) -> bool {
        self.read(cx).auto_hidden()
    }

    fn set_auto_hidden(&self, auto_hidden: bool, cx: &mut App) {
        self.update(cx, |this, cx| this.set_auto_hidden(auto_hidden, cx));
    }

    fn activation_priority(&self, cx: &App) -> u32 {
        self.read(cx).activation_priority()
    }

    fn enabled(&self, cx: &App) -> bool {
        self.read(cx).enabled(cx)
    }

    fn panel_focus_handle(&self, cx: &App) -> FocusHandle {
        self.read(cx).focus_handle(cx)
    }

    fn to_any(&self) -> AnyView {
        self.clone().into()
    }
}

impl From<&dyn PanelHandle> for AnyView {
    fn from(value: &dyn PanelHandle) -> Self {
        value.to_any()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockPosition {
    Left,
    Bottom,
}

impl DockPosition {
    pub fn label(&self) -> &'static str {
        match self {
            DockPosition::Left => "Left",
            DockPosition::Bottom => "Bottom",
        }
    }

    pub fn axis(&self) -> Axis {
        match self {
            DockPosition::Left => Axis::Horizontal,
            DockPosition::Bottom => Axis::Vertical,
        }
    }
}

#[derive(Clone)]
pub struct DraggedDock(pub DockPosition);

impl Render for DraggedDock {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct PanelSizeState {
    pub size: Option<Pixels>,
}

pub(crate) struct PanelEntry {
    panel: Arc<dyn PanelHandle>,
    size_state: PanelSizeState,
    _observe_panel_subscription: Subscription,
}

impl PanelEntry {
    fn new(
        panel: Arc<dyn PanelHandle>,
        size_state: PanelSizeState,
        observe_panel_subscription: Subscription,
    ) -> Self {
        Self {
            panel,
            size_state,
            _observe_panel_subscription: observe_panel_subscription,
        }
    }

    pub(crate) fn panel(&self) -> &Arc<dyn PanelHandle> {
        &self.panel
    }
}

pub struct Dock {
    position: DockPosition,
    workspace: WeakEntity<Workspace>,
    panel_entries: Vec<PanelEntry>,
    is_open: bool,
    active_panel_index: Option<usize>,
    pub(crate) serialized_dock: Option<DockData>,
    focus_handle: FocusHandle,
    _focus_subscription: Subscription,
}

impl Dock {
    pub fn new(
        position: DockPosition,
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let focus_subscription =
            cx.on_focus(&focus_handle, window, |dock: &mut Dock, window, cx| {
                if let Some(active_entry) = dock.active_panel_entry() {
                    active_entry
                        .panel()
                        .panel_focus_handle(cx)
                        .focus(window, cx);
                }
            });

        Self {
            position,
            workspace,
            panel_entries: Vec::default(),
            is_open: false,
            active_panel_index: None,
            serialized_dock: None,
            focus_handle: focus_handle.clone(),
            _focus_subscription: focus_subscription,
        }
    }

    pub fn position(&self) -> DockPosition {
        self.position
    }

    pub fn panel<T: Panel>(&self) -> Option<Entity<T>> {
        self.panel_entries
            .iter()
            .find_map(|entry| entry.panel.to_any().downcast().ok())
    }

    pub(crate) fn panel_entries(&self) -> &[PanelEntry] {
        &self.panel_entries
    }

    pub fn is_open(&self) -> bool {
        self.is_open
    }

    pub fn active_panel_index(&self) -> Option<usize> {
        self.active_panel_index
    }

    pub fn set_open(&mut self, is_open: bool, window: &mut Window, cx: &mut Context<Self>) {
        if is_open != self.is_open {
            self.is_open = is_open;
            if let Some(active_panel) = self.active_panel_entry() {
                active_panel.panel().set_active(is_open, window, cx);
            }

            cx.notify();
        }
    }

    pub fn add_panel<T: Panel>(
        &mut self,
        panel: &Entity<T>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> usize {
        let panel = panel.clone();
        let observe_panel_subscription = cx.observe(&panel, |_, _, cx| cx.notify());
        let panel_priority = panel.read(cx).activation_priority();
        let panel_starts_open = panel.read(cx).starts_open(window, cx);
        let panel_handle: Arc<dyn PanelHandle> = Arc::new(panel.clone());
        let size_state = PanelSizeState::default();
        let index = match self
            .panel_entries
            .binary_search_by_key(&panel_priority, |entry| {
                entry.panel().activation_priority(cx)
            }) {
            Ok(index) => {
                #[cfg(debug_assertions)]
                {
                    panic!(
                        "Panels `{}` and `{}` have the same activation priority. Each panel must have a unique priority so the status bar order is deterministic.",
                        T::panel_key(),
                        self.panel_entries[index].panel().panel_key()
                    );
                }

                #[cfg(not(debug_assertions))]
                {
                    index
                }
            }
            Err(index) => index,
        };

        if let Some(active_panel_index) = self.active_panel_index.as_mut()
            && *active_panel_index >= index
        {
            *active_panel_index += 1;
        }

        self.panel_entries.insert(
            index,
            PanelEntry::new(panel_handle, size_state, observe_panel_subscription),
        );

        let restored = self.restore_state(window, cx);

        if !restored && panel_starts_open {
            self.activate_panel(index, window, cx);
            self.set_open(true, window, cx);
        }

        cx.notify();
        index
    }

    pub fn panel_index_for_type<T: Panel>(&self) -> Option<usize> {
        self.panel_entries
            .iter()
            .position(|entry| entry.panel().to_any().downcast::<T>().is_ok())
    }

    pub fn panel_index_for_persistent_name(&self, persistent_name: &str) -> Option<usize> {
        self.panel_entries
            .iter()
            .position(|entry| entry.panel().persistent_name() == persistent_name)
    }

    pub fn restore_state(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if let Some(serialized) = self.serialized_dock.clone() {
            if let Some(active_panel) = serialized.active_panel.as_deref()
                && let Some(panel_index) = self.panel_index_for_persistent_name(active_panel)
            {
                if let Some(panel) = self.panel_entries.get(panel_index).map(PanelEntry::panel) {
                    panel.set_auto_hidden(serialized.auto_hidden, cx);
                }

                if serialized.visible {
                    self.activate_panel(panel_index, window, cx);
                }
            }

            self.set_open(serialized.visible, window, cx);
            return true;
        }

        false
    }

    pub fn activate_panel(
        &mut self,
        panel_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if Some(panel_index) != self.active_panel_index {
            if let Some(active_panel) = self.active_panel_entry() {
                active_panel.panel().set_active(false, window, cx);
            }

            self.active_panel_index = Some(panel_index);
            if let Some(active_panel) = self.active_panel_entry() {
                active_panel.panel().set_active(true, window, cx);
            }

            cx.notify();
        }
    }

    fn active_panel_entry(&self) -> Option<&PanelEntry> {
        let active_panel_index = self.active_panel_index?;
        self.panel_entries.get(active_panel_index)
    }

    pub fn active_panel(&self) -> Option<&Arc<dyn PanelHandle>> {
        let panel_entry = self.active_panel_entry()?;
        Some(panel_entry.panel())
    }

    pub fn first_enabled_panel_idx(&mut self, cx: &mut Context<Self>) -> anyhow::Result<usize> {
        self.panel_entries
            .iter()
            .position(|entry| entry.panel.enabled(cx))
            .with_context(|| {
                format!(
                    "Couldn't find any enabled panel for the {} dock.",
                    self.position.label()
                )
            })
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
            DockPosition::Left => actions::workspace::ToggleLeftDock.boxed_clone(),
            DockPosition::Bottom => actions::workspace::ToggleBottomDock.boxed_clone(),
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
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(active_panel_index) = self.active_panel_index else {
            return;
        };
        let Some(entry) = self.panel_entries.get_mut(active_panel_index) else {
            return;
        };

        entry.size_state.size = size.map(|size| size.max(RESIZE_HANDLE_SIZE).round());
        let panel_key = entry.panel.panel_key();
        let size_state = entry.size_state;
        let workspace = self.workspace.clone();
        cx.defer(move |cx| {
            if let Some(workspace) = workspace.upgrade() {
                workspace.update(cx, |workspace, cx| {
                    workspace.save_panel_size_state(panel_key, size_state, cx);
                });
            }
        });
        cx.notify();
    }

    pub fn stored_panel_size_state(&self, panel: &dyn PanelHandle) -> Option<PanelSizeState> {
        self.panel_entries
            .iter()
            .find(|entry| entry.panel.panel_id() == panel.panel_id())
            .map(|entry| entry.size_state)
    }

    pub fn set_panel_size_state(
        &mut self,
        panel: &dyn PanelHandle,
        size_state: PanelSizeState,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(entry) = self
            .panel_entries
            .iter_mut()
            .find(|entry| entry.panel.panel_id() == panel.panel_id())
        {
            entry.size_state = size_state;
            cx.notify();
            true
        } else {
            false
        }
    }

    pub fn clamp_panel_size(&mut self, max_size: Pixels, window: &Window, cx: &mut Context<Self>) {
        let max_size = max_size.max(RESIZE_HANDLE_SIZE).round();
        let mut size_changed = false;

        for entry in &mut self.panel_entries {
            let size = entry
                .size_state
                .size
                .unwrap_or_else(|| entry.panel.default_size(window, cx));
            if size > max_size {
                entry.size_state.size = Some(max_size);
                size_changed = true;
            }
        }

        if size_changed {
            cx.notify();
        }
    }

    fn resolved_panel_size(entry: &PanelEntry, window: &Window, cx: &App) -> Pixels {
        entry
            .size_state
            .size
            .unwrap_or_else(|| entry.panel.default_size(window, cx))
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
            let size = Self::resolved_panel_size(entry, window, cx);
            let position = self.position;

            let create_resize_handle = || {
                let handle = gpui::div()
                    .id("dock-drag-handle")
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
                    DockPosition::Bottom => this.border_t_1(),
                })
                .child(
                    gpui::div()
                        .map(|this| match position.axis() {
                            Axis::Horizontal => this.w_full().h_full(),
                            Axis::Vertical => this.h_full().w_full(),
                        })
                        .child(entry.panel().to_any()),
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
        let is_disabled = !dock
            .panel_entries()
            .iter()
            .any(|entry| entry.panel().enabled(cx));
        let active_index = dock.active_panel_index();
        let is_open = dock.is_open();
        let focus_handle = dock.focus_handle(cx);
        let buttons = dock
            .panel_entries()
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                let icon = entry.panel().icon(window, cx)?;
                let icon_tooltip = entry.panel().icon_tooltip(window, cx)?;
                let panel_key = entry.panel().panel_key();

                let is_active_button = Some(index) == active_index && is_open;
                let (action, tooltip) = if is_active_button {
                    let action = dock.toggle_action();
                    (action, format!("Close {} Dock", dock.position().label()))
                } else {
                    let action = entry.panel().toggle_action(window, cx);
                    (action, icon_tooltip.to_string())
                };

                let action = action.boxed_clone();
                let tooltip = tooltip.clone();
                let focus_handle = focus_handle.clone();

                Some(
                    IconButton::new(format!("{panel_key}-button-{is_active_button}"), icon)
                        .variant(ButtonVariant::Ghost)
                        .icon_size(IconSize::Small)
                        .shape(IconButtonShape::Square)
                        .icon_color(Color::Muted)
                        .hover_icon_color(Color::Selected)
                        .disabled(is_disabled)
                        .toggle_state(is_active_button)
                        .tooltip(Tooltip::for_action_title(tooltip, action.as_ref()))
                        .on_click(move |_, window, cx| {
                            window.focus(&focus_handle, cx);
                            window.dispatch_action(action.boxed_clone(), cx);
                        }),
                )
            })
            .collect::<Vec<_>>();

        gpui::div()
            .flex()
            .gap_1()
            .children(buttons)
            .font_ui(cx)
            .text_ui_sm(cx)
    }
}

impl StatusItemView for PanelButtons {
    fn set_active_pane(
        &mut self,
        _active_pane: &Entity<Pane>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }
}

#[cfg(any(test, feature = "test"))]
pub mod test {
    use super::*;

    gpui::actions!(test_only, [ToggleTestPanel]);

    pub struct TestPanel {
        pub active: bool,
        pub focus_handle: FocusHandle,
        pub default_size: Pixels,
        pub activation_priority: u32,
    }

    impl TestPanel {
        pub fn new(activation_priority: u32, cx: &mut App) -> Self {
            Self {
                active: false,
                focus_handle: cx.focus_handle(),
                default_size: gpui::px(250.0),
                activation_priority,
            }
        }
    }

    impl Render for TestPanel {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            gpui::div().id("test-panel").track_focus(&self.focus_handle)
        }
    }

    impl Panel for TestPanel {
        fn persistent_name() -> &'static str {
            "TestPanel"
        }

        fn panel_key() -> &'static str {
            "TestPanel"
        }

        fn default_size(&self, _window: &Window, _: &App) -> Pixels {
            self.default_size
        }

        fn icon(&self, _window: &Window, _: &App) -> Option<ui::IconName> {
            None
        }

        fn icon_tooltip(&self, _window: &Window, _cx: &App) -> Option<&'static str> {
            None
        }

        fn toggle_action(&self) -> Box<dyn Action> {
            ToggleTestPanel.boxed_clone()
        }

        fn set_active(&mut self, active: bool, _window: &mut Window, _cx: &mut Context<Self>) {
            self.active = active;
        }

        fn activation_priority(&self) -> u32 {
            self.activation_priority
        }
    }

    impl Focusable for TestPanel {
        fn focus_handle(&self, _cx: &App) -> FocusHandle {
            self.focus_handle.clone()
        }
    }
}
