use std::{cmp, ops::Range};

use language::LanguageAwareStyling;
use multi_buffer::{MultiBufferChunks, MultiBufferOffset, MultiBufferSnapshot};

use super::tab_map::Chunk;

pub(super) struct RawChunks<'a> {
    buffer_chunks: MultiBufferChunks<'a>,
    buffer_chunk: Option<language::Chunk<'a>>,
    offset: MultiBufferOffset,
    max_offset: MultiBufferOffset,
}

impl<'a> RawChunks<'a> {
    pub(super) fn new(
        range: Range<MultiBufferOffset>,
        multibuffer_snapshot: &'a MultiBufferSnapshot,
        language_aware: LanguageAwareStyling,
    ) -> Self {
        let range = normalize_range(multibuffer_snapshot, range);
        Self {
            buffer_chunks: multibuffer_snapshot.chunks(range.start..range.end, language_aware),
            buffer_chunk: None,
            offset: range.start,
            max_offset: range.end,
        }
    }
}

impl<'a> Iterator for RawChunks<'a> {
    type Item = Chunk<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.max_offset {
            return None;
        }

        loop {
            if self.buffer_chunk.is_none() {
                self.buffer_chunk = self.buffer_chunks.next();
            }

            let chunk = self.buffer_chunk.as_mut()?;
            if chunk.text.is_empty() {
                self.buffer_chunk = None;
                continue;
            }

            let remaining_total_bytes = self.max_offset.saturating_sub(self.offset);
            if remaining_total_bytes == 0 {
                return None;
            }

            let max_bytes = cmp::min(
                chunk.text.len(),
                cmp::min(text::Chunk::MASK_BITS, remaining_total_bytes),
            );
            let split_index = floor_char_boundary(chunk.text, max_bytes);
            if split_index == 0 {
                self.buffer_chunk = None;
                continue;
            }

            let (text, suffix) = chunk.text.split_at(split_index);
            let shift = u32::try_from(split_index).expect("split index should fit in u32");
            let mask = 1u128.unbounded_shl(shift).wrapping_sub(1);
            let chars = chunk.chars & mask;
            let tabs = chunk.tabs & mask;
            let newlines = chunk.newlines & mask;
            let syntax_highlight_id = chunk.syntax_highlight_id;
            chunk.text = suffix;
            chunk.chars = chunk.chars.unbounded_shr(shift);
            chunk.tabs = chunk.tabs.unbounded_shr(shift);
            chunk.newlines = chunk.newlines.unbounded_shr(shift);
            let chunk_is_empty = chunk.text.is_empty();
            self.offset += split_index;
            if chunk_is_empty {
                self.buffer_chunk = None;
            }

            return Some(Chunk {
                text,
                syntax_highlight_id,
                chars,
                tabs,
                newlines,
            });
        }
    }
}

fn normalize_range(
    snapshot: &MultiBufferSnapshot,
    range: Range<MultiBufferOffset>,
) -> Range<MultiBufferOffset> {
    let start = snapshot.clip_offset(range.start, text::Bias::Left);
    let end = snapshot.clip_offset(range.end, text::Bias::Right);
    if start <= end { start..end } else { end..start }
}

fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    index = cmp::min(index, text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }

    if index > 0 {
        return index;
    }

    text.chars().next().map_or(0, char::len_utf8)
}
