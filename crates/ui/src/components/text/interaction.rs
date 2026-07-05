use gpui::{
    App, Bounds, ClipboardItem, Context, DismissEvent, Entity, FocusHandle, Focusable, Pixels,
    Point, SharedString, Subscription, TextLayout, Window,
};
use std::ops::Range;

use crate::ContextMenu;

use super::selection::{TextSelectionPoint, TextSelectionState};

pub struct TextInteractionState<T: Copy + Ord + 'static> {
    focus_handle: FocusHandle,
    text_selection: TextSelectionState<T>,
    context_menu: Option<(Entity<ContextMenu>, Point<Pixels>, Subscription)>,
}

impl<T: Copy + Ord + 'static> TextInteractionState<T> {
    pub fn new(cx: &mut App) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            text_selection: TextSelectionState::new(),
            context_menu: None,
        }
    }

    pub fn clear_text_selection(&mut self) {
        self.text_selection.clear();
        self.context_menu.take();
    }

    pub(super) fn focus_handle(&self) -> FocusHandle {
        self.focus_handle.clone()
    }

    pub(super) fn context_menu(&self) -> Option<(Entity<ContextMenu>, Point<Pixels>)> {
        self.context_menu
            .as_ref()
            .map(|(menu, position, _)| (menu.clone(), *position))
    }

    pub(super) fn clear_text_layouts(&mut self) {
        self.text_selection.clear_layouts();
    }

    pub(super) fn set_text_selection_bounds(&mut self, bounds: Bounds<Pixels>) {
        self.text_selection.set_selection_bounds(bounds);
    }

    pub(super) fn register_text_layout(
        &mut self,
        id: T,
        text: SharedString,
        text_layout: &TextLayout,
    ) {
        self.text_selection.register_layout(id, text, text_layout);
    }

    pub(super) fn selected_range_for_text(&self, id: T, text: &str) -> Option<Range<usize>> {
        self.text_selection.selected_range_for_id(id, text)
    }

    #[cfg(test)]
    pub(super) fn position_for_text_offset(&self, id: T, offset: usize) -> Option<Point<Pixels>> {
        self.text_selection.position_for_id_offset(id, offset)
    }

    pub(super) fn begin_text_selection_at_position(
        &mut self,
        position: Point<Pixels>,
        click_count: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        self.context_menu.take();

        if self
            .text_selection
            .begin_selection_at_position(position, click_count)
        {
            cx.notify();
        }
    }

    pub(super) fn update_text_selection_at_position(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if self.text_selection.update_selection_at_position(position) {
            cx.notify();
        }
    }

    pub(super) fn end_text_selection_drag(&mut self, cx: &mut Context<Self>) {
        if self.text_selection.end_selection_drag() {
            cx.notify();
        }
    }

    pub(super) fn selected_text(
        &self,
        selection_order: &[T],
        copy_separator: &str,
        text_for_selection: &dyn Fn(T, &mut Window, &mut App) -> Option<SharedString>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<String> {
        self.text_selection
            .selected_text(selection_order, copy_separator, |id| {
                text_for_selection(id, window, cx)
            })
    }

    pub(super) fn copy_selected_text(
        &mut self,
        selection_order: &[T],
        copy_separator: &str,
        text_for_selection: &dyn Fn(T, &mut Window, &mut App) -> Option<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selected_text) = self.selected_text(
            selection_order,
            copy_separator,
            text_for_selection,
            window,
            cx,
        ) else {
            return;
        };

        cx.write_to_clipboard(ClipboardItem::new_string(selected_text));
    }

    pub(super) fn deploy_text_context_menu(
        &mut self,
        selection_order: &[T],
        copy_separator: &str,
        text_for_selection: &dyn Fn(T, &mut Window, &mut App) -> Option<SharedString>,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);

        let has_selected_text = self
            .selected_text(
                selection_order,
                copy_separator,
                text_for_selection,
                window,
                cx,
            )
            .is_some();
        let focus_handle = self.focus_handle.clone();
        let context_menu = ContextMenu::build(window, cx, move |menu, _, _| {
            menu.context(focus_handle)
                .action_disabled_when(!has_selected_text, "Copy", Box::new(actions::text::Copy))
                .action("Select All", Box::new(actions::text::SelectAll))
        });

        window.focus(&context_menu.focus_handle(cx), cx);
        let subscription = cx.subscribe(&context_menu, |this, _, _: &DismissEvent, cx| {
            this.context_menu.take();
            cx.notify();
        });
        self.context_menu = Some((context_menu, position, subscription));
        cx.notify();
    }

    pub(super) fn select_all_text(&mut self, selection_order: &[T], cx: &mut Context<Self>) {
        let Some(first_id) = selection_order.first().copied() else {
            return;
        };
        let Some(last_id) = selection_order.last().copied() else {
            return;
        };

        if !self.text_selection.has_registered_layouts() {
            return;
        }

        self.text_selection.select_all(
            TextSelectionPoint::new(first_id, 0),
            TextSelectionPoint::new(last_id, usize::MAX),
        );
        cx.notify();
    }
}
