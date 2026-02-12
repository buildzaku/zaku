use gpui::{
    AnyView, App, Context, Entity, IntoElement, ParentElement, Render, Styled, Subscription, Window,
};

use theme::ActiveTheme;
use ui::StyledTypography;

use crate::pane::Pane;

pub trait StatusItemView: Render {
    fn set_active_pane(&mut self, active_pane: &Entity<Pane>, cx: &mut Context<Self>);
}

trait StatusItemViewHandle: Send {
    fn to_any(&self) -> AnyView;
    fn set_active_pane(&self, active_pane: &Entity<Pane>, cx: &mut App);
}

pub struct StatusBar {
    left_items: Vec<Box<dyn StatusItemViewHandle>>,
    right_items: Vec<Box<dyn StatusItemViewHandle>>,
    active_pane: Entity<Pane>,
    _observe_active_pane: Subscription,
}

impl StatusBar {
    pub fn new(active_pane: Entity<Pane>, cx: &mut Context<Self>) -> Self {
        let observe_active_pane = cx.observe(&active_pane, |this, _, cx| {
            this.update_active_pane(cx);
        });

        Self {
            left_items: Vec::new(),
            right_items: Vec::new(),
            active_pane,
            _observe_active_pane: observe_active_pane,
        }
    }

    pub fn add_left_item<T>(&mut self, item: Entity<T>, cx: &mut Context<Self>)
    where
        T: 'static + StatusItemView,
    {
        let active_pane = self.active_pane.clone();
        item.update(cx, |item, cx| item.set_active_pane(&active_pane, cx));
        self.left_items.push(Box::new(item));
        cx.notify();
    }

    pub fn add_right_item<T>(&mut self, item: Entity<T>, cx: &mut Context<Self>)
    where
        T: 'static + StatusItemView,
    {
        let active_pane = self.active_pane.clone();
        item.update(cx, |item, cx| item.set_active_pane(&active_pane, cx));
        self.right_items.push(Box::new(item));
        cx.notify();
    }

    pub fn set_active_pane(&mut self, active_pane: Entity<Pane>, cx: &mut Context<Self>) {
        self.active_pane = active_pane.clone();
        self._observe_active_pane = cx.observe(&active_pane, |this, _, cx| {
            this.update_active_pane(cx);
        });
        self.update_active_pane(cx);
    }

    fn render_left_tools(&self) -> impl IntoElement {
        gpui::div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .overflow_x_hidden()
            .children(self.left_items.iter().map(|item| item.to_any()))
    }

    fn render_right_tools(&self) -> impl IntoElement {
        gpui::div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .overflow_x_hidden()
            .children(self.right_items.iter().rev().map(|item| item.to_any()))
    }

    fn update_active_pane(&mut self, cx: &mut Context<Self>) {
        for item in self.left_items.iter().chain(self.right_items.iter()) {
            item.set_active_pane(&self.active_pane, cx);
        }
        cx.notify();
    }
}

impl Render for StatusBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme_colors = cx.theme().colors();

        gpui::div()
            .flex()
            .flex_row()
            .items_center()
            .w_full()
            .h(gpui::px(26.))
            .px_3()
            .gap_2()
            .font_ui(cx)
            .text_ui_sm(cx)
            .bg(theme_colors.status_bar_background)
            .border_t_1()
            .border_color(theme_colors.border_variant)
            .child(self.render_left_tools())
            .child(gpui::div().flex_1())
            .child(self.render_right_tools())
    }
}

impl<T: StatusItemView> StatusItemViewHandle for Entity<T> {
    fn to_any(&self) -> AnyView {
        self.clone().into()
    }

    fn set_active_pane(&self, active_pane: &Entity<Pane>, cx: &mut App) {
        self.update(cx, |this, cx| this.set_active_pane(active_pane, cx));
    }
}
