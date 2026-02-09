use gpui::{MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Window, prelude::*};

use theme::ActiveTheme;

use crate::{DockPosition, DraggedDock};

const DEFAULT_DOCK_SIZE: Pixels = gpui::px(250.);
const RESIZE_HANDLE_SIZE: Pixels = gpui::px(6.);

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
            visible: false,
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
        let theme_colors = cx.theme().colors();
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
                    cx.listener(|dock, e: &MouseUpEvent, window, cx| {
                        if e.click_count == 2 {
                            dock.set_size(gpui::px(250.), window, cx);
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

        gpui::div()
            .flex()
            .flex_col()
            .h_full()
            .overflow_hidden()
            .when(self.visible, |this| {
                this.w(self.size)
                    .bg(theme_colors.surface_background)
                    .border_r_1()
                    .border_color(theme_colors.border_variant)
                    .child(gpui::div().min_w(self.size).h_full())
                    .child(create_resize_handle())
            })
    }
}
