use gpui::{
    App, FontWeight, HighlightStyle, RenderOnce, SharedString, StyledText, Window, prelude::*,
};
use std::ops::Range;

use theme::ActiveTheme;

use super::label_like::{LabelCommon, LabelLike, LabelSize, LineHeightStyle};

use crate::Color;

#[derive(IntoElement)]
pub struct HighlightedLabel {
    base: LabelLike,
    label: SharedString,
    highlight_indices: Vec<usize>,
}

impl HighlightedLabel {
    pub fn new(label: impl Into<SharedString>, mut highlight_indices: Vec<usize>) -> Self {
        let label = label.into();

        if highlight_indices
            .iter()
            .any(|index| !label.is_char_boundary(*index))
        {
            highlight_indices.clear();
        }

        Self {
            base: LabelLike::new(),
            label,
            highlight_indices,
        }
    }
}

impl LabelCommon for HighlightedLabel {
    fn size(mut self, size: LabelSize) -> Self {
        self.base = self.base.size(size);
        self
    }

    fn weight(mut self, weight: FontWeight) -> Self {
        self.base = self.base.weight(weight);
        self
    }

    fn line_height_style(mut self, line_height_style: LineHeightStyle) -> Self {
        self.base = self.base.line_height_style(line_height_style);
        self
    }

    fn color(mut self, color: Color) -> Self {
        self.base = self.base.color(color);
        self
    }

    fn strikethrough(mut self) -> Self {
        self.base = self.base.strikethrough();
        self
    }

    fn italic(mut self) -> Self {
        self.base = self.base.italic();
        self
    }

    fn underline(mut self) -> Self {
        self.base = self.base.underline();
        self
    }

    fn alpha(mut self, alpha: f32) -> Self {
        self.base = self.base.alpha(alpha);
        self
    }

    fn truncate(mut self) -> Self {
        self.base = self.base.truncate();
        self
    }

    fn single_line(mut self) -> Self {
        self.base = self.base.single_line();
        self
    }

    fn font_buffer(mut self, cx: &App) -> Self {
        self.base = self.base.font_buffer(cx);
        self
    }

    fn inline_code(mut self, cx: &App) -> Self {
        self.base = self.base.inline_code(cx);
        self
    }
}

pub fn highlight_ranges(
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

impl RenderOnce for HighlightedLabel {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let highlight_color = cx.theme().colors().text_accent;
        let highlights = highlight_ranges(
            &self.label,
            &self.highlight_indices,
            HighlightStyle {
                color: Some(highlight_color),
                ..Default::default()
            },
        );

        let mut text_style = window.text_style();
        text_style.color = self.base.color.color(cx);

        self.base
            .child(StyledText::new(self.label).with_default_highlights(&text_style, highlights))
    }
}
