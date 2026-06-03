use gpui::{
    AnyView, App, Context, Decorations, Entity, IntoElement, ParentElement, Render, Styled,
    Subscription, Window, prelude::*,
};

use theme::{ActiveTheme, CLIENT_SIDE_DECORATION_ROUNDING};
use ui::{DynamicSpacing, StyledTypography};

use crate::pane::Pane;

pub trait StatusItemView: Render {
    fn set_active_pane(
        &mut self,
        active_pane: &Entity<Pane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    );
}

trait StatusItemViewHandle: Send {
    fn to_any(&self) -> AnyView;
    fn set_active_pane(&self, active_pane: &Entity<Pane>, window: &mut Window, cx: &mut App);
}

pub struct StatusBar {
    left_items: Vec<Box<dyn StatusItemViewHandle>>,
    right_items: Vec<Box<dyn StatusItemViewHandle>>,
    active_pane: Entity<Pane>,
    active_pane_subscription: Subscription,
}

impl StatusBar {
    pub fn new(active_pane: &Entity<Pane>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            left_items: Vec::new(),
            right_items: Vec::new(),
            active_pane: active_pane.clone(),
            active_pane_subscription: cx.observe_in(active_pane, window, |this, _, window, cx| {
                this.update_active_pane(window, cx);
            }),
        };
        this.update_active_pane(window, cx);
        this
    }

    pub fn add_left_item<T>(&mut self, item: Entity<T>, window: &mut Window, cx: &mut Context<Self>)
    where
        T: 'static + StatusItemView,
    {
        let active_pane = self.active_pane.clone();
        item.update(cx, |item, cx| {
            item.set_active_pane(&active_pane, window, cx);
        });
        self.left_items.push(Box::new(item));
        cx.notify();
    }

    pub fn add_right_item<T>(
        &mut self,
        item: Entity<T>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) where
        T: 'static + StatusItemView,
    {
        let active_pane = self.active_pane.clone();
        item.update(cx, |item, cx| {
            item.set_active_pane(&active_pane, window, cx);
        });
        self.right_items.push(Box::new(item));
        cx.notify();
    }

    pub fn set_active_pane(
        &mut self,
        active_pane: &Entity<Pane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.active_pane = active_pane.clone();
        self.active_pane_subscription =
            cx.observe_in(active_pane, window, |this, _, window, cx| {
                this.update_active_pane(window, cx);
            });
        self.update_active_pane(window, cx);
    }

    fn render_left_tools(&self) -> impl IntoElement {
        gpui::div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .min_w_0()
            .overflow_x_hidden()
            .children(self.left_items.iter().map(|item| item.to_any()))
    }

    fn render_right_tools(&self) -> impl IntoElement {
        gpui::div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .flex_shrink_0()
            .overflow_x_hidden()
            .children(self.right_items.iter().rev().map(|item| item.to_any()))
    }

    fn update_active_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for item in self.left_items.iter().chain(self.right_items.iter()) {
            item.set_active_pane(&self.active_pane, window, cx);
        }
    }
}

impl Render for StatusBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();

        gpui::div()
            .flex()
            .flex_row()
            .items_center()
            .w_full()
            .gap(DynamicSpacing::Base08.rems(cx))
            .px(DynamicSpacing::Base08.rems(cx))
            .py(DynamicSpacing::Base04.rems(cx))
            .font_ui(cx)
            .text_ui_sm(cx)
            .bg(colors.status_bar_background)
            .map(|this| match window.window_decorations() {
                Decorations::Server => this,
                Decorations::Client { tiling, .. } => this
                    .when(!(tiling.bottom || tiling.right), |this| {
                        this.rounded_br(CLIENT_SIDE_DECORATION_ROUNDING)
                    })
                    .when(!(tiling.bottom || tiling.left), |this| {
                        this.rounded_bl(CLIENT_SIDE_DECORATION_ROUNDING)
                    })
                    .mb(gpui::px(-1.0))
                    .mt({
                        #[cfg(target_os = "linux")]
                        let needs_wayland_scale_gap_fix = {
                            gpui::guess_compositor() == "Wayland" && window.scale_factor() != 1.0
                        };
                        #[cfg(any(target_os = "macos", target_os = "windows"))]
                        let needs_wayland_scale_gap_fix = false;

                        if needs_wayland_scale_gap_fix {
                            gpui::px(-1.0)
                        } else {
                            gpui::px(0.0)
                        }
                    })
                    .border_b(gpui::px(1.0))
                    .border_color(colors.status_bar_background),
            })
            .child(self.render_left_tools())
            .child(gpui::div().flex_1())
            .child(self.render_right_tools())
    }
}

impl<T: StatusItemView> StatusItemViewHandle for Entity<T> {
    fn to_any(&self) -> AnyView {
        self.clone().into()
    }

    fn set_active_pane(&self, active_pane: &Entity<Pane>, window: &mut Window, cx: &mut App) {
        self.update(cx, |this, cx| this.set_active_pane(active_pane, window, cx));
    }
}
