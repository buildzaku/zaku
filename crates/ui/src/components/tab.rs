use gpui::{
    AnyElement, App, Div, ElementId, InteractiveElement, Interactivity, IntoElement, ParentElement,
    Pixels, RenderOnce, Stateful, StatefulInteractiveElement, Window, prelude::*,
};
use smallvec::SmallVec;
use std::cmp::Ordering;

use theme::ActiveTheme;

use crate::{DynamicSpacing, Toggleable, h_flex};

const START_TAB_SLOT_SIZE: Pixels = gpui::px(12.0);
const END_TAB_SLOT_SIZE: Pixels = gpui::px(14.0);

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum TabPosition {
    First,
    Middle(Ordering),
    Last,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum TabCloseSide {
    Start,
    End,
}

#[derive(IntoElement)]
pub struct Tab {
    div: Stateful<Div>,
    selected: bool,
    position: TabPosition,
    close_side: TabCloseSide,
    start_slot: Option<AnyElement>,
    end_slot: Option<AnyElement>,
    children: SmallVec<[AnyElement; 2]>,
}

impl Tab {
    pub fn new(id: impl Into<ElementId>) -> Self {
        let id = id.into();

        Self {
            div: gpui::div()
                .id(id.clone())
                .debug_selector(|| format!("TAB-{id}")),
            selected: false,
            position: TabPosition::First,
            close_side: TabCloseSide::End,
            start_slot: None,
            end_slot: None,
            children: SmallVec::new(),
        }
    }

    pub fn position(mut self, position: TabPosition) -> Self {
        self.position = position;
        self
    }

    pub fn close_side(mut self, close_side: TabCloseSide) -> Self {
        self.close_side = close_side;
        self
    }

    pub fn start_slot<E: IntoElement>(mut self, element: impl Into<Option<E>>) -> Self {
        self.start_slot = element.into().map(IntoElement::into_any_element);
        self
    }

    pub fn end_slot<E: IntoElement>(mut self, element: impl Into<Option<E>>) -> Self {
        self.end_slot = element.into().map(IntoElement::into_any_element);
        self
    }

    pub fn content_height(cx: &App) -> Pixels {
        DynamicSpacing::Base32.px(cx) - gpui::px(1.0)
    }

    pub fn container_height(cx: &App) -> Pixels {
        DynamicSpacing::Base32.px(cx)
    }
}

impl InteractiveElement for Tab {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.div.interactivity()
    }
}

impl StatefulInteractiveElement for Tab {}

impl Toggleable for Tab {
    fn toggle_state(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl ParentElement for Tab {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Tab {
    #[allow(refining_impl_trait)]
    fn render(self, _: &mut Window, cx: &mut App) -> Stateful<Div> {
        let colors = cx.theme().colors();
        let (text_color, tab_bg) = if self.selected {
            (colors.text, colors.tab_active_background)
        } else {
            (colors.text_muted, colors.tab_inactive_background)
        };

        let start_slot = h_flex()
            .size(START_TAB_SLOT_SIZE)
            .justify_center()
            .children(self.start_slot);

        let end_slot = h_flex()
            .size(END_TAB_SLOT_SIZE)
            .justify_center()
            .children(self.end_slot);

        let (start_slot, end_slot) = match self.close_side {
            TabCloseSide::End => (start_slot, end_slot),
            TabCloseSide::Start => (end_slot, start_slot),
        };

        self.div
            .h(Self::container_height(cx))
            .bg(tab_bg)
            .border_color(colors.border)
            .map(|this| match self.position {
                TabPosition::First => {
                    if self.selected {
                        this.pl_px().border_r_1().pb_px()
                    } else {
                        this.pl_px().pr_px().border_b_1()
                    }
                }
                TabPosition::Last => {
                    if self.selected {
                        this.border_l_1().border_r_1().pb_px()
                    } else {
                        this.pl_px().border_b_1().border_r_1()
                    }
                }
                TabPosition::Middle(Ordering::Equal) => this.border_l_1().border_r_1().pb_px(),
                TabPosition::Middle(Ordering::Less) => this.border_l_1().pr_px().border_b_1(),
                TabPosition::Middle(Ordering::Greater) => this.border_r_1().pl_px().border_b_1(),
            })
            .cursor_pointer()
            .child(
                h_flex()
                    .group("")
                    .relative()
                    .h(Self::content_height(cx))
                    .px(DynamicSpacing::Base04.px(cx))
                    .gap(DynamicSpacing::Base04.rems(cx))
                    .text_color(text_color)
                    .child(start_slot)
                    .children(self.children)
                    .child(end_slot),
            )
    }
}
