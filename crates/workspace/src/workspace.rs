pub mod dock;
pub mod pane;
pub mod panel;
pub mod status_bar;

use gpui::{
    Action, App, Axis, Bounds, DragMoveEvent, Empty, Entity, EntityId, FocusHandle, Focusable,
    KeyBinding, KeyContext, MouseButton, MouseDownEvent, Pixels, Point, Subscription, Window,
    prelude::*,
};

use theme::ActiveTheme;
use theme::{GlobalTheme, SystemAppearance};
use ui::StyledTypography;

use crate::{
    dock::Dock,
    pane::Pane,
    panel::{ProjectPanel, ResponsePanel, buttons::PanelButtons, project_panel, response_panel},
    status_bar::StatusBar,
};

gpui::actions!(
    workspace,
    [
        SendRequest,
        ToggleBottomDock,
        ToggleLeftDock,
        ToggleRightDock
    ]
);

const KEY_CONTEXT: &str = "Workspace";
const MIN_DOCK_WIDTH: Pixels = gpui::px(110.0);
const MIN_PANE_WIDTH: Pixels = gpui::px(250.0);
const MIN_CONFIG_PANE_HEIGHT: Pixels = gpui::px(180.0);
const MIN_RESPONSE_PANE_HEIGHT: Pixels = gpui::px(110.0);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", SendRequest, Some("RequestUrl > Editor")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-j", ToggleBottomDock, Some(KEY_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-j", ToggleBottomDock, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-b", ToggleLeftDock, Some(KEY_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-b", ToggleLeftDock, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "cmd-shift-r",
            response_panel::ToggleFocus,
            Some(KEY_CONTEXT),
        ),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-shift-r",
            response_panel::ToggleFocus,
            Some(KEY_CONTEXT),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-e", project_panel::ToggleFocus, Some(KEY_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-shift-e",
            project_panel::ToggleFocus,
            Some(KEY_CONTEXT),
        ),
    ]);
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DockPosition {
    Left,
    Bottom,
    Right,
}

impl DockPosition {
    pub fn label(&self) -> &'static str {
        match self {
            DockPosition::Left => "Left",
            DockPosition::Bottom => "Bottom",
            DockPosition::Right => "Right",
        }
    }

    pub fn axis(&self) -> Axis {
        match self {
            DockPosition::Left | DockPosition::Right => Axis::Horizontal,
            DockPosition::Bottom => Axis::Vertical,
        }
    }
}

pub struct Workspace {
    left_dock: Entity<Dock>,
    bottom_dock: Entity<Dock>,
    right_dock: Entity<Dock>,
    pane: Entity<Pane>,
    response_panel: Entity<ResponsePanel>,
    status_bar: Entity<StatusBar>,
    bounds: Bounds<Pixels>,
    previous_dock_drag_coordinates: Option<Point<Pixels>>,
    _window_appearance_subscription: Subscription,
}

#[derive(Clone)]
pub struct DraggedDock(pub DockPosition);

impl Render for DraggedDock {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

impl Workspace {
    fn dock_at_position(&self, position: DockPosition) -> &Entity<Dock> {
        match position {
            DockPosition::Left => &self.left_dock,
            DockPosition::Bottom => &self.bottom_dock,
            DockPosition::Right => &self.right_dock,
        }
    }

    fn toggle_dock(&mut self, position: DockPosition, window: &mut Window, cx: &mut Context<Self>) {
        let dock = self.dock_at_position(position).clone();
        let was_visible = dock.read(cx).is_open();
        if was_visible && !window.bindings_for_action(&menu::Cancel).is_empty() {
            // Move focus to the pane first so dismissing a menu does not focus a hidden dock element.
            let focus_handle = self.pane.read(cx).focus_handle(cx);
            window.focus(&focus_handle, cx);
        }
        window.dispatch_action(menu::Cancel.boxed_clone(), cx);

        let mut focus_center = false;

        dock.update(cx, |dock, cx| {
            if was_visible && dock.focus_handle(cx).contains_focused(window, cx) {
                focus_center = true;
            }
            dock.set_open(!was_visible, cx);

            if let Some(active_panel) = dock.active_panel() {
                if !was_visible {
                    let focus_handle = active_panel.panel_focus_handle(cx);
                    window.focus(&focus_handle, cx);
                }
            }
        });

        if focus_center {
            let focus_handle = self.pane.read(cx).focus_handle(cx);
            window.focus(&focus_handle, cx);
        }

        cx.notify();
    }

    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let window_appearance_subscription =
            cx.observe_window_appearance(window, |_, window, cx| {
                let window_appearance = window.appearance();
                *SystemAppearance::global_mut(cx) = SystemAppearance(window_appearance.into());
                GlobalTheme::reload_theme(cx);
            });

        let pane = cx.new(Pane::new);
        let pane_focus_handle = pane.read(cx).focus_handle(cx);
        window.focus(&pane_focus_handle, cx);

        let workspace = cx.entity().downgrade();

        let left_dock = cx.new(|cx| Dock::new(DockPosition::Left, workspace.clone(), cx));
        let bottom_dock = cx.new(|cx| Dock::new(DockPosition::Bottom, workspace.clone(), cx));
        let right_dock = cx.new(|cx| Dock::new(DockPosition::Right, workspace.clone(), cx));

        let left_dock_panel = cx.new(ProjectPanel::new);
        left_dock.update(cx, |left_dock, cx| {
            left_dock.add_panel(left_dock_panel, window, cx);
        });

        let response_panel = cx.new(|cx| ResponsePanel::new(window, cx));
        bottom_dock.update(cx, |bottom_dock, cx| {
            bottom_dock.add_panel(response_panel.clone(), window, cx);
        });

        let left_dock_buttons = cx.new(|cx| PanelButtons::new(left_dock.clone(), cx));
        let bottom_dock_buttons = cx.new(|cx| PanelButtons::new(bottom_dock.clone(), cx));
        let right_dock_buttons = cx.new(|cx| PanelButtons::new(right_dock.clone(), cx));

        pane.update(cx, |pane, cx| {
            pane.set_response_targets(bottom_dock.clone(), response_panel.clone(), cx);
        });

        let status_bar = cx.new(|cx| StatusBar::new(pane.clone(), cx));
        status_bar.update(cx, |status_bar, cx| {
            status_bar.add_left_item(left_dock_buttons, cx);
            status_bar.add_right_item(bottom_dock_buttons, cx);
            status_bar.add_right_item(right_dock_buttons, cx);
        });

        Self {
            left_dock,
            bottom_dock,
            right_dock,
            pane,
            response_panel,
            status_bar,
            bounds: Bounds::default(),
            previous_dock_drag_coordinates: None,
            _window_appearance_subscription: window_appearance_subscription,
        }
    }

    fn resize_left_dock(&mut self, size: Pixels, window: &mut Window, cx: &mut App) {
        let size = size
            .min(self.bounds.size.width - MIN_PANE_WIDTH)
            .max(MIN_DOCK_WIDTH);
        self.left_dock.update(cx, |dock, cx| {
            dock.resize_active_panel(Some(size), window, cx);
        })
    }

    fn resize_right_dock(&mut self, size: Pixels, window: &mut Window, cx: &mut App) {
        let size = size
            .min(self.bounds.size.width - MIN_PANE_WIDTH)
            .max(MIN_DOCK_WIDTH);
        self.right_dock.update(cx, |dock, cx| {
            dock.resize_active_panel(Some(size), window, cx);
        })
    }

    fn resize_bottom_dock(&mut self, size: Pixels, window: &mut Window, cx: &mut App) {
        let size = size
            .min(self.bounds.size.height - MIN_CONFIG_PANE_HEIGHT)
            .max(MIN_RESPONSE_PANE_HEIGHT);
        self.bottom_dock.update(cx, |dock, cx| {
            dock.resize_active_panel(Some(size), window, cx);
        })
    }

    fn toggle_panel_focus(
        &mut self,
        panel_id: EntityId,
        dock: &Entity<Dock>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut focus_center = false;
        dock.update(cx, |dock, cx| {
            dock.activate_panel(panel_id, cx);
            let Some(active_panel) = dock.active_panel() else {
                return;
            };

            let focus_handle = active_panel.panel_focus_handle(cx);
            if focus_handle.contains_focused(window, cx) {
                focus_center = true;
            } else {
                dock.set_open(true, cx);
                window.focus(&focus_handle, cx);
            }
        });

        if focus_center {
            let focus_handle = self.pane.read(cx).focus_handle(cx);
            window.focus(&focus_handle, cx);
        }

        cx.notify();
    }

    fn toggle_project_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let panel_id = self
            .left_dock
            .read(cx)
            .active_panel()
            .map(|panel| panel.panel_id());
        if let Some(panel_id) = panel_id {
            let dock = self.left_dock.clone();
            self.toggle_panel_focus(panel_id, &dock, window, cx);
        }
    }

    fn toggle_response_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let panel_id = Entity::entity_id(&self.response_panel);
        let dock = self.bottom_dock.clone();
        self.toggle_panel_focus(panel_id, &dock, window, cx);
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui_font = theme::setup_ui_font(window, cx);
        let theme_colors = cx.theme().colors();
        let mut context = KeyContext::new_with_defaults();
        context.add(KEY_CONTEXT);
        if self.left_dock.read(cx).is_open() {
            if let Some(active_panel) = self.left_dock.read(cx).active_panel() {
                context.set("left_dock", active_panel.persistent_name());
            }
        }
        if self.right_dock.read(cx).is_open() {
            if let Some(active_panel) = self.right_dock.read(cx).active_panel() {
                context.set("right_dock", active_panel.persistent_name());
            }
        }
        if self.bottom_dock.read(cx).is_open() {
            if let Some(active_panel) = self.bottom_dock.read(cx).active_panel() {
                context.set("bottom_dock", active_panel.persistent_name());
            }
        }
        let focus_handle = self.focus_handle(cx);
        gpui::div()
            .id("workspace")
            .key_context(context)
            .track_focus(&focus_handle)
            .relative()
            .flex()
            .flex_col()
            .bg(theme_colors.background)
            .text_color(theme_colors.text)
            .font(ui_font)
            .text_ui(cx)
            .size_full()
            .on_action(cx.listener(|workspace, _: &ToggleLeftDock, window, cx| {
                workspace.toggle_dock(DockPosition::Left, window, cx);
            }))
            .on_action(cx.listener(|workspace, _: &ToggleRightDock, window, cx| {
                workspace.toggle_dock(DockPosition::Right, window, cx);
            }))
            .on_action(cx.listener(|workspace, _: &ToggleBottomDock, window, cx| {
                workspace.toggle_dock(DockPosition::Bottom, window, cx);
            }))
            .on_action(
                cx.listener(|workspace, _: &project_panel::ToggleFocus, window, cx| {
                    workspace.toggle_project_panel(window, cx);
                }),
            )
            .on_action(
                cx.listener(|workspace, _: &response_panel::ToggleFocus, window, cx| {
                    workspace.toggle_response_panel(window, cx);
                }),
            )
            .on_drag_move(
                cx.listener(|workspace, e: &DragMoveEvent<DraggedDock>, window, cx| {
                    if workspace.previous_dock_drag_coordinates != Some(e.event.position) {
                        workspace.previous_dock_drag_coordinates = Some(e.event.position);
                        match e.drag(cx).0 {
                            DockPosition::Left => {
                                workspace.resize_left_dock(
                                    e.event.position.x - workspace.bounds.left(),
                                    window,
                                    cx,
                                );
                            }
                            DockPosition::Right => {
                                workspace.resize_right_dock(
                                    workspace.bounds.right() - e.event.position.x,
                                    window,
                                    cx,
                                );
                            }
                            DockPosition::Bottom => {
                                workspace.resize_bottom_dock(
                                    workspace.bounds.bottom() - e.event.position.y,
                                    window,
                                    cx,
                                );
                            }
                        }
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|workspace, _: &MouseDownEvent, window, cx| {
                    if !window.default_prevented() {
                        let focus_handle = workspace.focus_handle(cx);
                        window.focus(&focus_handle, cx);
                    }
                }),
            )
            .child(
                gpui::div()
                    .relative()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .overflow_hidden()
                    .child({
                        let this = cx.entity();
                        gpui::canvas(
                            move |bounds, _window, cx| {
                                this.update(cx, |this, _cx| {
                                    this.bounds = bounds;
                                });
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full()
                    })
                    .child(
                        gpui::div()
                            .flex_none()
                            .overflow_hidden()
                            .child(self.left_dock.clone()),
                    )
                    .child(
                        gpui::div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .h_full()
                            .overflow_hidden()
                            .child(
                                gpui::div()
                                    .flex_1()
                                    .overflow_hidden()
                                    .child(self.pane.clone()),
                            )
                            .child(self.bottom_dock.clone()),
                    )
                    .child(
                        gpui::div()
                            .flex_none()
                            .overflow_hidden()
                            .child(self.right_dock.clone()),
                    ),
            )
            .child(self.status_bar.clone())
    }
}

impl Focusable for Workspace {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.pane.read(cx).focus_handle(cx)
    }
}
