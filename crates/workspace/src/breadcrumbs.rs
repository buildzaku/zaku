use gpui::{
    Context, EventEmitter, IntoElement, ParentElement, Render, Subscription, Window, prelude::*,
};

use ui::{Color, StyledTypography, Text, TextCommon};

use crate::{ItemEvent, ItemHandle, ToolbarItemEvent, ToolbarItemLocation, ToolbarItemView};

pub struct Breadcrumbs {
    active_item: Option<Box<dyn ItemHandle>>,
    active_item_subscription: Option<Subscription>,
}

impl Breadcrumbs {
    pub fn new() -> Self {
        Self {
            active_item: None,
            active_item_subscription: None,
        }
    }
}

impl Default for Breadcrumbs {
    fn default() -> Self {
        Self::new()
    }
}

impl EventEmitter<ToolbarItemEvent> for Breadcrumbs {}

impl Render for Breadcrumbs {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        const MAX_SEGMENTS: usize = 12;

        let element = gpui::div()
            .id("breadcrumb-container")
            .flex()
            .flex_grow_1()
            .h_6()
            .items_center()
            .overflow_x_scroll()
            .text_ui(cx);

        let Some(active_item) = self.active_item.as_ref() else {
            return element;
        };

        let Some(mut segments) = active_item.breadcrumbs(cx) else {
            return element;
        };

        if segments.len() > MAX_SEGMENTS {
            let prefix_end_index = MAX_SEGMENTS / 2;
            let suffix_start_index = segments.len() - MAX_SEGMENTS / 2;
            segments.splice(prefix_end_index..suffix_start_index, Some("⋯".into()));
        }

        let segment_elements = segments.into_iter().map(|segment| {
            Text::new(segment.replace('\n', " "))
                .color(Color::Muted)
                .font_buffer(cx)
                .into_any_element()
        });

        let breadcrumb_elements = itertools::intersperse_with(segment_elements, || {
            Text::new("›").color(Color::Placeholder).into_any_element()
        });

        let breadcrumbs = gpui::div()
            .flex()
            .items_center()
            .gap_1()
            .children(breadcrumb_elements);

        element.child(breadcrumbs)
    }
}

impl ToolbarItemView for Breadcrumbs {
    fn set_active_pane_item(
        &mut self,
        active_pane_item: Option<&dyn ItemHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ToolbarItemLocation {
        cx.notify();
        self.active_item = None;
        let _previous_subscription = self.active_item_subscription.take();

        let Some(item) = active_pane_item else {
            return ToolbarItemLocation::Hidden;
        };

        let this = cx.entity().downgrade();
        self.active_item_subscription = Some(item.subscribe_to_item_events(
            window,
            cx,
            Box::new(move |event, _, cx| {
                if let ItemEvent::UpdateBreadcrumbs = event
                    && let Err(error) = this.update(cx, |this, cx| {
                        cx.notify();
                        if let Some(active_item) = this.active_item.as_ref() {
                            cx.emit(ToolbarItemEvent::ChangeLocation(
                                active_item.breadcrumb_location(cx),
                            ));
                        }
                    })
                {
                    log::debug!("Failed to update breadcrumbs: {error:?}");
                }
            }),
        ));
        self.active_item = Some(item.boxed_clone());
        item.breadcrumb_location(cx)
    }
}
