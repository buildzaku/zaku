use gpui::{
    App, Div, FontWeight, RenderOnce, SharedString, StyleRefinement, StyledText, UnderlineStyle,
    Window, prelude::*,
};

use theme::{ActiveTheme, ThemeSettings};

use crate::{Color, LineHeightStyle, StyledTypography, TextSize};

pub trait TextCommon {
    fn size(self, size: TextSize) -> Self;
    fn weight(self, weight: FontWeight) -> Self;
    fn line_height_style(self, line_height_style: LineHeightStyle) -> Self;
    fn color(self, color: Color) -> Self;
    fn strikethrough(self) -> Self;
    fn italic(self) -> Self;
    fn underline(self) -> Self;
    fn alpha(self, alpha: f32) -> Self;
    fn truncate(self) -> Self;
    fn single_line(self) -> Self;
    fn font_buffer(self, cx: &App) -> Self;
    fn inline_code(self, cx: &App) -> Self;
}

#[derive(IntoElement)]
pub struct Text {
    base: Div,
    text: SharedString,
    size: TextSize,
    weight: Option<FontWeight>,
    line_height_style: LineHeightStyle,
    color: Color,
    strikethrough: bool,
    italic: bool,
    alpha: Option<f32>,
    underline: bool,
    single_line: bool,
    truncate: bool,
    truncate_start: bool,
}

impl Text {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            base: gpui::div(),
            text: text.into(),
            size: TextSize::Default,
            weight: None,
            line_height_style: LineHeightStyle::default(),
            color: Color::Default,
            strikethrough: false,
            italic: false,
            alpha: None,
            underline: false,
            single_line: false,
            truncate: false,
            truncate_start: false,
        }
    }

    pub fn set_text(&mut self, text: impl Into<SharedString>) {
        self.text = text.into();
    }

    pub fn truncate_start(mut self) -> Self {
        self.truncate_start = true;
        self
    }

    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }

    gpui::margin_style_methods!({
        visibility: pub
    });
}

impl TextCommon for Text {
    fn size(mut self, size: TextSize) -> Self {
        self.size = size;
        self
    }

    fn weight(mut self, weight: FontWeight) -> Self {
        self.weight = Some(weight);
        self
    }

    fn line_height_style(mut self, line_height_style: LineHeightStyle) -> Self {
        self.line_height_style = line_height_style;
        self
    }

    fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    fn strikethrough(mut self) -> Self {
        self.strikethrough = true;
        self
    }

    fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    fn underline(mut self) -> Self {
        self.underline = true;
        self
    }

    fn alpha(mut self, alpha: f32) -> Self {
        self.alpha = Some(alpha);
        self
    }

    fn truncate(mut self) -> Self {
        self.truncate = true;
        self
    }

    fn single_line(mut self) -> Self {
        self.text = SharedString::from(self.text.replace('\n', "\u{23ce}"));
        self.single_line = true;
        self
    }

    fn font_buffer(mut self, cx: &App) -> Self {
        self.base = self
            .base
            .font(ThemeSettings::get_global(cx).buffer_font.clone());
        self
    }

    fn inline_code(mut self, cx: &App) -> Self {
        self.base = self
            .base
            .font(ThemeSettings::get_global(cx).buffer_font.clone())
            .bg(cx.theme().colors().element_background)
            .rounded_sm()
            .px_0p5();
        self
    }
}

impl RenderOnce for Text {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            base,
            text,
            size,
            weight,
            line_height_style,
            color,
            strikethrough,
            italic,
            alpha,
            underline,
            single_line,
            truncate,
            truncate_start,
        } = self;

        let mut color = color.color(cx);
        if let Some(alpha) = alpha {
            color.fade_out(1.0 - alpha);
        }

        base.text_ui_size(size, cx)
            .when(line_height_style == LineHeightStyle::UiLabel, |this| {
                this.line_height(gpui::relative(1.0))
            })
            .when(italic, |this| this.italic())
            .when(underline, |mut this| {
                this.text_style().underline = Some(UnderlineStyle {
                    thickness: gpui::px(1.0),
                    color: Some(cx.theme().colors().text_muted.opacity(0.4)),
                    wavy: false,
                });
                this
            })
            .when(strikethrough, |this| this.line_through())
            .when(single_line, |this| this.whitespace_nowrap())
            .when(truncate, |this| {
                this.min_w_0()
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
            })
            .when(truncate_start, |this| {
                this.min_w_0()
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis_start()
            })
            .text_color(color)
            .font_weight(weight.unwrap_or(ThemeSettings::get_global(cx).ui_font.weight))
            .child(StyledText::new(text))
    }
}
