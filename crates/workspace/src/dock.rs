use gpui::{
    MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Window, deferred, div, prelude::*, px, rgb,
};

use crate::{DockPosition, DraggedDock};

const DEFAULT_DOCK_SIZE: Pixels = px(250.0);
const RESIZE_HANDLE_SIZE: Pixels = px(6.0);

pub struct Dock {
    size: Pixels,
    position: DockPosition,
    visible: bool,
}

impl Dock {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            size: DEFAULT_DOCK_SIZE,
            position: DockPosition::Left,
            visible: true,
        }
    }

    pub fn set_size(&mut self, size: Pixels, _window: &mut Window, cx: &mut Context<Self>) {
        self.size = size.round();
        cx.notify();
    }

    pub fn toggle_visibility(&mut self, cx: &mut Context<Self>) {
        self.visible = !self.visible;
        cx.notify();
    }
}

impl Render for Dock {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let position = self.position;
        let create_resize_handle = || {
            let handle = div()
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
                    cx.listener(|dock, e: &MouseUpEvent, window, cx| {
                        if e.click_count == 2 {
                            dock.set_size(px(250.0), window, cx);
                            cx.stop_propagation();
                        }
                    }),
                )
                .occlude();
            match position {
                DockPosition::Left => deferred(
                    handle
                        .absolute()
                        .right(-RESIZE_HANDLE_SIZE / 2.0)
                        .top(px(0.0))
                        .h_full()
                        .w(RESIZE_HANDLE_SIZE)
                        .cursor_col_resize(),
                ),
                DockPosition::Right => deferred(
                    handle
                        .absolute()
                        .left(-RESIZE_HANDLE_SIZE / 2.0)
                        .top(px(0.0))
                        .h_full()
                        .w(RESIZE_HANDLE_SIZE)
                        .cursor_col_resize(),
                ),
            }
        };

        div()
            .flex()
            .flex_col()
            .h_full()
            .overflow_hidden()
            .when(self.visible, |this| {
                this.w(self.size)
                    .bg(rgb(0x141414))
                    .border_r_1()
                    .border_color(rgb(0x2a2a2a))
                    .child(div().min_w(self.size).h_full())
                    .child(create_resize_handle())
            })
    }
}
