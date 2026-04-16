use gpui::{App, IntoElement, SharedString, Window, prelude::*};

use crate::{Color, StyledTypography, TextSize};

#[derive(IntoElement)]
pub struct Link {
    text: SharedString,
    text_size: TextSize,
    color: Color,
    font_buffer: bool,
    underline: bool,
    url: String,
}

impl Link {
    pub fn new(text: impl Into<SharedString>, url: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            text_size: TextSize::Default,
            color: Color::Default,
            font_buffer: false,
            underline: true,
            url: url.into(),
        }
    }

    pub fn text_size(mut self, text_size: TextSize) -> Self {
        self.text_size = text_size;
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn font_buffer(mut self) -> Self {
        self.font_buffer = true;
        self
    }

    pub fn underline(mut self, underline: bool) -> Self {
        self.underline = underline;
        self
    }
}

impl RenderOnce for Link {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        gpui::div()
            .id(format!("{}-{}", self.text, self.url))
            .cursor_pointer()
            .text_ui_size(self.text_size, cx)
            .text_color(self.color.color(cx))
            .when(self.font_buffer, |this| this.font_buffer(cx))
            .when(self.underline, |this| this.underline())
            .child(self.text)
            .on_click(move |_, _, cx| cx.open_url(&self.url))
    }
}
