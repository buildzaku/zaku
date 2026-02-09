use gpui::{
    App, Bounds, DragMoveEvent, Entity, FocusHandle, Focusable, KeyBinding, MouseButton,
    MouseDownEvent, Pixels, Point, Window, actions, prelude::*,
};

use theme::ActiveTheme;
use theme::{GlobalTheme, SystemAppearance};
use ui::StyledTypography;

use crate::{dock::Dock, pane::Pane, status_bar::StatusBar};

pub mod dock;
pub mod pane;
pub mod status_bar;

actions!(workspace, [SendRequest, ToggleLeftDock]);

const KEY_CONTEXT: &str = "Workspace";
const MIN_DOCK_WIDTH: Pixels = gpui::px(110.);
const MIN_PANE_WIDTH: Pixels = gpui::px(250.);

pub fn init(cx: &mut App) {
    component::init();
    cx.bind_keys([
        KeyBinding::new("enter", SendRequest, Some("RequestUrl > Editor")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-b", ToggleLeftDock, Some(KEY_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-b", ToggleLeftDock, Some(KEY_CONTEXT)),
    ]);
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DockPosition {
    Left,
    Right,
}

pub struct Workspace {
    dock: Entity<Dock>,
    pane: Entity<Pane>,
    status_bar: Entity<StatusBar>,
    bounds: Bounds<Pixels>,
    previous_dock_drag_coordinates: Option<Point<Pixels>>,
    _window_appearance_subscription: gpui::Subscription,
}

#[derive(Clone)]
pub struct DraggedDock(pub DockPosition);

impl Render for DraggedDock {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        gpui::Empty
    }
}

impl Workspace {
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

        Self {
            dock: cx.new(Dock::new),
            pane,
            status_bar: cx.new(StatusBar::new),
            bounds: Bounds::default(),
            previous_dock_drag_coordinates: None,
            _window_appearance_subscription: window_appearance_subscription,
        }
    }

    fn resize_dock(&mut self, size: Pixels, window: &mut Window, cx: &mut App) {
        let size = size
            .min(self.bounds.size.width - MIN_PANE_WIDTH)
            .max(MIN_DOCK_WIDTH);
        self.dock.update(cx, |dock, cx| {
            dock.set_size(size, window, cx);
        });
    }

    fn toggle_dock(&mut self, cx: &mut Context<Self>) {
        self.dock.update(cx, |dock, cx| dock.toggle_visibility(cx));
        cx.notify();
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui_font = theme::setup_ui_font(window, cx);
        let theme_colors = cx.theme().colors();
        let focus_handle = self.focus_handle(cx);
        gpui::div()
            .id("workspace")
            .key_context(KEY_CONTEXT)
            .track_focus(&focus_handle)
            .relative()
            .flex()
            .flex_col()
            .bg(theme_colors.background)
            .text_color(theme_colors.text)
            .font(ui_font)
            .text_ui(cx)
            .size_full()
            .on_action(cx.listener(|workspace, _: &ToggleLeftDock, _window, cx| {
                workspace.toggle_dock(cx);
            }))
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
            .on_drag_move(
                cx.listener(|workspace, e: &DragMoveEvent<DraggedDock>, window, cx| {
                    if workspace.previous_dock_drag_coordinates != Some(e.event.position) {
                        workspace.previous_dock_drag_coordinates = Some(e.event.position);
                        match e.drag(cx).0 {
                            DockPosition::Left => {
                                workspace.resize_dock(
                                    e.event.position.x - workspace.bounds.left(),
                                    window,
                                    cx,
                                );
                            }
                            DockPosition::Right => {
                                workspace.resize_dock(
                                    workspace.bounds.right() - e.event.position.x,
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
                    .flex()
                    .flex_row()
                    .flex_1()
                    .overflow_hidden()
                    .child(
                        gpui::div()
                            .flex_none()
                            .overflow_hidden()
                            .child(self.dock.clone()),
                    )
                    .child(self.pane.clone()),
            )
            .child(self.status_bar.clone())
    }
}

impl Focusable for Workspace {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.pane.read(cx).focus_handle(cx)
    }
}
