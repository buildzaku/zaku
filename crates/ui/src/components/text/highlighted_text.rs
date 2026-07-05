use gpui::{
    App, Div, FontWeight, HighlightStyle, RenderOnce, SharedString, StyleRefinement, StyledText,
    Window, prelude::*,
};
use std::ops::Range;

use theme::{ActiveTheme, ThemeSettings};

use crate::{Color, LineHeightStyle, TextSize};

use super::{TextCommon, TextStyle};

#[derive(IntoElement)]
pub struct HighlightedText {
    base: Div,
    text: SharedString,
    highlight_indices: Vec<usize>,
    style: TextStyle,
}

impl HighlightedText {
    pub fn new(text: impl Into<SharedString>, mut highlight_indices: Vec<usize>) -> Self {
        let text = text.into();

        if highlight_indices
            .iter()
            .any(|index| !text.is_char_boundary(*index))
        {
            highlight_indices.clear();
        }

        Self {
            base: gpui::div(),
            text,
            highlight_indices,
            style: TextStyle::default(),
        }
    }

    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }

    gpui::margin_style_methods!({
        visibility: pub
    });
}

impl TextCommon for HighlightedText {
    fn size(mut self, size: TextSize) -> Self {
        self.style.size = size;
        self
    }

    fn weight(mut self, weight: FontWeight) -> Self {
        self.style.weight = Some(weight);
        self
    }

    fn line_height_style(mut self, line_height_style: LineHeightStyle) -> Self {
        self.style.line_height_style = line_height_style;
        self
    }

    fn color(mut self, color: Color) -> Self {
        self.style.color = color;
        self
    }

    fn strikethrough(mut self) -> Self {
        self.style.strikethrough = true;
        self
    }

    fn italic(mut self) -> Self {
        self.style.italic = true;
        self
    }

    fn underline(mut self) -> Self {
        self.style.underline = true;
        self
    }

    fn alpha(mut self, alpha: f32) -> Self {
        self.style.alpha = Some(alpha);
        self
    }

    fn truncate(mut self) -> Self {
        self.style.truncate = true;
        self
    }

    fn single_line(mut self) -> Self {
        self.text = SharedString::from(self.text.replace('\n', "\u{23ce}"));
        self.style.single_line = true;
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

fn highlight_ranges(
    text: &str,
    indices: &[usize],
    style: HighlightStyle,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let mut highlight_indices = indices.iter().copied().peekable();
    let mut highlights = Vec::new();

    while let Some(start_index) = highlight_indices.next() {
        let mut end_index = start_index;

        while let Some(character) = text.get(end_index..).and_then(|text| text.chars().next()) {
            end_index += character.len_utf8();
            if highlight_indices
                .next_if(|index| *index == end_index)
                .is_none()
            {
                break;
            }
        }

        highlights.push((start_index..end_index, style));
    }

    highlights
}

impl RenderOnce for HighlightedText {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let highlight_color = cx.theme().colors().text_accent;
        let highlights = highlight_ranges(
            &self.text,
            &self.highlight_indices,
            HighlightStyle {
                color: Some(highlight_color),
                ..Default::default()
            },
        );

        let mut text_style = window.text_style();
        text_style.color = self.style.text_color(cx);

        self.style
            .apply(self.base, cx)
            .child(StyledText::new(self.text).with_default_highlights(&text_style, highlights))
    }
}
