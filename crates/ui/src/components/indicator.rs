use gpui::{App, IntoElement, RenderOnce, Window, prelude::*};

use crate::Color;

#[derive(IntoElement)]
pub struct Indicator {
    pub color: Color,
}

impl Indicator {
    pub fn dot() -> Self {
        Self {
            color: Color::Default,
        }
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

impl RenderOnce for Indicator {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        gpui::div()
            .flex_none()
            .size_2()
            .rounded_full()
            .bg(self.color.color(cx))
    }
}
