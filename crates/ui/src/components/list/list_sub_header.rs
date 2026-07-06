use gpui::{AnyElement, App, SharedString, Window, prelude::*};

use crate::{
    ActiveTheme, Color, DynamicSpacing, Icon, IconAsset, IconSize, Text, TextCommon, TextSize,
    Toggleable,
};

#[derive(IntoElement)]
pub struct ListSubHeader {
    text: SharedString,
    start_slot: Option<IconAsset>,
    end_slot: Option<AnyElement>,
    inset: bool,
    selected: bool,
}

impl ListSubHeader {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            start_slot: None,
            end_slot: None,
            inset: false,
            selected: false,
        }
    }

    pub fn left_icon(mut self, left_icon: Option<IconAsset>) -> Self {
        self.start_slot = left_icon;
        self
    }

    pub fn end_slot(mut self, end_slot: AnyElement) -> Self {
        self.end_slot = Some(end_slot);
        self
    }

    pub fn inset(mut self, inset: bool) -> Self {
        self.inset = inset;
        self
    }
}

impl Toggleable for ListSubHeader {
    fn toggle_state(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl RenderOnce for ListSubHeader {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        gpui::div()
            .flex()
            .items_center()
            .flex_1()
            .w_full()
            .relative()
            .pb(DynamicSpacing::Base04.rems(cx))
            .px(DynamicSpacing::Base02.rems(cx))
            .child(
                gpui::div()
                    .h_5()
                    .when(self.inset, |this| this.px_2())
                    .when(self.selected, |this| {
                        this.bg(cx.theme().colors().ghost_element_selected)
                    })
                    .flex()
                    .flex_1()
                    .w_full()
                    .gap_1()
                    .items_center()
                    .justify_between()
                    .child(
                        gpui::div()
                            .flex()
                            .gap_1()
                            .items_center()
                            .children(self.start_slot.map(|icon| {
                                Icon::new(icon).color(Color::Muted).size(IconSize::Small)
                            }))
                            .child(
                                Text::new(self.text.clone())
                                    .color(Color::Muted)
                                    .size(TextSize::Small),
                            ),
                    )
                    .children(self.end_slot),
            )
    }
}
