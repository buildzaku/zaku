use gpui::{Context, IntoElement, Render, Styled, Window, div, rgb};

pub struct Pane {}

impl Pane {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {}
    }
}

impl Render for Pane {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().flex().flex_col().size_full().bg(rgb(0x1a1a1a))
    }
}
