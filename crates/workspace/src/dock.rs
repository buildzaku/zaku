use gpui::{Context, IntoElement, Pixels, Render, Styled, Window, div, px, rgb};

pub struct Dock {
    size: Pixels,
}

impl Dock {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self { size: px(250.0) }
    }
}

impl Render for Dock {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .w(self.size)
            .h_full()
            .bg(rgb(0x141414))
            .border_r_1()
            .border_color(rgb(0x2a2a2a))
    }
}
