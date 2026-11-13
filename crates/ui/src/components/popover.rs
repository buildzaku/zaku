use gpui::{AnyElement, App, IntoElement, Pixels, RenderOnce, Window, prelude::*};
use smallvec::SmallVec;

use crate::{ActiveTheme, StyledExt};

pub const POPOVER_Y_PADDING: Pixels = gpui::px(8.0);

#[derive(IntoElement)]
pub struct Popover {
    children: SmallVec<[AnyElement; 2]>,
    aside: Option<AnyElement>,
}

impl RenderOnce for Popover {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        gpui::div()
            .flex()
            .gap_1()
            .child(
                gpui::div()
                    .flex()
                    .flex_col()
                    .elevation_2(cx)
                    .py(POPOVER_Y_PADDING / 2.0)
                    .child(gpui::div().children(self.children)),
            )
            .when_some(self.aside, |this, aside| {
                this.child(
                    gpui::div()
                        .flex()
                        .flex_col()
                        .elevation_2(cx)
                        .bg(cx.theme().colors().surface_background)
                        .px_1()
                        .child(aside),
                )
            })
    }
}

impl Default for Popover {
    fn default() -> Self {
        Self::new()
    }
}

impl Popover {
    pub fn new() -> Self {
        Self {
            children: SmallVec::new(),
            aside: None,
        }
    }

    pub fn aside(mut self, aside: impl IntoElement) -> Self
    where
        Self: Sized,
    {
        self.aside = Some(aside.into_element().into_any());
        self
    }
}

impl ParentElement for Popover {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}
