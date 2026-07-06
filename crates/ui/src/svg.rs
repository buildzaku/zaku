use gpui::{App, IntoElement, Rems, RenderOnce, Size, Window, prelude::*};
use std::sync::Arc;

use ::svg::SvgAsset;

use crate::Color;

#[derive(IntoElement)]
pub struct Svg {
    path: Arc<str>,
    color: Color,
    size: Size<Option<Rems>>,
    aspect_ratio: f32,
}

impl Svg {
    pub fn new(svg_asset: SvgAsset, width: Rems, height: Rems) -> Self {
        Self {
            path: svg_asset.path(),
            color: Color::default(),
            size: Size {
                width: Some(width),
                height: Some(height),
            },
            aspect_ratio: svg_asset.aspect_ratio(),
        }
    }

    pub fn with_width(svg_asset: SvgAsset, width: Rems) -> Self {
        Self {
            path: svg_asset.path(),
            color: Color::default(),
            size: Size {
                width: Some(width),
                height: None,
            },
            aspect_ratio: svg_asset.aspect_ratio(),
        }
    }

    pub fn with_height(svg_asset: SvgAsset, height: Rems) -> Self {
        Self {
            path: svg_asset.path(),
            color: Color::default(),
            size: Size {
                width: None,
                height: Some(height),
            },
            aspect_ratio: svg_asset.aspect_ratio(),
        }
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn size(mut self, size: impl Into<Size<Rems>>) -> Self {
        let size = size.into();
        self.size = Size {
            width: Some(size.width),
            height: Some(size.height),
        };
        self
    }
}

impl RenderOnce for Svg {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let width = self.size.width;
        let height = self.size.height;
        let aspect_ratio = (width.is_none() || height.is_none()).then_some(self.aspect_ratio);

        gpui::svg()
            .flex_none()
            .when_some(width, |this, width| this.w(width))
            .when_some(height, |this, height| this.h(height))
            .when_some(aspect_ratio, |this, aspect_ratio| {
                this.aspect_ratio(aspect_ratio)
            })
            .path(self.path)
            .text_color(self.color.color(cx))
    }
}
