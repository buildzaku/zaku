use std::{cmp, ops::Range};

use multi_buffer::{MultiBufferOffset, MultiBufferSnapshot};

use super::tab_map::Chunk;

pub struct RawChunks<'a> {
    buffer_chunks: Box<dyn Iterator<Item = &'a str> + 'a>,
    buffer_chunk: Option<&'a str>,
    buffer_chunk_offset: usize,
    offset: MultiBufferOffset,
    max_offset: MultiBufferOffset,
}

impl<'a> RawChunks<'a> {
    pub fn new(
        range: Range<MultiBufferOffset>,
        multibuffer_snapshot: &'a MultiBufferSnapshot,
    ) -> Self {
        let range = normalize_range(multibuffer_snapshot, range);
        Self {
            buffer_chunks: Box::new(multibuffer_snapshot.text_for_range(range.start..range.end)),
            buffer_chunk: None,
            buffer_chunk_offset: 0,
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
                self.buffer_chunk_offset = 0;
            }

            let chunk = self.buffer_chunk?;
            if self.buffer_chunk_offset >= chunk.len() {
                self.buffer_chunk = None;
                continue;
            }

            let remaining = &chunk[self.buffer_chunk_offset..];
            let remaining_total_bytes = self.max_offset.saturating_sub(self.offset);
            if remaining_total_bytes == 0 {
                return None;
            }

            let max_bytes = cmp::min(
                remaining.len(),
                cmp::min(rope::Chunk::MASK_BITS, remaining_total_bytes),
            );
            let split_index = floor_char_boundary(remaining, max_bytes);
            if split_index == 0 {
                self.buffer_chunk = None;
                continue;
            }

            let text = &remaining[..split_index];
            self.buffer_chunk_offset += split_index;
            if self.buffer_chunk_offset >= chunk.len() {
                self.buffer_chunk = None;
                self.buffer_chunk_offset = 0;
            }
            self.offset += split_index;

            let (chars, tabs, newlines) = compute_bitmaps(text);
            return Some(Chunk {
                text,
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

    text.chars()
        .next()
        .map(|character| character.len_utf8())
        .unwrap_or(0)
}

fn compute_bitmaps(text: &str) -> (u128, u128, u128) {
    let mut chars = 0u128;
    let mut tabs = 0u128;
    let mut newlines = 0u128;

    for (index, character) in text.char_indices() {
        if let Some(mask) = bit(index as u32) {
            chars |= mask;
            if character == '\t' {
                tabs |= mask;
            } else if character == '\n' {
                newlines |= mask;
            }
        }
    }

    (chars, tabs, newlines)
}

fn bit(shift: u32) -> Option<u128> {
    if shift >= u128::BITS {
        None
    } else {
        Some(1u128 << shift)
    }
}
