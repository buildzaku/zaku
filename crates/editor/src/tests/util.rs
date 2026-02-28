use gpui::AppContext;
use text::{Buffer as TextBuffer, ReplicaId};

use multi_buffer::{MultiBuffer, MultiBufferOffset};
use util::test::marked_text_offsets;

use crate::{
    DEFAULT_TAB_SIZE,
    display_map::{DisplayPoint, DisplaySnapshot, ToDisplayPoint},
};

pub fn marked_display_snapshot(
    marked_text: &str,
    cx: &mut gpui::App,
) -> (DisplaySnapshot, Vec<DisplayPoint>) {
    let (text, marker_offsets) = marked_text_offsets(marked_text);
    let text_buffer =
        cx.new(|_| TextBuffer::new(ReplicaId::LOCAL, crate::next_buffer_id(), text.as_str()));
    let multibuffer = cx.new(|cx| MultiBuffer::singleton(text_buffer, cx));
    let display_map =
        cx.new(|cx| crate::display_map::DisplayMap::new(multibuffer, DEFAULT_TAB_SIZE, cx));
    let snapshot = display_map.update(cx, |map, cx| map.snapshot(cx));
    let display_points = marker_offsets
        .into_iter()
        .map(|offset| {
            snapshot
                .buffer_snapshot()
                .offset_to_point(MultiBufferOffset(offset))
                .to_display_point(&snapshot)
        })
        .collect();
    (snapshot, display_points)
}
