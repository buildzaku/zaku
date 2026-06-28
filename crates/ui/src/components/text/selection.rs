use gpui::{Bounds, Hsla, Pixels, Point, SharedString, TextLayout, Window};
use std::ops::Range;

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
enum TextSelectionMode {
    Character,
    Word,
}

#[derive(Clone, Copy)]
struct TextSelection<T> {
    anchor: TextSelectionPoint<T>,
    head: TextSelectionPoint<T>,
    mode: TextSelectionMode,
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
    layout: TextLayout,
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

    pub fn register_layout(&mut self, id: T, text: SharedString, layout: TextLayout) {
        if let Some(existing_layout) = self.layouts.iter_mut().find(|layout| layout.id == id) {
            existing_layout.text = text;
            existing_layout.layout = layout;
        } else {
            self.layouts.push(TextLayoutEntry { id, text, layout });
        }
    }

    pub fn selected_range_for_id(&self, id: T, text: &str) -> Option<Range<usize>> {
        let range = selection_range_for_id(self.selection.as_ref()?, id, text.len())?;
        valid_selection_range(text, range)
    }

    pub fn begin_selection(&mut self, id: T, position: Point<Pixels>, click_count: usize) -> bool {
        let Some(layout) = self.layout_for_id(id) else {
            return false;
        };
        let point = self.point_for_layout(id, layout, position);
        let word_range =
            (click_count >= 2).then(|| text_word_range(layout.text.as_ref(), point.offset));
        self.is_selecting = true;

        if let Some(word_range) = word_range {
            self.selection = Some(TextSelection {
                anchor: TextSelectionPoint::new(id, word_range.start),
                head: TextSelectionPoint::new(id, word_range.end),
                mode: TextSelectionMode::Word,
            });
        } else {
            self.selection = Some(TextSelection {
                anchor: point,
                head: point,
                mode: TextSelectionMode::Character,
            });
        }

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

        let head = match selection.mode {
            TextSelectionMode::Character => point,
            TextSelectionMode::Word => {
                let word_range = text_word_range(layout.text.as_ref(), point.offset);
                let offset = if point < selection.anchor {
                    word_range.start
                } else {
                    word_range.end
                };
                TextSelectionPoint::new(id, offset)
            }
        };

        if let Some(selection) = self.selection.as_mut()
            && selection.head != head
        {
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
        let offset = match layout.layout.index_for_position(position) {
            Ok(offset) | Err(offset) => offset.min(layout.text.len()),
        };
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

fn text_word_range(text: &str, offset: usize) -> Range<usize> {
    if text.is_empty() {
        return 0..0;
    }

    let mut offset = previous_char_boundary(text, offset.min(text.len()));
    if offset == text.len()
        && let Some((previous_index, _)) = text.char_indices().next_back()
    {
        offset = previous_index;
    }

    let Some(character) = text.get(offset..).and_then(|text| text.chars().next()) else {
        return offset..offset;
    };
    if character.is_whitespace() {
        return offset..offset + character.len_utf8();
    }

    let mut start = offset;
    if let Some(text_before_offset) = text.get(..offset) {
        for (index, character) in text_before_offset.char_indices().rev() {
            if character.is_whitespace() {
                break;
            }
            start = index;
        }
    }

    let mut end = text.len();
    if let Some(text_after_offset) = text.get(offset..) {
        for (index, character) in text_after_offset.char_indices() {
            if character.is_whitespace() {
                end = offset + index;
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
                        window.pixel_snap_bounds(Bounds::new(
                            gpui::point(left, wrapped_line_top),
                            gpui::size(right - left, line_height),
                        )),
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
