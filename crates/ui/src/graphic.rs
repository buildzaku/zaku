use std::sync::Arc;

use gpui::{App, IntoElement, Rems, RenderOnce, Size, Window, prelude::*};
use serde::{Deserialize, Serialize};
use strum::{EnumIter, EnumString, IntoStaticStr};

use crate::Color;

#[derive(
    Debug, PartialEq, Eq, Copy, Clone, EnumIter, EnumString, IntoStaticStr, Serialize, Deserialize,
)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum GraphicName {
    Zaku,
}

impl GraphicName {
    pub fn path(&self) -> Arc<str> {
        let file_stem: &'static str = self.into();
        format!("graphics/{file_stem}.svg").into()
    }

    fn aspect_ratio(self) -> f32 {
        match self {
            GraphicName::Zaku => 70.0 / 32.0,
        }
    }
}

#[derive(IntoElement)]
pub struct Graphic {
    path: Arc<str>,
    color: Color,
    size: Size<Option<Rems>>,
    aspect_ratio: f32,
}

impl Graphic {
    pub fn new(graphic: GraphicName, width: Rems, height: Rems) -> Self {
        Self {
            path: graphic.path(),
            color: Color::default(),
            size: Size {
                width: Some(width),
                height: Some(height),
            },
            aspect_ratio: graphic.aspect_ratio(),
        }
    }

    pub fn with_width(graphic: GraphicName, width: Rems) -> Self {
        Self {
            path: graphic.path(),
            color: Color::default(),
            size: Size {
                width: Some(width),
                height: None,
            },
            aspect_ratio: graphic.aspect_ratio(),
        }
    }

    pub fn with_height(graphic: GraphicName, height: Rems) -> Self {
        Self {
            path: graphic.path(),
            color: Color::default(),
            size: Size {
                width: None,
                height: Some(height),
            },
            aspect_ratio: graphic.aspect_ratio(),
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

impl RenderOnce for Graphic {
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
