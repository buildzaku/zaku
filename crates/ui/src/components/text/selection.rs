use gpui::{Bounds, Hsla, Pixels, Point, SharedString, TextLayout, Window, WrappedLineLayout};
use smallvec::SmallVec;
use std::{cmp::Ordering, ops::Range, sync::Arc};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TextSelectionPoint<T> {
    id: T,
    offset: usize,
}

impl<T> TextSelectionPoint<T> {
    pub fn new(id: T, offset: usize) -> Self {
        Self { id, offset }
    }
}

#[derive(Clone, PartialEq, Eq)]
enum TextSelectionMode<T> {
    Character,
    Word(Range<TextSelectionPoint<T>>),
    Line(Range<TextSelectionPoint<T>>),
}

#[derive(Clone, PartialEq, Eq)]
struct TextSelection<T> {
    start: TextSelectionPoint<T>,
    end: TextSelectionPoint<T>,
    reversed: bool,
    mode: TextSelectionMode<T>,
}

impl<T: Copy + Ord> TextSelection<T> {
    fn head(&self) -> TextSelectionPoint<T> {
        if self.reversed { self.start } else { self.end }
    }

    fn tail(&self) -> TextSelectionPoint<T> {
        if self.reversed { self.end } else { self.start }
    }

    fn set_head(&mut self, head: TextSelectionPoint<T>) {
        if head.cmp(&self.tail()) < Ordering::Equal {
            if !self.reversed {
                self.end = self.start;
                self.reversed = true;
            }
            self.start = head;
        } else {
            if self.reversed {
                self.start = self.end;
                self.reversed = false;
            }
            self.end = head;
        }
    }

    fn is_empty(&self) -> bool {
        self.head() == self.tail()
    }

    fn range(&self) -> Range<TextSelectionPoint<T>> {
        self.start..self.end
    }
}

#[derive(Clone, Copy)]
struct PointForPosition<T> {
    nearest_valid: TextSelectionPoint<T>,
    is_text_hovered: bool,
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

    #[cfg(test)]
    fn position_for_offset(&self, text: &str, offset: usize) -> Option<Point<Pixels>> {
        let offset = previous_char_boundary(text, offset.min(text.len()));
        let mut line_origin = self.bounds.origin;
        let mut line_start_index = 0;

        for line_layout in &self.lines {
            let line_end_index = line_start_index + line_layout.len();
            if offset <= line_end_index {
                let position =
                    line_layout.position_for_index(offset - line_start_index, self.line_height)?;
                return Some(gpui::point(
                    line_origin.x + position.x,
                    line_origin.y + position.y,
                ));
            }

            line_origin.y += line_layout.size(self.line_height).height;
            line_start_index += line_layout.len() + 1;
        }

        None
    }
}

pub struct TextSelectionState<T> {
    selection: Option<TextSelection<T>>,
    is_selecting: bool,
    layouts: Vec<TextLayoutEntry<T>>,
    selection_bounds: Option<Bounds<Pixels>>,
}

impl<T: Copy + Ord> TextSelectionState<T> {
    pub fn new() -> Self {
        Self {
            selection: None,
            is_selecting: false,
            layouts: Vec::new(),
            selection_bounds: None,
        }
    }

    pub fn clear(&mut self) {
        self.selection = None;
        self.is_selecting = false;
    }

    pub fn clear_layouts(&mut self) {
        self.layouts.clear();
        self.selection_bounds = None;
    }

    pub(crate) fn has_registered_layouts(&self) -> bool {
        !self.layouts.is_empty()
    }

    pub fn end_selection_drag(&mut self) -> bool {
        let was_selecting = self.is_selecting;
        self.is_selecting = false;
        was_selecting
    }

    pub fn selection_is_empty(&self) -> bool {
        self.selection.as_ref().is_none_or(TextSelection::is_empty)
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

    pub fn set_selection_bounds(&mut self, bounds: Bounds<Pixels>) {
        self.selection_bounds = Some(bounds);
    }

    pub fn selected_range_for_id(&self, id: T, text: &str) -> Option<Range<usize>> {
        let selection_range = self.selection.as_ref()?.range();
        let text_len = text.len();
        let cell_start = TextSelectionPoint::new(id, 0);
        let cell_end = TextSelectionPoint::new(id, text_len);
        if selection_range.end <= cell_start || selection_range.start >= cell_end {
            return None;
        }

        let selected_start = if selection_range.start.id == id {
            selection_range.start.offset.min(text_len)
        } else {
            0
        };
        let selected_end = if selection_range.end.id == id {
            selection_range.end.offset.min(text_len)
        } else {
            text_len
        };
        let selected_start = previous_char_boundary(text, selected_start);
        let selected_end = previous_char_boundary(text, selected_end);

        (selected_start < selected_end).then_some(selected_start..selected_end)
    }

    #[cfg(test)]
    pub(super) fn position_for_id_offset(&self, id: T, offset: usize) -> Option<Point<Pixels>> {
        let layout = self.layout_for_id(id)?;
        layout
            .layout
            .position_for_offset(layout.text.as_ref(), offset)
    }

    pub(super) fn selected_text(
        &self,
        selection_order: &[T],
        copy_separator: &str,
        mut text_for_selection: impl FnMut(T) -> Option<SharedString>,
    ) -> Option<String> {
        if self.selection_is_empty() {
            return None;
        }

        let mut selected_text = Vec::new();

        for id in selection_order {
            let Some(text) = text_for_selection(*id) else {
                continue;
            };
            let text: &str = text.as_ref();
            let Some(range) = self.selected_range_for_id(*id, text) else {
                continue;
            };

            if let Some(text) = text.get(range) {
                selected_text.push(text.to_string());
            }
        }

        let selected_text = selected_text.join(copy_separator);
        (!selected_text.is_empty()).then_some(selected_text)
    }

    pub fn begin_selection_at_position(
        &mut self,
        position: Point<Pixels>,
        click_count: usize,
    ) -> bool {
        let Some(point_for_position) = self.point_for_position(position) else {
            return false;
        };
        let click_count = if point_for_position.is_text_hovered {
            click_count
        } else {
            1
        };

        self.begin_selection_at_point(point_for_position.nearest_valid, click_count)
    }

    pub fn begin_selection(&mut self, id: T, position: Point<Pixels>, click_count: usize) -> bool {
        let Some(layout) = self.layout_for_id(id) else {
            return false;
        };
        let point = self.point_for_layout(id, layout, position);

        self.begin_selection_at_point(point, click_count)
    }

    fn begin_selection_at_point(
        &mut self,
        point: TextSelectionPoint<T>,
        click_count: usize,
    ) -> bool {
        if self.layout_for_id(point.id).is_none() {
            return false;
        }

        let selection = match click_count {
            1 => TextSelection {
                start: point,
                end: point,
                reversed: false,
                mode: TextSelectionMode::Character,
            },
            2 => {
                let Some(word_range) = self.surrounding_word_range(point) else {
                    return false;
                };
                let start = word_range.start;
                let end = word_range.end;
                TextSelection {
                    start,
                    end,
                    reversed: false,
                    mode: TextSelectionMode::Word(start..end),
                }
            }
            _ => {
                let Some(line_range) = self.surrounding_line_range(point) else {
                    return false;
                };
                let start = line_range.start;
                let end = line_range.end;
                TextSelection {
                    start,
                    end,
                    reversed: false,
                    mode: TextSelectionMode::Line(start..end),
                }
            }
        };

        self.is_selecting = true;
        self.selection = Some(selection);

        true
    }

    pub fn update_selection(&mut self, id: T, position: Point<Pixels>) -> bool {
        let Some(layout) = self.layout_for_id(id) else {
            return false;
        };
        let point = self.point_for_layout(id, layout, position);

        self.update_selection_to_point(point)
    }

    fn update_selection_to_point(&mut self, point: TextSelectionPoint<T>) -> bool {
        if !self.is_selecting {
            return false;
        }

        let Some(mut selection) = self.selection.clone() else {
            return false;
        };
        let old_selection = selection.clone();

        match selection.mode.clone() {
            TextSelectionMode::Character => selection.set_head(point),
            TextSelectionMode::Word(original_range) | TextSelectionMode::Line(original_range) => {
                let head_range = if matches!(&selection.mode, TextSelectionMode::Word(_)) {
                    self.surrounding_word_range(point)
                } else {
                    self.surrounding_line_range(point)
                };
                let Some(head_range) = head_range else {
                    return false;
                };

                if point < original_range.start {
                    selection.start = head_range.start;
                    selection.end = original_range.end;
                    selection.reversed = true;
                } else if point >= original_range.end {
                    selection.start = original_range.start;
                    selection.end = head_range.end;
                    selection.reversed = false;
                } else {
                    selection.start = original_range.start;
                    selection.end = original_range.end;
                    selection.reversed = false;
                }
            }
        }

        if selection != old_selection {
            self.selection = Some(selection);
            return true;
        }

        false
    }

    fn surrounding_word_range(
        &self,
        point: TextSelectionPoint<T>,
    ) -> Option<Range<TextSelectionPoint<T>>> {
        let layout = self.layout_for_id(point.id)?;
        let range = surrounding_word_range_for_text(layout.text.as_ref(), point.offset);

        Some(
            TextSelectionPoint::new(point.id, range.start)
                ..TextSelectionPoint::new(point.id, range.end),
        )
    }

    fn surrounding_line_range(
        &self,
        point: TextSelectionPoint<T>,
    ) -> Option<Range<TextSelectionPoint<T>>> {
        let layout = self.layout_for_id(point.id)?;

        Some(
            TextSelectionPoint::new(point.id, 0)
                ..TextSelectionPoint::new(point.id, layout.text.len()),
        )
    }

    pub fn update_selection_at_position(&mut self, position: Point<Pixels>) -> bool {
        let Some(point_for_position) = self.point_for_position(position) else {
            return false;
        };

        self.update_selection_to_point(point_for_position.nearest_valid)
    }

    pub fn end_selection(&mut self, id: T, position: Point<Pixels>) -> bool {
        let was_selecting = self.is_selecting;
        let updated = self.update_selection(id, position);
        self.is_selecting = false;
        updated || was_selecting
    }

    pub fn select_all(&mut self, tail: TextSelectionPoint<T>, head: TextSelectionPoint<T>) {
        let reversed = head < tail;
        let (start, end) = if reversed { (head, tail) } else { (tail, head) };

        self.selection = Some(TextSelection {
            start,
            end,
            reversed,
            mode: TextSelectionMode::Character,
        });
        self.is_selecting = false;
    }

    fn layout_for_id(&self, id: T) -> Option<&TextLayoutEntry<T>> {
        self.layouts.iter().find(|layout| layout.id == id)
    }

    fn point_for_position(&self, position: Point<Pixels>) -> Option<PointForPosition<T>> {
        let mut layouts = self.layouts.iter().collect::<Vec<_>>();
        layouts.sort_by_key(|layout| layout.id);

        let first_layout = *layouts.first()?;
        let last_layout = *layouts.last()?;
        let mut top = first_layout.layout.bounds.top();
        let mut bottom = first_layout.layout.bounds.bottom();

        for layout in &layouts {
            top = top.min(layout.layout.bounds.top());
            bottom = bottom.max(layout.layout.bounds.bottom());
        }

        if let Some(selection_bounds) = self.selection_bounds {
            top = selection_bounds.top();
            bottom = selection_bounds.bottom();
        }

        if position.y < top {
            return Some(PointForPosition {
                nearest_valid: TextSelectionPoint::new(first_layout.id, 0),
                is_text_hovered: false,
            });
        }
        if position.y > bottom {
            return Some(PointForPosition {
                nearest_valid: TextSelectionPoint::new(last_layout.id, last_layout.text.len()),
                is_text_hovered: false,
            });
        }

        for (index, layout) in layouts.iter().enumerate() {
            if position.x < layout.layout.bounds.left() {
                if let Some(previous_layout) =
                    index.checked_sub(1).and_then(|index| layouts.get(index))
                {
                    let previous_right = previous_layout.layout.bounds.right();
                    let next_left = layout.layout.bounds.left();
                    if position.x - previous_right < next_left - position.x {
                        return Some(PointForPosition {
                            nearest_valid: TextSelectionPoint::new(
                                previous_layout.id,
                                previous_layout.text.len(),
                            ),
                            is_text_hovered: false,
                        });
                    }
                }

                return Some(PointForPosition {
                    nearest_valid: TextSelectionPoint::new(layout.id, 0),
                    is_text_hovered: false,
                });
            }

            if position.x <= layout.layout.bounds.right() {
                let is_text_hovered = position.y >= layout.layout.bounds.top()
                    && position.y <= layout.layout.bounds.bottom();
                let position = if is_text_hovered {
                    position
                } else {
                    gpui::point(position.x, layout.layout.bounds.top())
                };
                return Some(PointForPosition {
                    nearest_valid: self.point_for_layout(layout.id, layout, position),
                    is_text_hovered,
                });
            }
        }

        Some(PointForPosition {
            nearest_valid: TextSelectionPoint::new(last_layout.id, last_layout.text.len()),
            is_text_hovered: false,
        })
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

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum CharKind {
    Whitespace,
    Punctuation,
    Word,
}

impl From<char> for CharKind {
    fn from(character: char) -> Self {
        if character == '_' || character.is_alphanumeric() {
            return Self::Word;
        }
        if character.is_whitespace() {
            return Self::Whitespace;
        }
        Self::Punctuation
    }
}

fn surrounding_word_range_for_text(text: &str, offset: usize) -> Range<usize> {
    let offset = previous_char_boundary(text, offset.min(text.len()));
    let mut start = offset;
    let mut end = offset;

    let previous_kind = text
        .get(..offset)
        .and_then(|text| text.chars().next_back())
        .map(CharKind::from);
    let next_kind = text
        .get(offset..)
        .and_then(|text| text.chars().next())
        .map(CharKind::from);
    let word_kind = std::cmp::max(previous_kind, next_kind);

    if let Some(text_before_offset) = text.get(..offset) {
        for character in text_before_offset.chars().rev().take(128) {
            if Some(CharKind::from(character)) == word_kind && character != '\n' {
                start -= character.len_utf8();
            } else {
                break;
            }
        }
    }

    if let Some(text_after_offset) = text.get(offset..) {
        for character in text_after_offset.chars().take(128) {
            if Some(CharKind::from(character)) == word_kind && character != '\n' {
                end += character.len_utf8();
            } else {
                break;
            }
        }
    }

    start..end
}

fn previous_char_boundary(text: &str, mut offset: usize) -> usize {
    while !text.is_char_boundary(offset) {
        offset = offset.saturating_sub(1);
    }
    offset
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selected_text_tracks_forward_and_backward_selection() {
        let items = [(0, "foo"), (1, "bar"), (2, "baz")];
        let [(foo_id, _), (bar_id, _), (baz_id, _)] = items;
        let selection_order = items.map(|(id, _)| id);
        let copy_separator = "\t";
        let mut text_for_selection = |id| {
            items.iter().find_map(|&(item_id, text)| {
                if item_id == id {
                    Some(SharedString::from(text))
                } else {
                    None
                }
            })
        };
        let foo_byte_offset = 1;
        let bar_byte_offset = 1;
        let baz_byte_offset = 2;
        let state = TextSelectionState {
            selection: Some(TextSelection {
                start: TextSelectionPoint::new(bar_id, bar_byte_offset),
                end: TextSelectionPoint::new(baz_id, baz_byte_offset),
                reversed: false,
                mode: TextSelectionMode::Character,
            }),
            is_selecting: false,
            layouts: Vec::new(),
            selection_bounds: None,
        };

        assert_eq!(
            state.selected_text(&selection_order, copy_separator, &mut text_for_selection),
            Some("ar\tba".to_string()),
        );

        let state = TextSelectionState {
            selection: Some(TextSelection {
                start: TextSelectionPoint::new(foo_id, foo_byte_offset),
                end: TextSelectionPoint::new(bar_id, bar_byte_offset),
                reversed: true,
                mode: TextSelectionMode::Character,
            }),
            is_selecting: false,
            layouts: Vec::new(),
            selection_bounds: None,
        };

        assert_eq!(
            state.selected_text(&selection_order, copy_separator, &mut text_for_selection),
            Some("oo\tb".to_string()),
        );
    }

    #[test]
    fn test_update_selection_to_point_stops_after_selection_drag_ends() {
        let items = [(0, "foo"), (1, "bar"), (2, "baz")];
        let [(foo_id, _), (bar_id, _), (baz_id, _)] = items;
        let foo_byte_offset = 1;
        let bar_byte_offset = 1;
        let baz_byte_offset = 2;
        let mut state = TextSelectionState {
            selection: Some(TextSelection {
                start: TextSelectionPoint::new(foo_id, foo_byte_offset),
                end: TextSelectionPoint::new(bar_id, bar_byte_offset),
                reversed: true,
                mode: TextSelectionMode::Character,
            }),
            is_selecting: true,
            layouts: Vec::new(),
            selection_bounds: None,
        };

        assert!(state.update_selection_to_point(TextSelectionPoint::new(baz_id, baz_byte_offset)));

        let selection = state.selection.as_ref().unwrap();
        assert_eq!(
            selection.tail(),
            TextSelectionPoint::new(bar_id, bar_byte_offset)
        );
        assert_eq!(
            selection.head(),
            TextSelectionPoint::new(baz_id, baz_byte_offset)
        );
        assert!(!selection.reversed);

        assert!(state.end_selection_drag());
        assert!(!state.update_selection_to_point(TextSelectionPoint::new(foo_id, foo_byte_offset)));

        let selection = state.selection.as_ref().unwrap();
        assert_eq!(
            selection.tail(),
            TextSelectionPoint::new(bar_id, bar_byte_offset)
        );
        assert_eq!(
            selection.head(),
            TextSelectionPoint::new(baz_id, baz_byte_offset)
        );
        assert!(!selection.reversed);
    }

    #[test]
    fn test_surrounding_word_uses_adjacent_word_at_boundary() {
        assert_eq!(surrounding_word_range_for_text("foo bar baz", 1), 0..3);
        assert_eq!(surrounding_word_range_for_text("foo bar baz", 4), 4..7);
        assert_eq!(surrounding_word_range_for_text("foo bar baz", 6), 4..7);
        assert_eq!(surrounding_word_range_for_text("foo bar baz", 10), 8..11);
        assert_eq!(surrounding_word_range_for_text("foo bar baz", 3), 0..3);
    }
}
