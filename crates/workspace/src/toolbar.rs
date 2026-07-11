use gpui::{
    AnyView, App, Context, Entity, EntityId, EventEmitter, IntoElement, KeyContext, ParentElement,
    Render, Styled, Window, prelude::*,
};

use theme::ActiveTheme;
use ui::{DynamicSpacing, StyledTypography};

use crate::ItemHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarItemEvent {
    ChangeLocation(ToolbarItemLocation),
}

pub trait ToolbarItemView: Render + EventEmitter<ToolbarItemEvent> {
    fn set_active_pane_item(
        &mut self,
        active_pane_item: Option<&dyn ItemHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ToolbarItemLocation;

    fn pane_focus_update(&mut self, _pane_focused: bool, _: &mut Window, _: &mut Context<Self>) {}

    fn contribute_context(&self, _context: &mut KeyContext, _cx: &App) {}
}

trait ToolbarItemViewHandle: Send {
    fn id(&self) -> EntityId;
    fn to_any(&self) -> AnyView;
    fn set_active_pane_item(
        &self,
        active_pane_item: Option<&dyn ItemHandle>,
        window: &mut Window,
        cx: &mut App,
    ) -> ToolbarItemLocation;
    fn focus_changed(&mut self, pane_focused: bool, window: &mut Window, cx: &mut App);
    fn contribute_context(&self, context: &mut KeyContext, cx: &App);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarItemLocation {
    Hidden,
    PrimaryLeft,
    PrimaryRight,
    Secondary,
}

pub struct Toolbar {
    active_item: Option<Box<dyn ItemHandle>>,
    items: Vec<(Box<dyn ToolbarItemViewHandle>, ToolbarItemLocation)>,
}

impl Toolbar {
    pub fn new() -> Self {
        Self {
            active_item: None,
            items: Vec::new(),
        }
    }

    pub fn add_item<T>(&mut self, item: Entity<T>, window: &mut Window, cx: &mut Context<Self>)
    where
        T: 'static + ToolbarItemView,
    {
        let location = item.set_active_pane_item(self.active_item.as_deref(), window, cx);
        cx.subscribe(&item, |toolbar, item, event, cx| {
            if let Some((_, current_location)) = toolbar
                .items
                .iter_mut()
                .find(|(item_handle, _)| item_handle.id() == item.entity_id())
            {
                match event {
                    ToolbarItemEvent::ChangeLocation(new_location) => {
                        if new_location != current_location {
                            *current_location = *new_location;
                            cx.notify();
                        }
                    }
                }
            }
        })
        .detach();
        self.items.push((Box::new(item), location));
        cx.notify();
    }

    pub fn set_active_item(
        &mut self,
        item: Option<&dyn ItemHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.active_item = item.map(|item| item.boxed_clone());

        for (toolbar_item, current_location) in &mut self.items {
            let new_location = toolbar_item.set_active_pane_item(item, window, cx);
            if new_location != *current_location {
                *current_location = new_location;
                cx.notify();
            }
        }
    }

    pub fn focus_changed(&mut self, focused: bool, window: &mut Window, cx: &mut Context<Self>) {
        for (toolbar_item, _) in &mut self.items {
            toolbar_item.focus_changed(focused, window, cx);
        }
    }

    pub fn item_of_type<T: 'static + ToolbarItemView>(&self) -> Option<Entity<T>> {
        self.items
            .iter()
            .find_map(|(item, _)| item.to_any().downcast().ok())
    }

    pub fn contribute_context(&self, context: &mut KeyContext, cx: &App) {
        for (item, location) in &self.items {
            if *location != ToolbarItemLocation::Hidden {
                item.contribute_context(context, cx);
            }
        }
    }

    fn has_any_visible_items(&self) -> bool {
        self.items
            .iter()
            .any(|(_, location)| *location != ToolbarItemLocation::Hidden)
    }

    fn left_items(&self) -> impl Iterator<Item = &dyn ToolbarItemViewHandle> {
        self.items.iter().filter_map(|(item, location)| {
            if *location == ToolbarItemLocation::PrimaryLeft {
                Some(item.as_ref())
            } else {
                None
            }
        })
    }

    fn right_items(&self) -> impl Iterator<Item = &dyn ToolbarItemViewHandle> {
        self.items.iter().filter_map(|(item, location)| {
            if *location == ToolbarItemLocation::PrimaryRight {
                Some(item.as_ref())
            } else {
                None
            }
        })
    }

    fn secondary_items(&self) -> impl Iterator<Item = &dyn ToolbarItemViewHandle> {
        self.items.iter().rev().filter_map(|(item, location)| {
            if *location == ToolbarItemLocation::Secondary {
                Some(item.as_ref())
            } else {
                None
            }
        })
    }
}

impl Default for Toolbar {
    fn default() -> Self {
        Self::new()
    }
}

impl Render for Toolbar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.has_any_visible_items() {
            return gpui::div();
        }

        let colors = cx.theme().colors();
        let has_left_items = self.left_items().count() > 0;
        let has_right_items = self.right_items().count() > 0;

        gpui::div()
            .group("toolbar")
            .relative()
            .flex()
            .flex_col()
            .px(DynamicSpacing::Base08.rems(cx))
            .py(DynamicSpacing::Base06.rems(cx))
            .font_ui(cx)
            .text_ui_sm(cx)
            .border_b_1()
            .border_color(colors.border_variant)
            .bg(colors.panel_background)
            .on_any_mouse_down(|_, window, _| {
                window.prevent_default();
            })
            .when(has_left_items || has_right_items, |toolbar| {
                toolbar.gap(DynamicSpacing::Base06.rems(cx)).child(
                    gpui::div()
                        .flex()
                        .items_start()
                        .justify_between()
                        .gap(DynamicSpacing::Base08.rems(cx))
                        .when(has_left_items, |row| {
                            row.child(
                                gpui::div()
                                    .min_h_6()
                                    .flex()
                                    .flex_1()
                                    .justify_start()
                                    .overflow_x_hidden()
                                    .children(self.left_items().map(|item| item.to_any())),
                            )
                        })
                        .when(has_right_items, |row| {
                            row.child(
                                gpui::div()
                                    .h_6()
                                    .flex()
                                    .flex_row_reverse()
                                    .when(has_left_items, |right_items| right_items.flex_none())
                                    .justify_end()
                                    .children(self.right_items().map(|item| item.to_any())),
                            )
                        }),
                )
            })
            .children(self.secondary_items().map(|item| item.to_any()))
    }
}

impl<T: ToolbarItemView> ToolbarItemViewHandle for Entity<T> {
    fn id(&self) -> EntityId {
        self.entity_id()
    }

    fn to_any(&self) -> AnyView {
        self.clone().into()
    }

    fn set_active_pane_item(
        &self,
        active_pane_item: Option<&dyn ItemHandle>,
        window: &mut Window,
        cx: &mut App,
    ) -> ToolbarItemLocation {
        self.update(cx, |this, cx| {
            this.set_active_pane_item(active_pane_item, window, cx)
        })
    }

    fn focus_changed(&mut self, pane_focused: bool, window: &mut Window, cx: &mut App) {
        self.update(cx, |this, cx| {
            this.pane_focus_update(pane_focused, window, cx);
            cx.notify();
        });
    }

    fn contribute_context(&self, context: &mut KeyContext, cx: &App) {
        self.read(cx).contribute_context(context, cx);
    }
}
