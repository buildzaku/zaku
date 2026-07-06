use gpui::{App, IntoElement, Pixels, Rems, SharedString, Transformation, Window, prelude::*};

use ::svg::IconAsset;

use crate::{Color, DynamicSpacing};

#[derive(Clone, Copy, Default, PartialEq)]
pub enum IconSize {
    /// 10px
    Indicator,
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
            IconSize::Indicator => crate::rems_from_px(10.0),
            IconSize::XSmall => crate::rems_from_px(12.0),
            IconSize::Small => crate::rems_from_px(14.0),
            IconSize::Medium => crate::rems_from_px(16.0),
            IconSize::XLarge => crate::rems_from_px(48.0),
            IconSize::Custom(size) => size,
        }
    }

    pub fn square_components(&self, window: &mut Window, cx: &mut App) -> (Pixels, Pixels) {
        let icon_size = self.rems() * window.rem_size();
        let padding = match self {
            IconSize::Indicator => DynamicSpacing::Base00.px(cx),
            IconSize::XSmall | IconSize::Small | IconSize::Medium | IconSize::XLarge => {
                DynamicSpacing::Base04.px(cx)
            }
            IconSize::Custom(size) => size.to_pixels(window.rem_size()),
        };

        (icon_size, padding)
    }

    pub fn square(&self, window: &mut Window, cx: &mut App) -> Pixels {
        let (icon_size, padding) = self.square_components(window, cx);

        let size = icon_size + padding * 2.;
        let scale_factor = window.scale_factor();
        let size_f32: f32 = size.into();

        gpui::px((size_f32 * scale_factor).round() / scale_factor)
    }
}

#[derive(IntoElement)]
pub struct Icon {
    path: SharedString,
    color: Color,
    group_hover_color: Option<(SharedString, Color)>,
    size: Rems,
    transformation: Transformation,
}

impl Icon {
    pub fn new(icon: IconAsset) -> Self {
        Self {
            path: icon.path().into(),
            color: Color::default(),
            group_hover_color: None,
            size: IconSize::default().rems(),
            transformation: Transformation::default(),
        }
    }

    pub fn from_path(path: impl Into<SharedString>) -> Self {
        Self {
            path: path.into(),
            color: Color::default(),
            group_hover_color: None,
            size: IconSize::default().rems(),
            transformation: Transformation::default(),
        }
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub(crate) fn group_hover_color(
        mut self,
        group_name: impl Into<SharedString>,
        color: impl Into<Option<Color>>,
    ) -> Self {
        self.group_hover_color = color.into().map(|color| (group_name.into(), color));
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
        let group_hover_color = self
            .group_hover_color
            .map(|(group_name, color)| (group_name, color.color(cx)));

        gpui::svg()
            .with_transformation(self.transformation)
            .size(self.size)
            .flex_none()
            .path(self.path)
            .text_color(color)
            .when_some(group_hover_color, |this, (group_name, color)| {
                this.group_hover(group_name, |style| style.text_color(color))
            })
            .into_any_element()
    }
}
