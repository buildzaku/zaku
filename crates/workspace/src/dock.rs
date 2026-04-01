use anyhow::Context as AnyhowContext;
use gpui::{
    Action, App, Axis, Entity, FocusHandle, Focusable, KeyContext, MouseButton, MouseDownEvent,
    MouseUpEvent, Pixels, Subscription, Window, prelude::*,
};
use std::sync::Arc;

use theme::ActiveTheme;

use crate::{
    DockPosition, DraggedDock,
    panel::{Panel, PanelHandle},
};

pub(crate) const RESIZE_HANDLE_SIZE: Pixels = gpui::px(6.);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct PanelSizeState {
    size: Option<Pixels>,
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
    panel_entries: Vec<PanelEntry>,
    is_open: bool,
    active_panel_index: Option<usize>,
    focus_handle: FocusHandle,
    _focus_subscription: Subscription,
}

impl Dock {
    pub fn new(position: DockPosition, window: &mut Window, cx: &mut Context<Self>) -> Self {
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
            panel_entries: Default::default(),
            is_open: false,
            active_panel_index: None,
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
        panel: Entity<T>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
                if cfg!(debug_assertions) {
                    panic!(
                        "Panels `{}` and `{}` have the same activation priority. Each panel must have a unique priority so the status bar order is deterministic.",
                        T::panel_key(),
                        self.panel_entries[index].panel().panel_key()
                    );
                }
                index
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

        if panel_starts_open {
            self.activate_panel(index, window, cx);
            self.set_open(true, window, cx);
        }

        cx.notify();
    }

    pub fn panel_index_for_type<T: Panel>(&self) -> Option<usize> {
        self.panel_entries
            .iter()
            .position(|entry| entry.panel().to_any().downcast::<T>().is_ok())
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
            DockPosition::Left => crate::ToggleLeftDock.boxed_clone(),
            DockPosition::Bottom => crate::ToggleBottomDock.boxed_clone(),
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
        cx.notify();
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

    fn resolved_panel_size(&self, entry: &PanelEntry, window: &Window, cx: &App) -> Pixels {
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
            let size = self.resolved_panel_size(entry, window, cx);
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
                            Axis::Horizontal => this.min_w(size).h_full(),
                            Axis::Vertical => this.min_h(size).w_full(),
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
