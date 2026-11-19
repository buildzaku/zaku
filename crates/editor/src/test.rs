pub mod editor_test_context;

use gpui::{Entity, VisualTestContext};

use actions::editor::Hover;
use multi_buffer::MultiBufferOffset;

use crate::{Editor, SelectionEffects, hover_popover};

pub fn trigger_hover(editor: &Entity<Editor>, cx: &mut VisualTestContext, offset: usize) {
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
            selections.select_ranges([MultiBufferOffset(offset)..MultiBufferOffset(offset)]);
        });
        hover_popover::hover(editor, &Hover, window, cx);
    });
}
