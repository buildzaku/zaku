use gpui::{App, AppContext};

use language::Buffer;
use multi_buffer::{MultiBuffer, MultiBufferOffset};
use util::test::marked_text_offsets;

use crate::{
    DEFAULT_TAB_SIZE,
    display_map::{DisplayMap, DisplayPoint, DisplaySnapshot, ToDisplayPoint},
};

pub fn marked_display_snapshot(
    marked_text: &str,
    cx: &mut App,
) -> (DisplaySnapshot, Vec<DisplayPoint>) {
    let (text, marker_offsets) = marked_text_offsets(marked_text);
    let buffer = cx.new(|cx| Buffer::local(text.as_str(), cx));
    let multibuffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let display_map = cx.new(|cx| DisplayMap::new(multibuffer, DEFAULT_TAB_SIZE, cx));
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
