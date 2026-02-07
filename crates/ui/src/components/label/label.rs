use gpui::{App, RenderOnce, SharedString, StyleRefinement, Window, prelude::*};

use component::{Component, ComponentScope};
use ui_macros::RegisterComponent;

use crate::{Color, LabelCommon, LabelLike, LabelSize, LineHeightStyle};

#[derive(IntoElement, RegisterComponent)]
pub struct Label {
    base: LabelLike,
    label: SharedString,
}

impl Label {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            base: LabelLike::new(),
            label: label.into(),
        }
    }

    pub fn set_text(&mut self, text: impl Into<SharedString>) {
        self.label = text.into();
    }

    pub fn truncate_start(mut self) -> Self {
        self.base = self.base.truncate_start();
        self
    }
}

impl Label {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.base.style()
    }

    gpui::margin_style_methods!({
        visibility: pub
    });
}

impl LabelCommon for Label {
    fn size(mut self, size: LabelSize) -> Self {
        self.base = self.base.size(size);
        self
    }

    fn weight(mut self, weight: gpui::FontWeight) -> Self {
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

    fn alpha(mut self, alpha: f32) -> Self {
        self.base = self.base.alpha(alpha);
        self
    }

    fn underline(mut self) -> Self {
        self.base = self.base.underline();
        self
    }

    fn truncate(mut self) -> Self {
        self.base = self.base.truncate();
        self
    }

    fn single_line(mut self) -> Self {
        self.label = SharedString::from(self.label.replace('\n', "⏎"));
        self.base = self.base.single_line();
        self
    }

    fn buffer_font(mut self, cx: &App) -> Self {
        self.base = self.base.buffer_font(cx);
        self
    }

    fn inline_code(mut self, cx: &App) -> Self {
        self.base = self.base.inline_code(cx);
        self
    }
}

impl RenderOnce for Label {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        self.base.child(self.label)
    }
}

impl Component for Label {
    fn scope() -> ComponentScope {
        ComponentScope::Typography
    }
}
