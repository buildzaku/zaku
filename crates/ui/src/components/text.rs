mod interaction;
mod selectable;
mod selection;

pub use interaction::TextInteractionState;
pub use selectable::{SelectableText, SelectableTextGroup};
pub use selection::{TextSelectionPoint, TextSelectionState, paint_text_selection};

use gpui::{
    App, Bounds, Div, FontWeight, Hitbox, HitboxBehavior, Pixels, RenderOnce, SharedString,
    StyleRefinement, StyledText, TextAlign, TextLayout, UnderlineStyle, Window, prelude::*,
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

#[derive(Clone)]
struct TextStyle {
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

impl TextStyle {
    fn apply(&self, base: Div, cx: &mut App) -> Div {
        let mut color = self.color.color(cx);
        if let Some(alpha) = self.alpha {
            color.fade_out(1.0 - alpha);
        }

        base.text_ui_size(self.size, cx)
            .when(self.line_height_style == LineHeightStyle::UiLabel, |this| {
                this.line_height(gpui::relative(1.0))
            })
            .when(self.italic, |this| this.italic())
            .when(self.underline, |mut this| {
                this.text_style().underline = Some(UnderlineStyle {
                    thickness: gpui::px(1.0),
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
    }
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
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
}

#[derive(IntoElement)]
pub struct Text {
    base: Div,
    text: SharedString,
    style: TextStyle,
}

impl Text {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            base: gpui::div(),
            text: text.into(),
            style: TextStyle::default(),
        }
    }

    pub fn set_text(&mut self, text: impl Into<SharedString>) {
        self.text = text.into();
    }

    pub fn truncate_start(mut self) -> Self {
        self.style.truncate_start = true;
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

impl RenderOnce for Text {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self { base, text, style } = self;
        style.apply(base, cx).child(StyledText::new(text))
    }
}

pub fn insert_text_hitboxes(text_layout: &TextLayout, window: &mut Window) -> Vec<Hitbox> {
    let line_height = text_layout.line_height();
    let text_bounds = text_layout.bounds();
    let text_align = window.text_style().text_align;
    let mut line_origin = text_bounds.origin;
    let mut hitboxes = Vec::new();

    for line_layout in text_layout.line_layouts() {
        let unwrapped_layout = &line_layout.unwrapped_layout;
        let mut row_start_x = Pixels::ZERO;
        let mut row_top = line_origin.y;
        let row_ends = line_layout
            .wrap_boundaries()
            .iter()
            .filter_map(|wrap_boundary| {
                unwrapped_layout
                    .runs
                    .get(wrap_boundary.run_ix)
                    .and_then(|run| run.glyphs.get(wrap_boundary.glyph_ix))
                    .map(|glyph| glyph.position.x)
            })
            .chain([unwrapped_layout.width]);

        for row_end_x in row_ends {
            let row_width = row_end_x - row_start_x;
            if row_width > Pixels::ZERO {
                let row_left = match text_align {
                    TextAlign::Left => line_origin.x,
                    TextAlign::Center => {
                        (line_origin.x * 2.0 + text_bounds.size.width - row_width) / 2.0
                    }
                    TextAlign::Right => line_origin.x + text_bounds.size.width - row_width,
                };
                hitboxes.push(window.insert_hitbox(
                    Bounds::new(
                        gpui::point(row_left, row_top),
                        gpui::size(row_width, line_height),
                    ),
                    HitboxBehavior::Normal,
                ));
            }

            row_start_x = row_end_x;
            row_top += line_height;
        }

        line_origin.y += line_layout.size(line_height).height;
    }

    hitboxes
}
