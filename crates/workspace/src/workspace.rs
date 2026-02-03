use gpui::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div, rgb,
};

use crate::{dock::Dock, pane::Pane, status_bar::StatusBar};

pub mod dock;
pub mod pane;
pub mod status_bar;

pub struct Workspace {
    dock: Entity<Dock>,
    pane: Entity<Pane>,
    status_bar: Entity<StatusBar>,
}

impl Workspace {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            dock: cx.new(Dock::new),
            pane: cx.new(Pane::new),
            status_bar: cx.new(StatusBar::new),
        }
    }
}

impl Render for Workspace {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .bg(rgb(0x141414))
            .text_color(rgb(0xffffff))
            .text_xs()
            .size_full()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .child(self.dock.clone())
                    .child(self.pane.clone()),
            )
            .child(self.status_bar.clone())
    }
}
