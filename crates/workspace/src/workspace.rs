use gpui::{
    App, Bounds, DragMoveEvent, Entity, KeyBinding, MouseButton, MouseDownEvent, Pixels, Point,
    Window, actions, prelude::*,
};

use theme::ActiveTheme;
use theme::{GlobalTheme, SystemAppearance};
use ui::StyledTypography;

use crate::{dock::Dock, pane::Pane, status_bar::StatusBar};

pub mod dock;
pub mod pane;
pub mod status_bar;

actions!(workspace, [SendRequest]);

const MIN_DOCK_WIDTH: Pixels = gpui::px(110.0);
const MIN_PANE_WIDTH: Pixels = gpui::px(250.0);

pub fn init(cx: &mut App) {
    component::init();
    cx.bind_keys([KeyBinding::new(
        "enter",
        SendRequest,
        Some("RequestUrl > Editor"),
    )]);
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
        let workspace = cx.entity();
        let window_appearance_subscription =
            cx.observe_window_appearance(window, |_, window, cx| {
                let window_appearance = window.appearance();
                *SystemAppearance::global_mut(cx) = SystemAppearance(window_appearance.into());
                GlobalTheme::reload_theme(cx);
            });

        Self {
            dock: cx.new(Dock::new),
            pane: cx.new(Pane::new),
            status_bar: cx.new(|cx| StatusBar::new(workspace, cx)),
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme_colors = cx.theme().colors();
        gpui::div()
            .id("workspace")
            .relative()
            .flex()
            .flex_col()
            .bg(theme_colors.background)
            .text_color(theme_colors.text)
            .font_ui(cx)
            .text_ui(cx)
            .size_full()
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
                cx.listener(|_, _: &MouseDownEvent, window, _| {
                    window.blur();
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
