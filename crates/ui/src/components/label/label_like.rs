use gpui::{
    AnyElement, App, Div, FontWeight, ParentElement, RenderOnce, StyleRefinement, UnderlineStyle,
    Window, prelude::*,
};
use smallvec::SmallVec;

use theme::{ActiveTheme, ThemeSettings};

use crate::{Color, StyledTypography};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy, Default)]
pub enum LabelSize {
    #[default]
    Default,
    Large,
    Small,
    XSmall,
}

#[derive(Default, PartialEq, Copy, Clone)]
pub enum LineHeightStyle {
    #[default]
    TextLabel,
    UiLabel,
}

pub trait LabelCommon {
    fn size(self, size: LabelSize) -> Self;
    fn weight(self, weight: FontWeight) -> Self;
    fn line_height_style(self, line_height_style: LineHeightStyle) -> Self;
    fn color(self, color: Color) -> Self;
    fn strikethrough(self) -> Self;
    fn italic(self) -> Self;
    fn underline(self) -> Self;
    fn alpha(self, alpha: f32) -> Self;
    fn truncate(self) -> Self;
    fn single_line(self) -> Self;
    fn buffer_font(self, cx: &App) -> Self;
    fn inline_code(self, cx: &App) -> Self;
}

#[derive(IntoElement)]
pub struct LabelLike {
    pub(super) base: Div,
    size: LabelSize,
    weight: Option<FontWeight>,
    line_height_style: LineHeightStyle,
    pub(crate) color: Color,
    strikethrough: bool,
    italic: bool,
    children: SmallVec<[AnyElement; 2]>,
    alpha: Option<f32>,
    underline: bool,
    single_line: bool,
    truncate: bool,
    truncate_start: bool,
}

impl Default for LabelLike {
    fn default() -> Self {
        Self::new()
    }
}

impl LabelLike {
    pub fn new() -> Self {
        Self {
            base: gpui::div(),
            size: LabelSize::Default,
            weight: None,
            line_height_style: LineHeightStyle::default(),
            color: Color::Default,
            strikethrough: false,
            italic: false,
            children: SmallVec::new(),
            alpha: None,
            underline: false,
            single_line: false,
            truncate: false,
            truncate_start: false,
        }
    }
}

impl LabelLike {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }

    gpui::margin_style_methods!({
        visibility: pub
    });

    pub fn truncate_start(mut self) -> Self {
        self.truncate_start = true;
        self
    }
}

impl LabelCommon for LabelLike {
    fn size(mut self, size: LabelSize) -> Self {
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
        self.single_line = true;
        self
    }

    fn buffer_font(mut self, cx: &App) -> Self {
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

impl ParentElement for LabelLike {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

impl RenderOnce for LabelLike {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let mut color = self.color.color(cx);
        if let Some(alpha) = self.alpha {
            color.fade_out(1. - alpha);
        }

        self.base
            .map(|this| match self.size {
                LabelSize::Large => this.text_ui_lg(cx),
                LabelSize::Default => this.text_ui(cx),
                LabelSize::Small => this.text_ui_sm(cx),
                LabelSize::XSmall => this.text_ui_xs(cx),
            })
            .when(self.line_height_style == LineHeightStyle::UiLabel, |this| {
                this.line_height(gpui::relative(1.))
            })
            .when(self.italic, |this| this.italic())
            .when(self.underline, |mut this| {
                this.text_style().underline = Some(UnderlineStyle {
                    thickness: gpui::px(1.),
                    color: Some(cx.theme().colors().text_muted.opacity(0.4)),
                    wavy: false,
                });
                this
            })
            .when(self.strikethrough, |this| this.line_through())
            .when(self.single_line, |this| this.whitespace_nowrap())
            .when(self.truncate, |this| {
                this.min_w_0()
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
            })
            .when(self.truncate_start, |this| {
                this.min_w_0()
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis_start()
            })
            .text_color(color)
            .font_weight(
                self.weight
                    .unwrap_or(ThemeSettings::get_global(cx).ui_font.weight),
            )
            .children(self.children)
    }
}
