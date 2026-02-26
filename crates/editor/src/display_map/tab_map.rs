use std::{cmp, mem, num::NonZeroU32, ops::Range};
use text::{Bias, Edit as TextEdit, Point};

use multi_buffer::{MultiBufferOffset, MultiBufferRow, MultiBufferSnapshot};

use super::raw_chunks::RawChunks;

const MAX_EXPANSION_COLUMN: u32 = 256;

// Handles a tab width <= 128
const SPACES: &[u8; text::Chunk::MASK_BITS] = &[b' '; text::Chunk::MASK_BITS];
const MAX_TABS: NonZeroU32 = NonZeroU32::new(SPACES.len() as u32).expect("non-zero tab width");

#[derive(Clone, Debug, Default)]
pub struct Chunk<'a> {
    pub text: &'a str,
    pub chars: u128,
    pub tabs: u128,
    pub newlines: u128,
}

/// Keeps track of hard tabs in a text buffer.
pub struct TabMap(TabSnapshot);

impl TabMap {
    pub fn new(buffer_snapshot: MultiBufferSnapshot, tab_size: NonZeroU32) -> (Self, TabSnapshot) {
        let snapshot = TabSnapshot {
            buffer_snapshot,
            tab_size: tab_size.min(MAX_TABS),
            max_expansion_column: MAX_EXPANSION_COLUMN,
            version: 0,
        };
        (Self(snapshot.clone()), snapshot)
    }

    pub fn sync(
        &mut self,
        buffer_snapshot: MultiBufferSnapshot,
        mut buffer_edits: Vec<TextEdit<MultiBufferOffset>>,
        tab_size: NonZeroU32,
    ) -> (TabSnapshot, Vec<TabEdit>) {
        let old_snapshot = &mut self.0;
        let mut new_snapshot = TabSnapshot {
            buffer_snapshot,
            tab_size: tab_size.min(MAX_TABS),
            max_expansion_column: old_snapshot.max_expansion_column,
            version: old_snapshot.version,
        };

        if old_snapshot.buffer_snapshot.edit_count() != new_snapshot.buffer_snapshot.edit_count() {
            new_snapshot.version += 1;
        }

        let tab_edits = if old_snapshot.tab_size == new_snapshot.tab_size {
            // Expand each edit to include the next tab on the same line as the edit,
            // and any subsequent tabs on that line that moved across the tab expansion
            // boundary.
            for buffer_edit in &mut buffer_edits {
                let old_end = old_snapshot
                    .buffer_snapshot
                    .offset_to_point(buffer_edit.old.end);
                let old_max_point = old_snapshot.buffer_snapshot.max_point();
                let old_end_row_successor = Point::new(old_end.row.saturating_add(1), 0);
                let old_end_row_successor = if old_end_row_successor <= old_max_point {
                    old_end_row_successor
                } else {
                    old_max_point
                };
                let old_end_row_successor_offset = old_snapshot
                    .buffer_snapshot
                    .point_to_offset(old_end_row_successor);

                let new_end = new_snapshot
                    .buffer_snapshot
                    .offset_to_point(buffer_edit.new.end);

                let mut offset_from_edit = 0;
                let mut first_tab_offset = None;
                let mut last_tab_with_changed_expansion_offset = None;
                'outer: for chunk in
                    old_snapshot.raw_chunks(buffer_edit.old.end..old_end_row_successor_offset)
                {
                    let mut remaining_tabs = chunk.tabs;
                    while remaining_tabs != 0 {
                        let tab_index = remaining_tabs.trailing_zeros();
                        let offset_from_edit = offset_from_edit + tab_index;
                        if first_tab_offset.is_none() {
                            first_tab_offset = Some(offset_from_edit);
                        }

                        let old_column = old_end.column + offset_from_edit;
                        let new_column = new_end.column + offset_from_edit;
                        let was_expanded = old_column < old_snapshot.max_expansion_column;
                        let is_expanded = new_column < new_snapshot.max_expansion_column;
                        if was_expanded != is_expanded {
                            last_tab_with_changed_expansion_offset = Some(offset_from_edit);
                        } else if !was_expanded && !is_expanded {
                            break 'outer;
                        }

                        remaining_tabs &= remaining_tabs - 1;
                    }

                    offset_from_edit += chunk.text.len() as u32;
                    if old_end.column + offset_from_edit >= old_snapshot.max_expansion_column
                        && new_end.column + offset_from_edit >= new_snapshot.max_expansion_column
                    {
                        break;
                    }
                }

                if let Some(offset) = last_tab_with_changed_expansion_offset.or(first_tab_offset) {
                    buffer_edit.old.end += offset as usize + 1;
                    buffer_edit.new.end += offset as usize + 1;
                }
            }

            let old_alloc_ptr = buffer_edits.as_ptr();

            // Combine any edits that overlap due to the expansion.
            let mut buffer_edits = buffer_edits.into_iter();

            if let Some(mut first_edit) = buffer_edits.next() {
                // This code relies on reusing allocations from the Vec<_> - at the time of writing .flatten() prevents them.
                #[allow(clippy::filter_map_identity)]
                let mut edits: Vec<_> = buffer_edits
                    .scan(&mut first_edit, |state, edit| {
                        if state.old.end >= edit.old.start {
                            state.old.end = edit.old.end;
                            state.new.end = edit.new.end;
                            Some(None) // Skip this edit, it's merged
                        } else {
                            let next_state = edit;
                            let result = Some(Some((*state).clone())); // Yield the previous edit
                            **state = next_state;
                            result
                        }
                    })
                    .filter_map(|edit| edit)
                    .collect();
                edits.push(first_edit);
                debug_assert_eq!(
                    edits.as_ptr(),
                    old_alloc_ptr,
                    "buffer edits were reallocated"
                );

                edits
                    .into_iter()
                    .map(|buffer_edit| {
                        let old_start = old_snapshot
                            .buffer_snapshot
                            .offset_to_point(buffer_edit.old.start);
                        let old_end = old_snapshot
                            .buffer_snapshot
                            .offset_to_point(buffer_edit.old.end);
                        let new_start = new_snapshot
                            .buffer_snapshot
                            .offset_to_point(buffer_edit.new.start);
                        let new_end = new_snapshot
                            .buffer_snapshot
                            .offset_to_point(buffer_edit.new.end);
                        TabEdit {
                            old: old_snapshot.point_to_tab_point(old_start, Bias::Left)
                                ..old_snapshot.point_to_tab_point(old_end, Bias::Right),
                            new: new_snapshot.point_to_tab_point(new_start, Bias::Left)
                                ..new_snapshot.point_to_tab_point(new_end, Bias::Right),
                        }
                    })
                    .collect()
            } else {
                Vec::new()
            }
        } else {
            new_snapshot.version += 1;
            vec![TabEdit {
                old: TabPoint::zero()..old_snapshot.max_point(),
                new: TabPoint::zero()..new_snapshot.max_point(),
            }]
        };

        *old_snapshot = new_snapshot;
        (old_snapshot.clone(), tab_edits)
    }
}

#[derive(Clone)]
pub struct TabSnapshot {
    pub buffer_snapshot: MultiBufferSnapshot,
    pub tab_size: NonZeroU32,
    pub max_expansion_column: u32,
    pub version: usize,
}

impl TabSnapshot {
    pub fn buffer_snapshot(&self) -> &MultiBufferSnapshot {
        &self.buffer_snapshot
    }

    pub fn line_len(&self, row: u32) -> u32 {
        let max_point = self.max_point();
        if row < max_point.row() {
            let buffer_line_len = self.buffer_snapshot.line_len(MultiBufferRow(row));
            self.point_to_tab_point(Point::new(row, buffer_line_len), Bias::Left)
                .column()
        } else {
            max_point.column()
        }
    }

    pub(crate) fn chunks<'a>(&'a self, range: Range<TabPoint>) -> TabChunks<'a> {
        let (input_start, expanded_char_column, to_next_stop) =
            self.tab_point_to_buffer_point(range.start, Bias::Left);
        let input_column = input_start.column;
        let mut input_start = self.buffer_snapshot.point_to_offset(input_start);
        let mut input_end = self
            .buffer_snapshot
            .point_to_offset(self.tab_point_to_buffer_point(range.end, Bias::Right).0);
        if input_end < input_start {
            mem::swap(&mut input_start, &mut input_end);
        }

        let to_next_stop = if range.start.0 + Point::new(0, to_next_stop) > range.end.0 {
            range.end.column() - range.start.column()
        } else {
            to_next_stop
        };

        TabChunks {
            raw_chunks: self.raw_chunks(input_start..input_end),
            input_column,
            column: expanded_char_column,
            max_expansion_column: self.max_expansion_column,
            output_position: range.start.0,
            max_output_position: range.end.0,
            tab_size: self.tab_size,
            chunk: Chunk {
                // Safety: `SPACES` is ASCII-only; any sub-slice is valid UTF-8.
                text: unsafe { std::str::from_utf8_unchecked(&SPACES[..to_next_stop as usize]) },
                chars: bitmask_for_len(to_next_stop),
                ..Default::default()
            },
            inside_leading_tab: to_next_stop > 0,
        }
    }

    pub fn max_point(&self) -> TabPoint {
        self.point_to_tab_point(self.buffer_snapshot.max_point(), Bias::Left)
    }

    pub fn clip_point(&self, point: TabPoint, bias: Bias) -> TabPoint {
        self.point_to_tab_point(self.tab_point_to_point(point, bias), bias)
    }

    pub fn point_to_tab_point(&self, point: Point, bias: Bias) -> TabPoint {
        let point = self.buffer_snapshot.clip_point(point, bias);
        let row = point.row;
        let line_start = Point::new(row, 0);
        let line_start_offset = self.buffer_snapshot.point_to_offset(line_start);
        let point_offset = self.buffer_snapshot.point_to_offset(point);

        let chunks = self.raw_chunks(line_start_offset..point_offset);
        let tab_cursor = TabStopCursor::new(chunks);
        let expanded = self.expand_tabs(tab_cursor, point.column);
        TabPoint::new(row, expanded)
    }

    pub fn tab_point_to_point(&self, point: TabPoint, bias: Bias) -> Point {
        self.buffer_snapshot
            .clip_point(self.tab_point_to_buffer_point(point, bias).0, bias)
    }

    fn tab_point_to_buffer_point(&self, output: TabPoint, bias: Bias) -> (Point, u32, u32) {
        let max_buffer_point = self.buffer_snapshot.max_point();
        let row = output.row().min(max_buffer_point.row);

        let line_start = Point::new(row, 0);
        let line_start_offset = self.buffer_snapshot.point_to_offset(line_start);
        let line_end = Point::new(row, self.buffer_snapshot.line_len(MultiBufferRow(row)));
        let line_end_offset = self.buffer_snapshot.point_to_offset(line_end);

        let chunks = self.raw_chunks(line_start_offset..line_end_offset);

        let tab_cursor = TabStopCursor::new(chunks);
        let expanded = output.column();
        let (collapsed, expanded_char_column, to_next_stop) =
            self.collapse_tabs(tab_cursor, expanded, bias);

        (
            Point::new(row, collapsed),
            expanded_char_column,
            to_next_stop,
        )
    }

    fn raw_chunks<'a>(&'a self, range: Range<MultiBufferOffset>) -> RawChunks<'a> {
        RawChunks::new(range, &self.buffer_snapshot)
    }

    fn expand_tabs<'a, I>(&self, mut cursor: TabStopCursor<'a, I>, column: u32) -> u32
    where
        I: Iterator<Item = Chunk<'a>>,
    {
        let tab_size = self.tab_size.get();

        let end_column = column.min(self.max_expansion_column);
        let mut seek_target = end_column;
        let mut tab_count = 0;
        let mut expanded_tab_len = 0;

        while let Some(tab_stop) = cursor.seek(seek_target) {
            let expanded_chars_old = tab_stop.char_offset + expanded_tab_len - tab_count;
            let tab_len = tab_size - ((expanded_chars_old - 1) % tab_size);
            tab_count += 1;
            expanded_tab_len += tab_len;

            seek_target = end_column.saturating_sub(cursor.byte_offset());
        }

        let left_over_char_bytes = if !cursor.is_char_boundary() {
            cursor.bytes_until_next_char().unwrap_or(0) as u32
        } else {
            0
        };

        let collapsed_bytes = cursor.byte_offset() + left_over_char_bytes;
        let expanded_bytes =
            cursor.byte_offset() + expanded_tab_len - tab_count + left_over_char_bytes;

        expanded_bytes + column.saturating_sub(collapsed_bytes)
    }

    fn collapse_tabs<'a, I>(
        &self,
        mut cursor: TabStopCursor<'a, I>,
        column: u32,
        bias: Bias,
    ) -> (u32, u32, u32)
    where
        I: Iterator<Item = Chunk<'a>>,
    {
        let tab_size = self.tab_size.get();
        let mut collapsed_column = column;
        let mut seek_target = column.min(self.max_expansion_column);
        let mut tab_count = 0;
        let mut expanded_tab_len = 0;

        while let Some(tab_stop) = cursor.seek(seek_target) {
            let expanded_chars_old = tab_stop.char_offset + expanded_tab_len - tab_count;
            let tab_len = tab_size - ((expanded_chars_old - 1) % tab_size);
            tab_count += 1;
            expanded_tab_len += tab_len;

            let expanded_bytes = tab_stop.byte_offset + expanded_tab_len - tab_count;

            if expanded_bytes > column {
                let mut expanded_chars = tab_stop.char_offset + expanded_tab_len - tab_count;
                expanded_chars -= expanded_bytes - column;
                return match bias {
                    Bias::Left => (
                        cursor.byte_offset().saturating_sub(1),
                        expanded_chars,
                        expanded_bytes - column,
                    ),
                    Bias::Right => (cursor.byte_offset(), expanded_chars, 0),
                };
            } else {
                collapsed_column = collapsed_column - tab_len + 1;
                seek_target = (collapsed_column.saturating_sub(cursor.byte_offset())).min(
                    self.max_expansion_column
                        .saturating_sub(cursor.byte_offset()),
                );
            }
        }

        let collapsed_bytes = cursor.byte_offset();
        let expanded_bytes = cursor.byte_offset() + expanded_tab_len - tab_count;
        let expanded_chars = cursor.char_offset() + expanded_tab_len - tab_count;
        (
            collapsed_bytes + column.saturating_sub(expanded_bytes),
            expanded_chars,
            0,
        )
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, Ord, PartialOrd, PartialEq)]
pub struct TabPoint(pub Point);

impl TabPoint {
    pub fn new(row: u32, column: u32) -> Self {
        Self(Point::new(row, column))
    }

    pub fn zero() -> Self {
        Self::new(0, 0)
    }

    pub fn row(self) -> u32 {
        self.0.row
    }

    pub fn column(self) -> u32 {
        self.0.column
    }
}

impl From<Point> for TabPoint {
    fn from(point: Point) -> Self {
        Self(point)
    }
}

pub type TabEdit = TextEdit<TabPoint>;

pub struct TabChunks<'a> {
    max_expansion_column: u32,
    max_output_position: Point,
    tab_size: NonZeroU32,
    raw_chunks: RawChunks<'a>,
    chunk: Chunk<'a>,
    column: u32,
    output_position: Point,
    input_column: u32,
    inside_leading_tab: bool,
}

impl<'a> Iterator for TabChunks<'a> {
    type Item = Chunk<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.chunk.text.is_empty() {
            if let Some(chunk) = self.raw_chunks.next() {
                self.chunk = chunk;
                if self.inside_leading_tab {
                    self.chunk.text = &self.chunk.text[1..];
                    self.chunk.tabs >>= 1;
                    self.chunk.chars >>= 1;
                    self.chunk.newlines >>= 1;
                    self.inside_leading_tab = false;
                    self.input_column += 1;
                }
            } else {
                return None;
            }
        }

        let first_tab_idx = if self.chunk.tabs != 0 {
            self.chunk.tabs.trailing_zeros() as usize
        } else {
            self.chunk.text.len()
        };

        if first_tab_idx == 0 {
            self.chunk.text = &self.chunk.text[1..];
            self.chunk.tabs >>= 1;
            self.chunk.chars >>= 1;
            self.chunk.newlines >>= 1;

            let tab_size = if self.input_column < self.max_expansion_column {
                self.tab_size.get()
            } else {
                1
            };
            let mut len = tab_size - self.column % tab_size;
            let next_output_position = cmp::min(
                self.output_position + Point::new(0, len),
                self.max_output_position,
            );
            len = next_output_position.column - self.output_position.column;
            self.column += len;
            self.input_column += 1;
            self.output_position = next_output_position;

            return Some(Chunk {
                // Safety: `SPACES` is ASCII-only; any sub-slice is valid UTF-8.
                text: unsafe { std::str::from_utf8_unchecked(&SPACES[..len as usize]) },
                chars: bitmask_for_len(len),
                tabs: 0,
                newlines: 0,
            });
        }

        let prefix_len = first_tab_idx;
        let (prefix, suffix) = self.chunk.text.split_at(prefix_len);

        let mask = 1u128.unbounded_shl(prefix_len as u32).wrapping_sub(1);
        let prefix_chars = self.chunk.chars & mask;
        let prefix_tabs = self.chunk.tabs & mask;
        let prefix_newlines = self.chunk.newlines & mask;

        self.chunk.text = suffix;
        self.chunk.tabs = self.chunk.tabs.unbounded_shr(prefix_len as u32);
        self.chunk.chars = self.chunk.chars.unbounded_shr(prefix_len as u32);
        self.chunk.newlines = self.chunk.newlines.unbounded_shr(prefix_len as u32);

        let newline_count = prefix_newlines.count_ones();
        if newline_count > 0 {
            let last_newline_bit = 128 - prefix_newlines.leading_zeros();
            let chars_after_last_newline =
                prefix_chars.unbounded_shr(last_newline_bit).count_ones();
            let bytes_after_last_newline = prefix_len as u32 - last_newline_bit;

            self.column = chars_after_last_newline;
            self.input_column = bytes_after_last_newline;
            self.output_position = Point::new(
                self.output_position.row + newline_count,
                bytes_after_last_newline,
            );
        } else {
            let char_count = prefix_chars.count_ones();
            self.column += char_count;
            if !self.inside_leading_tab {
                self.input_column += prefix_len as u32;
            }
            self.output_position.column += prefix_len as u32;
        }

        Some(Chunk {
            text: prefix,
            chars: prefix_chars,
            tabs: prefix_tabs,
            newlines: prefix_newlines,
        })
    }
}

struct TabStopCursor<'a, I>
where
    I: Iterator<Item = Chunk<'a>>,
{
    chunks: I,
    byte_offset: u32,
    char_offset: u32,
    current_chunk: Option<(Chunk<'a>, u32)>,
}

impl<'a, I> TabStopCursor<'a, I>
where
    I: Iterator<Item = Chunk<'a>>,
{
    fn new(chunks: impl IntoIterator<Item = Chunk<'a>, IntoIter = I>) -> Self {
        Self {
            chunks: chunks.into_iter(),
            byte_offset: 0,
            char_offset: 0,
            current_chunk: None,
        }
    }

    fn bytes_until_next_char(&self) -> Option<usize> {
        self.current_chunk.as_ref().and_then(|(chunk, index)| {
            let mut index = *index;
            let mut diff = 0;
            while index > 0 && chunk.chars & (1u128.unbounded_shl(index)) == 0 {
                index -= 1;
                diff += 1;
            }

            if chunk.chars & (1 << index) != 0 {
                Some(
                    (chunk.text[index as usize..].chars().next()?)
                        .len_utf8()
                        .saturating_sub(diff),
                )
            } else {
                None
            }
        })
    }

    fn is_char_boundary(&self) -> bool {
        self.current_chunk
            .as_ref()
            .is_some_and(|(chunk, index)| (chunk.chars & 1u128.unbounded_shl(*index)) != 0)
    }

    /// `distance`: length to move forward while searching for the next tab stop.
    fn seek(&mut self, distance: u32) -> Option<TabStop> {
        if distance == 0 {
            return None;
        }

        let mut traversed = 0;

        while let Some((mut chunk, chunk_position)) = self
            .current_chunk
            .take()
            .or_else(|| self.chunks.next().zip(Some(0)))
        {
            if chunk.tabs == 0 {
                let chunk_distance = chunk.text.len() as u32 - chunk_position;
                if chunk_distance + traversed >= distance {
                    let overshoot = traversed.abs_diff(distance);

                    self.byte_offset += overshoot;
                    self.char_offset += get_char_offset(
                        chunk_position..(chunk_position + overshoot).saturating_sub(1),
                        chunk.chars,
                    );

                    if chunk_position + overshoot < 128 {
                        self.current_chunk = Some((chunk, chunk_position + overshoot));
                    }

                    return None;
                }

                self.byte_offset += chunk_distance;
                self.char_offset += get_char_offset(
                    chunk_position..(chunk_position + chunk_distance).saturating_sub(1),
                    chunk.chars,
                );
                traversed += chunk_distance;
                continue;
            }

            let tab_position = chunk.tabs.trailing_zeros() + 1;

            if traversed + tab_position - chunk_position > distance {
                let cursor_position = traversed.abs_diff(distance);

                self.char_offset += get_char_offset(
                    chunk_position..(chunk_position + cursor_position - 1),
                    chunk.chars,
                );
                self.current_chunk = Some((chunk, cursor_position + chunk_position));
                self.byte_offset += cursor_position;

                return None;
            }

            self.byte_offset += tab_position - chunk_position;
            self.char_offset += get_char_offset(chunk_position..(tab_position - 1), chunk.chars);

            let tab_stop = TabStop {
                char_offset: self.char_offset,
                byte_offset: self.byte_offset,
            };

            chunk.tabs = (chunk.tabs - 1) & chunk.tabs;

            if tab_position as usize != chunk.text.len() {
                self.current_chunk = Some((chunk, tab_position));
            }

            return Some(tab_stop);
        }

        None
    }

    fn byte_offset(&self) -> u32 {
        self.byte_offset
    }

    fn char_offset(&self) -> u32 {
        self.char_offset
    }
}

#[inline(always)]
fn get_char_offset(range: Range<u32>, bit_map: u128) -> u32 {
    if range.start == range.end {
        return if (1u128 << range.start) & bit_map == 0 {
            0
        } else {
            1
        };
    }
    let end_shift = 127u128 - range.end as u128;
    let mut bit_mask = (u128::MAX >> range.start) << range.start;
    bit_mask = (bit_mask << end_shift) >> end_shift;
    let masked = bit_map & bit_mask;

    masked.count_ones()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TabStop {
    char_offset: u32,
    byte_offset: u32,
}

fn bitmask_for_len(len: u32) -> u128 {
    if len == 0 {
        0
    } else if len >= 128 {
        u128::MAX
    } else {
        (1u128 << len) - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use gpui::AppContext;
    use text::{Buffer as TextBuffer, BufferId, ReplicaId};

    use multi_buffer::MultiBuffer;

    fn expected_collapse_tabs(
        tab_snapshot: &TabSnapshot,
        chars: impl Iterator<Item = char>,
        column: u32,
        bias: Bias,
    ) -> (u32, u32, u32) {
        let tab_size = tab_snapshot.tab_size.get();

        let mut expanded_bytes = 0;
        let mut expanded_chars = 0;
        let mut collapsed_bytes = 0;
        for character in chars {
            if expanded_bytes >= column {
                break;
            }
            if collapsed_bytes >= tab_snapshot.max_expansion_column {
                break;
            }

            if character == '\t' {
                let tab_length = tab_size - (expanded_chars % tab_size);
                expanded_chars += tab_length;
                expanded_bytes += tab_length;
                if expanded_bytes > column {
                    expanded_chars -= expanded_bytes - column;
                    return match bias {
                        Bias::Left => (collapsed_bytes, expanded_chars, expanded_bytes - column),
                        Bias::Right => (collapsed_bytes + 1, expanded_chars, 0),
                    };
                }
            } else {
                expanded_chars += 1;
                expanded_bytes += character.len_utf8() as u32;
            }

            if expanded_bytes > column && matches!(bias, Bias::Left) {
                expanded_chars -= 1;
                break;
            }

            collapsed_bytes += character.len_utf8() as u32;
        }

        (
            collapsed_bytes + column.saturating_sub(expanded_bytes),
            expanded_chars,
            0,
        )
    }

    fn expected_expand_tabs(
        tab_snapshot: &TabSnapshot,
        chars: impl Iterator<Item = char>,
        column: u32,
    ) -> u32 {
        let tab_size = tab_snapshot.tab_size.get();

        let mut expanded_chars = 0;
        let mut expanded_bytes = 0;
        let mut collapsed_bytes = 0;
        let end_column = column.min(tab_snapshot.max_expansion_column);
        for character in chars {
            if collapsed_bytes >= end_column {
                break;
            }
            if character == '\t' {
                let tab_length = tab_size - expanded_chars % tab_size;
                expanded_bytes += tab_length;
                expanded_chars += tab_length;
            } else {
                expanded_bytes += character.len_utf8() as u32;
                expanded_chars += 1;
            }
            collapsed_bytes += character.len_utf8() as u32;
        }

        expanded_bytes + column.saturating_sub(collapsed_bytes)
    }

    fn expected_buffer_point(
        tab_snapshot: &TabSnapshot,
        output: TabPoint,
        bias: Bias,
    ) -> (Point, u32, u32) {
        let max_buffer_point = tab_snapshot.buffer_snapshot.max_point();
        let row = output.row().min(max_buffer_point.row);
        let chars = tab_snapshot.buffer_snapshot.chars_at(Point::new(row, 0));
        let expanded = output.column();
        let (collapsed, expanded_char_column, to_next_stop) =
            expected_collapse_tabs(tab_snapshot, chars, expanded, bias);
        (
            Point::new(row, collapsed),
            expanded_char_column,
            to_next_stop,
        )
    }

    fn tab_snapshot_for_text(text: &str, cx: &mut gpui::App) -> TabSnapshot {
        let buffer_id = BufferId::new(1).expect("buffer id must be valid");
        let text_buffer = cx.new(|_| TextBuffer::new(ReplicaId::LOCAL, buffer_id, text));
        let multibuffer = cx.new(|cx| MultiBuffer::singleton(text_buffer, cx));
        let tab_size = NonZeroU32::new(4).expect("tab size must be non-zero");
        let (_, tab_snapshot) = TabMap::new(multibuffer.read(cx).snapshot(cx), tab_size);
        tab_snapshot
    }

    fn tab_snapshot_text(tab_snapshot: &TabSnapshot) -> String {
        tab_snapshot
            .chunks(TabPoint::zero()..tab_snapshot.max_point())
            .map(|chunk| chunk.text)
            .collect()
    }

    #[gpui::test]
    fn test_long_lines(cx: &mut gpui::App) {
        let max_expansion_column = 12;
        let input = "A\tBC\tDEF\tG\tHI\tJ\tK\tL\tM";
        let output = "A   BC  DEF G   HI J K L M";

        let mut tab_snapshot = tab_snapshot_for_text(input, cx);
        tab_snapshot.max_expansion_column = max_expansion_column;
        assert_eq!(tab_snapshot_text(&tab_snapshot), output);

        for (index, character) in input.char_indices() {
            assert_eq!(
                tab_snapshot
                    .chunks(TabPoint::new(0, index as u32)..tab_snapshot.max_point())
                    .map(|chunk| chunk.text)
                    .collect::<String>(),
                &output[index..],
                "text from index {index}"
            );

            if character != '\t' {
                let input_point = Point::new(0, index as u32);
                let output_column = output
                    .find(character)
                    .expect("character from input must exist in output")
                    as u32;
                let output_point = TabPoint::new(0, output_column);

                assert_eq!(
                    tab_snapshot.point_to_tab_point(input_point, text::Bias::Left),
                    output_point,
                    "point_to_tab_point({input_point:?})"
                );
                assert_eq!(
                    tab_snapshot.tab_point_to_point(output_point, text::Bias::Left),
                    input_point,
                    "tab_point_to_point({output_point:?})"
                );
            }
        }
    }

    #[gpui::test]
    fn test_long_lines_with_character_spanning_max_expansion_column(cx: &mut gpui::App) {
        let max_expansion_column = 8;
        let input = "abcdefg⋯hij";

        let mut tab_snapshot = tab_snapshot_for_text(input, cx);
        tab_snapshot.max_expansion_column = max_expansion_column;

        assert_eq!(tab_snapshot_text(&tab_snapshot), input);
    }

    #[gpui::test]
    fn test_expand_tabs(cx: &mut gpui::App) {
        let test_values = [
            ("κg🎲 f\nwo❌🏀by🎲✋β❌c\tβ✋ \ncλ🎲", 17),
            (" \twςe", 4),
            ("fε", 1),
            ("i✋\t", 3),
        ];
        let tab_snapshot = tab_snapshot_for_text("", cx);

        for (text, column) in test_values {
            let mut tabs = 0u128;
            let mut chars = 0u128;
            for (index, character) in text.char_indices() {
                if character == '\t' {
                    tabs |= 1 << index;
                }
                chars |= 1 << index;
            }

            let chunks = [Chunk {
                text,
                tabs,
                chars,
                ..Default::default()
            }];

            let cursor = TabStopCursor::new(chunks);

            assert_eq!(
                expected_expand_tabs(&tab_snapshot, text.chars(), column),
                tab_snapshot.expand_tabs(cursor, column)
            );
        }
    }

    #[gpui::test]
    fn test_collapse_tabs(cx: &mut gpui::App) {
        let input = "A\tBC\tDEF\tG\tHI\tJ\tK\tL\tM";

        let tab_snapshot = tab_snapshot_for_text(input, cx);

        for (index, _) in input.char_indices() {
            let range = TabPoint::new(0, index as u32)..tab_snapshot.max_point();

            assert_eq!(
                expected_buffer_point(&tab_snapshot, range.start, Bias::Left),
                tab_snapshot.tab_point_to_buffer_point(range.start, Bias::Left),
                "failed with tab_point at column {index}"
            );
            assert_eq!(
                expected_buffer_point(&tab_snapshot, range.start, Bias::Right),
                tab_snapshot.tab_point_to_buffer_point(range.start, Bias::Right),
                "failed with tab_point at column {index}"
            );

            assert_eq!(
                expected_buffer_point(&tab_snapshot, range.end, Bias::Left),
                tab_snapshot.tab_point_to_buffer_point(range.end, Bias::Left),
                "failed with tab_point at column {index}"
            );
            assert_eq!(
                expected_buffer_point(&tab_snapshot, range.end, Bias::Right),
                tab_snapshot.tab_point_to_buffer_point(range.end, Bias::Right),
                "failed with tab_point at column {index}"
            );
        }
    }
}
