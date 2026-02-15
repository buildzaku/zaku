use gpui::{AnyElement, App, IntoElement, ParentElement, SharedString, Window, prelude::*};

use crate::{ListItem, prelude::*};

#[derive(IntoElement)]
pub struct ListBulletItem {
    label: SharedString,
    label_color: Option<Color>,
    children: Vec<AnyElement>,
}

impl ListBulletItem {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            label_color: None,
            children: Vec::new(),
        }
    }

    pub fn label_color(mut self, color: Color) -> Self {
        self.label_color = Some(color);
        self
    }
}

impl ParentElement for ListBulletItem {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

impl RenderOnce for ListBulletItem {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let line_height = window.line_height() * 0.85;

        ListItem::new("list-item")
            .selectable(false)
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .gap_1()
                    .items_start()
                    .child(
                        h_flex().h(line_height).justify_center().child(
                            Icon::new(IconName::Dash)
                                .size(IconSize::XSmall)
                                .color(Color::Hidden),
                        ),
                    )
                    .map(|this| {
                        if !self.children.is_empty() {
                            this.child(h_flex().gap_0p5().flex_wrap().children(self.children))
                        } else {
                            this.child(
                                gpui::div().w_full().min_w_0().child(
                                    Label::new(self.label)
                                        .color(self.label_color.unwrap_or(Color::Default)),
                                ),
                            )
                        }
                    }),
            )
            .into_any_element()
    }
}
