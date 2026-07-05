use gpui::{App, IntoElement, Rems, RenderOnce, Window, prelude::*};

use crate::Color;

#[derive(IntoElement)]
pub struct Indicator {
    pub color: Color,
    size: Rems,
}

impl Indicator {
    pub fn dot() -> Self {
        Self {
            color: Color::Default,
            size: crate::rems_from_px(8.0),
        }
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn size(mut self, size: Rems) -> Self {
        self.size = size;
        self
    }
}

impl RenderOnce for Indicator {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        gpui::div()
            .flex_none()
            .size(self.size)
            .rounded_full()
            .bg(self.color.color(cx))
    }
}
