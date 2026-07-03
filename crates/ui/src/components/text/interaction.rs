use gpui::{
    App, Bounds, ClipboardItem, Context, FocusHandle, Pixels, Point, SharedString, TextLayout,
    Window,
};
use std::ops::Range;

use super::selection::{TextSelectionPoint, TextSelectionState};

pub struct TextInteractionState<T: Copy + Ord + 'static> {
    focus_handle: FocusHandle,
    text_selection: TextSelectionState<T>,
}

impl<T: Copy + Ord + 'static> TextInteractionState<T> {
    pub fn new(cx: &mut App) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            text_selection: TextSelectionState::new(),
        }
    }

    pub fn clear_text_selection(&mut self) {
        self.text_selection.clear();
    }

    pub(super) fn focus_handle(&self) -> FocusHandle {
        self.focus_handle.clone()
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

    pub(super) fn begin_text_selection_at_position(
        &mut self,
        position: Point<Pixels>,
        click_count: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);

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

    fn selected_text(
        &self,
        selection_order: &[T],
        copy_separator: &str,
        text_for_selection: &dyn Fn(T, &mut Window, &mut App) -> Option<SharedString>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<String> {
        if self.text_selection.selection_is_empty() {
            return None;
        }

        let mut selected_text = Vec::new();

        for id in selection_order {
            let Some(text) = text_for_selection(*id, window, cx) else {
                continue;
            };
            let text: &str = text.as_ref();
            let Some(range) = self.text_selection.selected_range_for_id(*id, text) else {
                continue;
            };

            if let Some(text) = text.get(range) {
                selected_text.push(text.to_string());
            }
        }

        let selected_text = selected_text.join(copy_separator);
        (!selected_text.is_empty()).then_some(selected_text)
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
