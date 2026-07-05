use gpui::{AnyElement, App, IntoElement, ParentElement, SharedString, Window, prelude::*};

use crate::{Color, Icon, IconName, IconSize, ListItem, Text, TextCommon};

#[derive(IntoElement)]
pub struct ListBulletItem {
    text: SharedString,
    text_color: Option<Color>,
    children: Vec<AnyElement>,
}

impl ListBulletItem {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            text_color: None,
            children: Vec::new(),
        }
    }

    pub fn text_color(mut self, color: Color) -> Self {
        self.text_color = Some(color);
        self
    }
}

impl ParentElement for ListBulletItem {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for ListBulletItem {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let line_height = window.line_height() * 0.85;

        ListItem::new("list-item")
            .selectable(false)
            .child(
                gpui::div()
                    .flex()
                    .items_center()
                    .w_full()
                    .min_w_0()
                    .gap_1()
                    .items_start()
                    .child(
                        gpui::div()
                            .flex()
                            .items_center()
                            .h(line_height)
                            .justify_center()
                            .child(
                                Icon::new(IconName::Dash)
                                    .size(IconSize::XSmall)
                                    .color(Color::Hidden),
                            ),
                    )
                    .map(|this| {
                        if self.children.is_empty() {
                            this.child(
                                gpui::div().w_full().min_w_0().child(
                                    Text::new(self.text)
                                        .color(self.text_color.unwrap_or(Color::Default)),
                                ),
                            )
                        } else {
                            this.child(
                                gpui::div()
                                    .flex()
                                    .items_center()
                                    .gap_0p5()
                                    .flex_wrap()
                                    .children(self.children),
                            )
                        }
                    }),
            )
            .into_any_element()
    }
}
