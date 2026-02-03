use gpui::{
    App, Bounds, DragMoveEvent, Entity, Pixels, Point, Window, canvas, div, prelude::*, px, rgb,
};

use crate::{dock::Dock, pane::Pane, status_bar::StatusBar};

pub mod dock;
pub mod pane;
pub mod status_bar;

const MIN_DOCK_WIDTH: Pixels = px(110.0);
const MIN_PANE_WIDTH: Pixels = px(250.0);

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
}

#[derive(Clone)]
pub struct DraggedDock(pub DockPosition);

impl Render for DraggedDock {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        gpui::Empty
    }
}

impl Workspace {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let workspace = cx.entity();

        Self {
            dock: cx.new(Dock::new),
            pane: cx.new(Pane::new),
            status_bar: cx.new(|cx| StatusBar::new(workspace, cx)),
            bounds: Bounds::default(),
            previous_dock_drag_coordinates: None,
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
        div()
            .id("workspace")
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(0x141414))
            .text_color(rgb(0xffffff))
            .text_xs()
            .size_full()
            .child({
                let this = cx.entity();
                canvas(
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
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .overflow_hidden()
                    .child(div().flex_none().overflow_hidden().child(self.dock.clone()))
                    .child(self.pane.clone()),
            )
            .child(self.status_bar.clone())
    }
}
