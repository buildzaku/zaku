use gpui::{Context, IntoElement, Render, Styled, Window, div, px, rgb};

pub struct StatusBar {}

impl StatusBar {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {}
    }
}

impl Render for StatusBar {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .w_full()
            .h(px(26.))
            .bg(rgb(0x141414))
            .border_t_1()
            .border_color(rgb(0x2a2a2a))
    }
}
