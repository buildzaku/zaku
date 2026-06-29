use gpui::{Bounds, Hsla, Pixels, Point, SharedString, TextLayout, Window, WrappedLineLayout};
use smallvec::SmallVec;
use std::{ops::Range, sync::Arc};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TextSelectionPoint<T> {
    id: T,
    offset: usize,
}

impl<T> TextSelectionPoint<T> {
    pub fn new(id: T, offset: usize) -> Self {
        Self { id, offset }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TextSelectionMode<T> {
    Character,
    Word {
        start: TextSelectionPoint<T>,
        end: TextSelectionPoint<T>,
    },
    Block {
        start: TextSelectionPoint<T>,
        end: TextSelectionPoint<T>,
    },
}

#[derive(Clone, Copy)]
struct TextSelection<T> {
    anchor: TextSelectionPoint<T>,
    head: TextSelectionPoint<T>,
    mode: TextSelectionMode<T>,
}

impl<T: Copy + Ord> TextSelection<T> {
    fn range(&self) -> (TextSelectionPoint<T>, TextSelectionPoint<T>) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    fn is_empty(&self) -> bool {
        self.anchor == self.head
    }
}

#[derive(Clone)]
struct TextLayoutEntry<T> {
    id: T,
    text: SharedString,
    layout: TextLayoutSnapshot,
}

#[derive(Clone)]
struct TextLayoutSnapshot {
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    lines: SmallVec<[Arc<WrappedLineLayout>; 1]>,
}

impl TextLayoutSnapshot {
    fn new(layout: &TextLayout) -> Self {
        Self {
            bounds: layout.bounds(),
            line_height: layout.line_height(),
            lines: layout.line_layouts(),
        }
    }

    fn closest_index_for_position(&self, position: Point<Pixels>) -> usize {
        if position.y < self.bounds.top() {
            return 0;
        }

        let mut line_origin = self.bounds.origin;
        let mut line_start_index = 0;
        for line_layout in &self.lines {
            let line_bottom = line_origin.y + line_layout.size(self.line_height).height;
            if position.y > line_bottom {
                line_origin.y = line_bottom;
                line_start_index += line_layout.len() + 1;
            } else {
                let position_within_line = position - line_origin;
                let index_within_line = match line_layout
                    .closest_index_for_position(position_within_line, self.line_height)
                {
                    Ok(index) | Err(index) => index,
                };
                return line_start_index + index_within_line;
            }
        }

        line_start_index.saturating_sub(1)
    }
}

pub struct TextSelectionState<T> {
    selection: Option<TextSelection<T>>,
    is_selecting: bool,
    layouts: Vec<TextLayoutEntry<T>>,
}

impl<T: Copy + Ord> TextSelectionState<T> {
    pub fn new() -> Self {
        Self {
            selection: None,
            is_selecting: false,
            layouts: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.selection = None;
        self.is_selecting = false;
    }

    pub fn clear_layouts(&mut self) {
        self.layouts.clear();
    }

    pub(crate) fn has_registered_layouts(&self) -> bool {
        !self.layouts.is_empty()
    }

    pub fn end_selection_drag(&mut self) -> bool {
        let was_selecting = self.is_selecting;
        self.is_selecting = false;
        was_selecting
    }

    pub fn has_non_empty_selection(&self) -> bool {
        self.selection
            .as_ref()
            .is_some_and(|selection| !selection.is_empty())
    }

    pub fn register_layout(&mut self, id: T, text: SharedString, layout: &TextLayout) {
        let snapshot = TextLayoutSnapshot::new(layout);
        if let Some(existing_layout) = self.layouts.iter_mut().find(|entry| entry.id == id) {
            existing_layout.text = text;
            existing_layout.layout = snapshot;
        } else {
            self.layouts.push(TextLayoutEntry {
                id,
                text,
                layout: snapshot,
            });
        }
    }

    pub fn selected_range_for_id(&self, id: T, text: &str) -> Option<Range<usize>> {
        let range = selection_range_for_id(self.selection.as_ref()?, id, text.len())?;
        valid_selection_range(text, range)
    }

    pub fn begin_selection(&mut self, id: T, position: Point<Pixels>, click_count: usize) -> bool {
        let selection = {
            let Some(layout) = self.layout_for_id(id) else {
                return false;
            };
            let point = self.point_for_layout(id, layout, position);

            match click_count {
                1 => TextSelection {
                    anchor: point,
                    head: point,
                    mode: TextSelectionMode::Character,
                },
                2 => {
                    let word_range = surrounding_word(layout.text.as_ref(), point.offset);
                    let start = TextSelectionPoint::new(id, word_range.start);
                    let end = TextSelectionPoint::new(id, word_range.end);
                    TextSelection {
                        anchor: start,
                        head: end,
                        mode: TextSelectionMode::Word { start, end },
                    }
                }
                _ => {
                    let start = TextSelectionPoint::new(id, 0);
                    let end = TextSelectionPoint::new(id, layout.text.len());
                    TextSelection {
                        anchor: start,
                        head: end,
                        mode: TextSelectionMode::Block { start, end },
                    }
                }
            }
        };

        self.is_selecting = true;
        self.selection = Some(selection);

        true
    }

    pub fn update_selection(&mut self, id: T, position: Point<Pixels>) -> bool {
        if !self.is_selecting {
            return false;
        }

        let Some(selection) = self.selection else {
            return false;
        };
        let Some(layout) = self.layout_for_id(id) else {
            return false;
        };
        let point = self.point_for_layout(id, layout, position);

        let (anchor, head) = match selection.mode {
            TextSelectionMode::Character => (selection.anchor, point),
            TextSelectionMode::Word { start, end } => {
                let head = word_selection_head(layout.text.as_ref(), point, start, end);
                let anchor = if head <= start { end } else { start };
                (anchor, head)
            }
            TextSelectionMode::Block { start, end } => {
                let block_start = TextSelectionPoint::new(id, 0);
                let block_end = TextSelectionPoint::new(id, layout.text.len());
                let head = if point <= start {
                    block_start
                } else {
                    block_end
                };
                let anchor = if head <= start { end } else { start };
                (anchor, head)
            }
        };

        if let Some(selection) = self.selection.as_mut()
            && (selection.anchor != anchor || selection.head != head)
        {
            selection.anchor = anchor;
            selection.head = head;
            return true;
        }

        false
    }

    pub fn end_selection(&mut self, id: T, position: Point<Pixels>) -> bool {
        let was_selecting = self.is_selecting;
        let updated = self.update_selection(id, position);
        self.is_selecting = false;
        updated || was_selecting
    }

    pub fn select_all(&mut self, anchor: TextSelectionPoint<T>, head: TextSelectionPoint<T>) {
        self.selection = Some(TextSelection {
            anchor,
            head,
            mode: TextSelectionMode::Character,
        });
        self.is_selecting = false;
    }

    fn layout_for_id(&self, id: T) -> Option<&TextLayoutEntry<T>> {
        self.layouts.iter().find(|layout| layout.id == id)
    }

    fn point_for_layout(
        &self,
        id: T,
        layout: &TextLayoutEntry<T>,
        position: Point<Pixels>,
    ) -> TextSelectionPoint<T> {
        let offset = layout
            .layout
            .closest_index_for_position(position)
            .min(layout.text.len());
        TextSelectionPoint::new(id, offset)
    }
}

impl<T: Copy + Ord> Default for TextSelectionState<T> {
    fn default() -> Self {
        Self::new()
    }
}

fn selection_range_for_id<T: Copy + Ord>(
    selection: &TextSelection<T>,
    id: T,
    text_len: usize,
) -> Option<Range<usize>> {
    let (start, end) = selection.range();
    let cell_start = TextSelectionPoint::new(id, 0);
    let cell_end = TextSelectionPoint::new(id, text_len);
    if end <= cell_start || start >= cell_end {
        return None;
    }

    let selected_start = if start.id == id {
        start.offset.min(text_len)
    } else {
        0
    };
    let selected_end = if end.id == id {
        end.offset.min(text_len)
    } else {
        text_len
    };

    (selected_start < selected_end).then_some(selected_start..selected_end)
}

fn word_selection_head<T: Copy + Ord>(
    text: &str,
    point: TextSelectionPoint<T>,
    start: TextSelectionPoint<T>,
    end: TextSelectionPoint<T>,
) -> TextSelectionPoint<T> {
    let offset = if is_inside_word(text, point.offset) || (start <= point && point < end) {
        let word_range = surrounding_word(text, point.offset);
        let word_start = TextSelectionPoint::new(point.id, word_range.start);
        if word_start < start {
            word_range.start
        } else {
            word_range.end
        }
    } else {
        point.offset
    };

    TextSelectionPoint::new(point.id, offset)
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum CharKind {
    Whitespace,
    Punctuation,
    Word,
}

fn surrounding_word(text: &str, offset: usize) -> Range<usize> {
    let offset = previous_char_boundary(text, offset.min(text.len()));
    let mut start = offset;
    let mut end = offset;

    let previous_kind = text
        .get(..offset)
        .and_then(|text| text.chars().next_back())
        .map(char_kind);
    let next_kind = text
        .get(offset..)
        .and_then(|text| text.chars().next())
        .map(char_kind);
    let word_kind = std::cmp::max(previous_kind, next_kind);

    if let Some(text_before_offset) = text.get(..offset) {
        for character in text_before_offset.chars().rev().take(128) {
            if Some(char_kind(character)) == word_kind && character != '\n' {
                start -= character.len_utf8();
            } else {
                break;
            }
        }
    }

    if let Some(text_after_offset) = text.get(offset..) {
        for character in text_after_offset.chars().take(128) {
            if Some(char_kind(character)) == word_kind && character != '\n' {
                end += character.len_utf8();
            } else {
                break;
            }
        }
    }

    start..end
}

fn is_inside_word(text: &str, offset: usize) -> bool {
    let offset = previous_char_boundary(text, offset.min(text.len()));
    let next_char_kind = text
        .get(offset..)
        .and_then(|text| text.chars().next())
        .map(char_kind);
    let previous_char_kind = text
        .get(..offset)
        .and_then(|text| text.chars().next_back())
        .map(char_kind);

    previous_char_kind.zip(next_char_kind) == Some((CharKind::Word, CharKind::Word))
}

fn char_kind(character: char) -> CharKind {
    if character == '_' || character.is_alphanumeric() {
        return CharKind::Word;
    }
    if character.is_whitespace() {
        return CharKind::Whitespace;
    }
    CharKind::Punctuation
}

fn previous_char_boundary(text: &str, mut offset: usize) -> usize {
    while !text.is_char_boundary(offset) {
        offset = offset.saturating_sub(1);
    }
    offset
}

fn valid_selection_range(text: &str, range: Range<usize>) -> Option<Range<usize>> {
    let start = previous_char_boundary(text, range.start.min(text.len()));
    let end = previous_char_boundary(text, range.end.min(text.len()));

    (start < end).then_some(start..end)
}

pub fn paint_text_selection(
    range: Range<usize>,
    text_layout: &TextLayout,
    color: Hsla,
    window: &mut Window,
) {
    let line_height = text_layout.line_height();
    let text_bounds = text_layout.bounds();
    let mut line_origin = text_bounds.origin;
    let mut line_start_index = 0;

    for line_layout in text_layout.line_layouts() {
        let mut wrapped_line_start_index = 0;
        let mut wrapped_line_top = line_origin.y;
        let wrapped_line_end_indices = line_layout
            .wrap_boundaries()
            .iter()
            .filter_map(|wrap_boundary| {
                line_layout
                    .runs()
                    .get(wrap_boundary.run_ix)
                    .and_then(|run| run.glyphs.get(wrap_boundary.glyph_ix))
                    .map(|glyph| glyph.index)
            })
            .chain([line_layout.len()]);

        for wrapped_line_end_index in wrapped_line_end_indices {
            let visual_line_start = line_start_index + wrapped_line_start_index;
            let visual_line_end = line_start_index + wrapped_line_end_index;
            let selected_start = range.start.max(visual_line_start);
            let selected_end = range.end.min(visual_line_end);

            if selected_start < selected_end {
                let local_start = selected_start - line_start_index;
                let local_end = selected_end - line_start_index;
                let left = if selected_start == visual_line_start {
                    line_origin.x
                } else {
                    let Some(start_position) =
                        line_layout.position_for_index(local_start, line_height)
                    else {
                        wrapped_line_start_index = wrapped_line_end_index;
                        wrapped_line_top += line_height;
                        continue;
                    };
                    line_origin.x + start_position.x
                };
                let Some(end_position) = line_layout.position_for_index(local_end, line_height)
                else {
                    wrapped_line_start_index = wrapped_line_end_index;
                    wrapped_line_top += line_height;
                    continue;
                };

                let right = (line_origin.x + end_position.x).max(left);
                if right > left {
                    window.paint_quad(gpui::fill(
                        Bounds::new(
                            gpui::point(left, wrapped_line_top),
                            gpui::size(right - left, line_height),
                        ),
                        color,
                    ));
                }
            }

            wrapped_line_start_index = wrapped_line_end_index;
            wrapped_line_top += line_height;
        }

        line_origin.y += line_layout.size(line_height).height;
        line_start_index += line_layout.len() + 1;
    }
}
