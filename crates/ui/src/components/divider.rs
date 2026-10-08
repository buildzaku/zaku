use gpui::{App, Hsla, IntoElement, RenderOnce, Window, prelude::*};

use crate::ActiveTheme;

#[derive(Default)]
pub enum DividerColor {
    Border,
    #[default]
    BorderVariant,
}

impl DividerColor {
    pub fn hsla(self, cx: &mut App) -> Hsla {
        match self {
            DividerColor::Border => cx.theme().colors().border,
            DividerColor::BorderVariant => cx.theme().colors().border_variant,
        }
    }
}

#[derive(IntoElement)]
pub struct Divider {
    color: DividerColor,
}

impl Divider {
    pub fn vertical() -> Self {
        Self {
            color: DividerColor::default(),
        }
    }

    pub fn color(mut self, color: DividerColor) -> Self {
        self.color = color;
        self
    }
}

impl RenderOnce for Divider {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        gpui::div().min_w_0().w_px().h_4().bg(self.color.hsla(cx))
    }
}
