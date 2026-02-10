use gpui::{App, IntoElement, Rems, SharedString, Transformation, Window, prelude::*};

use component::{Component, ComponentScope};
use icons::IconName;
use ui_macros::RegisterComponent;

use crate::Color;

#[derive(Default, PartialEq, Copy, Clone)]
pub enum IconSize {
    /// 12px
    XSmall,
    /// 14px
    Small,
    #[default]
    /// 16px
    Medium,
    /// 48px
    XLarge,
    Custom(Rems),
}

impl IconSize {
    pub fn rems(self) -> Rems {
        match self {
            IconSize::XSmall => crate::rems_from_px(12.),
            IconSize::Small => crate::rems_from_px(14.),
            IconSize::Medium => crate::rems_from_px(16.),
            IconSize::XLarge => crate::rems_from_px(48.),
            IconSize::Custom(size) => size,
        }
    }
}

enum IconSource {
    Embedded(SharedString),
}

#[derive(IntoElement, RegisterComponent)]
pub struct Icon {
    source: IconSource,
    color: Color,
    size: Rems,
    transformation: Transformation,
}

impl Icon {
    pub fn new(icon: IconName) -> Self {
        Self {
            source: IconSource::Embedded(icon.path().into()),
            color: Color::default(),
            size: IconSize::default().rems(),
            transformation: Transformation::default(),
        }
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn size(mut self, size: IconSize) -> Self {
        self.size = size.rems();
        self
    }
}

impl RenderOnce for Icon {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let color = self.color.color(cx);
        match self.source {
            IconSource::Embedded(path) => gpui::svg()
                .with_transformation(self.transformation)
                .size(self.size)
                .flex_none()
                .path(path)
                .text_color(color)
                .into_any_element(),
        }
    }
}

impl Component for Icon {
    fn scope() -> ComponentScope {
        ComponentScope::Images
    }
}
