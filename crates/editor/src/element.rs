use gpui::{
    Action, App, Bounds, ContentMask, Context, CursorStyle, DispatchPhase, Element, ElementId,
    ElementInputHandler, Entity, GlobalElementId, Hitbox, HitboxBehavior, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, ShapedLine, Style, TextAlign,
    TextRun, UnderlineStyle, Window, point, prelude::*, px, size,
};
use multi_buffer::{MultiBufferOffset, MultiBufferRow};
use std::{any::TypeId, ops::Range};
use theme::ActiveTheme;

use crate::{Editor, EditorMode, EditorStyle, HandleInput, MAX_LINE_LEN};

pub(crate) struct PositionMap {
    pub size: gpui::Size<Pixels>,
    pub bounds: Bounds<Pixels>,
    pub line_height: Pixels,
    pub scroll_position: gpui::Point<f64>,
    pub em_layout_width: Pixels,
    pub line_layouts: Vec<LineWithInvisibles>,
    pub snapshot: crate::display_map::DisplaySnapshot,
    pub text_align: TextAlign,
    pub content_width: Pixels,
    pub masked: bool,
}

#[derive(Debug, Copy, Clone)]
pub struct PointForPosition {
    pub previous_valid: crate::display_map::DisplayPoint,
    pub next_valid: crate::display_map::DisplayPoint,
    pub exact_unclipped: crate::display_map::DisplayPoint,
    pub column_overshoot_after_line_end: u32,
}

impl PointForPosition {
    pub fn as_valid(&self) -> Option<crate::display_map::DisplayPoint> {
        if self.previous_valid == self.exact_unclipped && self.next_valid == self.exact_unclipped {
            Some(self.previous_valid)
        } else {
            None
        }
    }
}

impl PositionMap {
    pub(crate) fn point_for_position(&self, position: gpui::Point<Pixels>) -> PointForPosition {
        let local_position = position - self.bounds.origin;
        let y = local_position.y.max(px(0.)).min(self.size.height);
        let x = local_position.x + self.scroll_position.x as f32 * self.em_layout_width;
        let row = ((y / self.line_height) as f64 + self.scroll_position.y.max(0.0)) as u32;

        let (column, x_overshoot_after_line_end) = if let Some(line) = self
            .line_layouts
            .get(row.saturating_sub(self.scroll_position.y as u32) as usize)
        {
            let x_relative_to_text = x
                - line.alignment_offset(self.text_align, self.content_width)
                - self.em_layout_width * line.line_display_column_start as f32;
            if let Some(index) = line.index_for_x(x_relative_to_text) {
                let display_column = line
                    .line_display_column_start
                    .saturating_add(index)
                    .min(u32::MAX as usize);
                (display_column as u32, px(0.))
            } else {
                let display_column = line
                    .line_display_column_start
                    .saturating_add(line.len)
                    .min(u32::MAX as usize);
                (
                    display_column as u32,
                    px(0.).max(x_relative_to_text - line.width),
                )
            }
        } else {
            (0, x.max(px(0.)))
        };

        let mut exact_unclipped =
            crate::display_map::DisplayPoint::new(crate::display_map::DisplayRow(row), column);
        let previous_valid = self.snapshot.clip_point(exact_unclipped, text::Bias::Left);
        let next_valid = self.snapshot.clip_point(exact_unclipped, text::Bias::Right);

        let column_overshoot_after_line_end = if self.em_layout_width == px(0.) {
            0
        } else {
            (x_overshoot_after_line_end / self.em_layout_width) as u32
        };
        *exact_unclipped.column_mut() += column_overshoot_after_line_end;

        PointForPosition {
            previous_valid,
            next_valid,
            exact_unclipped,
            column_overshoot_after_line_end,
        }
    }
}

pub(crate) struct LineWithInvisibles {
    pub row: crate::display_map::DisplayRow,
    pub origin: gpui::Point<Pixels>,
    pub line_start_offset: usize,
    pub line_end_offset: usize,
    pub line_display_column_start: usize,
    pub len: usize,
    pub width: Pixels,
    pub line_text: String,
    pub shaped_line: ShapedLine,
}

impl LineWithInvisibles {
    pub fn x_for_index(&self, index: usize) -> Pixels {
        self.shaped_line.x_for_index(index.min(self.len))
    }

    pub fn index_for_x(&self, x: Pixels) -> Option<usize> {
        self.shaped_line
            .index_for_x(x)
            .map(|index| index.min(self.len))
    }

    pub fn alignment_offset(&self, text_align: TextAlign, content_width: Pixels) -> Pixels {
        match text_align {
            TextAlign::Left => px(0.),
            TextAlign::Center => ((content_width - self.width) / 2.).max(px(0.)),
            TextAlign::Right => (content_width - self.width).max(px(0.)),
        }
    }
}

pub struct EditorElement {
    editor: Entity<Editor>,
    style: EditorStyle,
}

impl EditorElement {
    pub fn new(editor: &Entity<Editor>, style: EditorStyle) -> Self {
        Self {
            editor: editor.clone(),
            style,
        }
    }

    fn register_actions(&self, window: &mut Window) {
        let editor = &self.editor;
        register_action(editor, window, Editor::move_left);
        register_action(editor, window, Editor::move_right);
        register_action(editor, window, Editor::move_up);
        register_action(editor, window, Editor::move_down);
        register_action(editor, window, Editor::select_left);
        register_action(editor, window, Editor::select_right);
        register_action(editor, window, Editor::select_up);
        register_action(editor, window, Editor::select_down);
        register_action(editor, window, Editor::select_all);
        register_action(editor, window, Editor::move_to_beginning);
        register_action(editor, window, Editor::move_to_end);
        register_action(editor, window, Editor::select_to_beginning);
        register_action(editor, window, Editor::select_to_end);
        register_action(editor, window, Editor::move_to_previous_word_start);
        register_action(editor, window, Editor::move_to_next_word_end);
        register_action(editor, window, Editor::move_to_previous_subword_start);
        register_action(editor, window, Editor::move_to_next_subword_end);
        register_action(editor, window, Editor::select_to_previous_word_start);
        register_action(editor, window, Editor::select_to_next_word_end);
        register_action(editor, window, Editor::select_to_previous_subword_start);
        register_action(editor, window, Editor::select_to_next_subword_end);
        register_action(editor, window, Editor::delete_to_previous_word_start);
        register_action(editor, window, Editor::delete_to_previous_subword_start);
        register_action(editor, window, Editor::delete_to_next_word_end);
        register_action(editor, window, Editor::delete_to_next_subword_end);
        register_action(editor, window, Editor::backspace);
        register_action(editor, window, Editor::delete);
        register_action(editor, window, Editor::newline);
        register_action(editor, window, Editor::copy);
        register_action(editor, window, Editor::cut);
        register_action(editor, window, Editor::paste);
        register_action(editor, window, Editor::undo);
        register_action(editor, window, Editor::redo);
        register_action(editor, window, Editor::undo_selection);
        register_action(editor, window, Editor::redo_selection);
        register_action(editor, window, Editor::move_to_beginning_of_line);
        register_action(editor, window, Editor::move_to_end_of_line);
        register_action(editor, window, Editor::select_to_beginning_of_line);
        register_action(editor, window, Editor::select_to_end_of_line);
        register_action(editor, window, Editor::delete_to_beginning_of_line);
        register_action(editor, window, Editor::delete_to_end_of_line);
        register_action(
            editor,
            window,
            |editor, action: &HandleInput, window, cx| {
                editor.handle_input(&action.0, window, cx);
            },
        );
    }
}

pub struct PrepaintState {
    line_layouts: Vec<LineWithInvisibles>,
    display_snapshot: crate::display_map::DisplaySnapshot,
    cursor: Option<PaintQuad>,
    selections: Vec<PaintQuad>,
    hitbox: Option<Hitbox>,
    line_height: Pixels,
    masked: bool,
}

impl IntoElement for EditorElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for EditorElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (gpui::LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = gpui::relative(1.).into();
        let line_height = self.style.text.line_height_in_pixels(window.rem_size());

        let editor = self.editor.read(cx);
        match editor.mode {
            EditorMode::SingleLine => {
                style.size.height = line_height.into();
            }
            EditorMode::AutoHeight {
                min_lines,
                max_lines,
            } => {
                let line_count = editor.snapshot(cx).max_point().row as usize + 1;
                let line_count = line_count.max(min_lines);
                let line_count =
                    max_lines.map_or(line_count, |max_lines| line_count.min(max_lines));
                style.size.height = (line_height * line_count as f32).into();
            }
            EditorMode::Full { .. } => {
                style.size.height = gpui::relative(1.).into();
            }
        }
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let style = self.style.text.clone();
        let rem_size = window.rem_size();
        let font_id = window.text_system().resolve_font(&style.font());
        let font_size = style.font_size.to_pixels(rem_size);
        let line_height = style.line_height_in_pixels(rem_size);
        let fallback_column_width = measure_column_width(&style, window);
        let column_width = window
            .text_system()
            .em_advance(font_id, font_size)
            .unwrap_or(fallback_column_width);
        let em_layout_width = window.text_system().em_layout_width(font_id, font_size);

        let (focus_handle, placeholder, masked, selection_range, marked_range, cursor_offset) = {
            let editor = self.editor.read(cx);
            (
                editor.focus_handle.clone(),
                editor.placeholder.clone(),
                editor.masked,
                editor.selected_range.clone(),
                editor.marked_range.clone(),
                editor.cursor_offset(),
            )
        };
        let display_snapshot = self
            .editor
            .update(cx, |editor, cx| editor.display_snapshot(cx));

        let max_display_row = display_snapshot.buffer_snapshot().max_point().row;

        let cursor_point = display_snapshot
            .buffer_snapshot()
            .offset_to_point(MultiBufferOffset(
                cursor_offset.min(display_snapshot.buffer_snapshot().len().0),
            ));
        let cursor_row = cursor_point.row;

        let has_content = !display_snapshot.buffer_snapshot().is_empty();
        let top_row = 0;
        let first_row_origin_y = bounds.top();
        let visible_row_count = ((bounds.bottom() - bounds.top()) / line_height).ceil() as u32 + 1;
        let line_scroll_x = 0.0;
        let lines = build_visible_lines(
            &display_snapshot,
            bounds,
            line_height,
            &style,
            font_size,
            &placeholder,
            masked,
            marked_range.as_ref(),
            max_display_row,
            top_row,
            visible_row_count,
            line_scroll_x,
            has_content,
            first_row_origin_y,
            em_layout_width,
            window,
            cx,
        );
        let mut selections = Vec::new();
        let mut cursor = None;
        for line in lines.iter() {
            let line_text = line.line_text.as_str();
            let line_display_column_start = line.line_display_column_start;
            if !selection_range.is_empty() {
                let selection_start = selection_range.start.max(line.line_start_offset);
                let selection_end = selection_range.end.min(line.line_end_offset);
                if selection_start < selection_end {
                    let start_column = (selection_start - line.line_start_offset) as u32;
                    let end_column = (selection_end - line.line_start_offset) as u32;
                    let (mut display_start, mut display_end) = if masked {
                        let start_column = (start_column as usize).min(line_text.len());
                        let end_column = (end_column as usize).min(line_text.len());
                        (
                            line_text.get(..start_column).unwrap_or("").chars().count(),
                            line_text.get(..end_column).unwrap_or("").chars().count(),
                        )
                    } else {
                        (
                            display_snapshot
                                .point_to_display_point(
                                    text::Point::new(line.row.0, start_column),
                                    text::Bias::Left,
                                )
                                .column() as usize,
                            display_snapshot
                                .point_to_display_point(
                                    text::Point::new(line.row.0, end_column),
                                    text::Bias::Right,
                                )
                                .column() as usize,
                        )
                    };
                    if !masked {
                        display_start = display_start.saturating_sub(line_display_column_start);
                        display_end = display_end.saturating_sub(line_display_column_start);
                    }
                    display_start = display_start.min(line.len);
                    display_end = display_end.min(line.len);

                    let selection_bounds = Bounds::from_corners(
                        point(
                            line.origin.x + line.x_for_index(display_start),
                            line.origin.y,
                        ),
                        point(
                            line.origin.x + line.x_for_index(display_end),
                            line.origin.y + line_height,
                        ),
                    );

                    selections.push(gpui::fill(
                        selection_bounds,
                        cx.theme().colors().element_selection_background,
                    ));
                }
            }

            if cursor.is_none() && cursor_row == line.row.0 {
                let cursor_column = cursor_offset.saturating_sub(line.line_start_offset) as u32;
                let cursor_x = if masked {
                    let cursor_column = (cursor_column as usize).min(line_text.len());
                    let display_column =
                        line_text.get(..cursor_column).unwrap_or("").chars().count();
                    line.origin.x + line.x_for_index(display_column.min(line.len))
                } else {
                    let cursor_display_column = display_snapshot
                        .point_to_display_point(
                            text::Point::new(line.row.0, cursor_column),
                            text::Bias::Left,
                        )
                        .column() as usize;
                    let line_display_column_end =
                        line_display_column_start.saturating_add(line.len);
                    if cursor_display_column < line_display_column_start {
                        line.origin.x
                            - column_width
                                * (line_display_column_start - cursor_display_column) as f32
                    } else if cursor_display_column <= line_display_column_end {
                        let local_column = cursor_display_column - line_display_column_start;
                        line.origin.x + line.x_for_index(local_column.min(line.len))
                    } else {
                        let trailing_columns = cursor_display_column - line_display_column_end;
                        line.origin.x + line.width + column_width * trailing_columns as f32
                    }
                };
                cursor = Some(gpui::fill(
                    Bounds::new(point(cursor_x, line.origin.y), size(px(2.), line_height)),
                    cx.theme().colors().editor_foreground,
                ));
            }
        }

        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        window.set_focus_handle(&focus_handle, cx);

        PrepaintState {
            line_layouts: lines,
            display_snapshot,
            cursor,
            selections,
            hitbox: Some(hitbox),
            line_height,
            masked,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.editor.read(cx).focus_handle.clone();
        let hitbox = prepaint.hitbox.take().expect("hitbox to be set");

        window.set_cursor_style(CursorStyle::IBeam, &hitbox);
        let key_context = self
            .editor
            .update(cx, |editor, cx| editor.key_context(window, cx));
        window.set_key_context(key_context);
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );
        self.register_actions(window);

        let line_height = prepaint.line_height;
        let text_style = self.style.text.clone();
        let rem_size = window.rem_size();
        let font_id = window.text_system().resolve_font(&text_style.font());
        let font_size = text_style.font_size.to_pixels(rem_size);
        let em_layout_width = window.text_system().em_layout_width(font_id, font_size);

        window.on_mouse_event({
            let editor = self.editor.clone();
            let hitbox = hitbox.clone();

            move |event: &MouseDownEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble {
                    return;
                }

                if event.button != MouseButton::Left {
                    return;
                }

                if hitbox.is_hovered(window) {
                    editor.update(cx, |editor, cx| editor.on_mouse_down(event, window, cx));
                }
            }
        });

        window.on_mouse_event({
            let editor = self.editor.clone();
            let hitbox = hitbox.clone();

            move |event: &MouseUpEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble {
                    return;
                }

                if event.button != MouseButton::Left {
                    return;
                }

                editor.update(cx, |editor, cx| {
                    if editor.selecting || hitbox.is_hovered(window) {
                        editor.on_mouse_up(event, window, cx);
                    }
                });
            }
        });

        window.on_mouse_event({
            let editor = self.editor.clone();

            move |event: &MouseMoveEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble {
                    return;
                }

                editor.update(cx, |editor, cx| {
                    if !editor.selecting {
                        return;
                    }

                    editor.on_mouse_move(event, window, cx);
                });
            }
        });

        if !self.style.background.is_transparent() {
            window.paint_quad(gpui::fill(bounds, self.style.background));
        }

        let text_bounds = bounds;

        window.with_content_mask(
            Some(ContentMask {
                bounds: text_bounds,
            }),
            |window| {
                for selection in prepaint.selections.drain(..) {
                    window.paint_quad(selection);
                }

                for line in prepaint.line_layouts.iter() {
                    line.shaped_line
                        .paint(line.origin, line_height, TextAlign::Left, None, window, cx)
                        .ok();
                }

                if focus_handle.is_focused(window)
                    && let Some(cursor) = prepaint.cursor.take()
                {
                    window.paint_quad(cursor);
                }
            },
        );

        self.editor.update(cx, |editor, _cx| {
            editor.last_position_map = Some(std::rc::Rc::new(PositionMap {
                size: bounds.size,
                bounds,
                line_height,
                scroll_position: point(0.0, 0.0),
                em_layout_width,
                snapshot: prepaint.display_snapshot.clone(),
                text_align: TextAlign::Left,
                content_width: bounds.size.width,
                masked: prepaint.masked,
                line_layouts: std::mem::take(&mut prepaint.line_layouts),
            }));
        });
    }
}

fn build_visible_lines(
    display_snapshot: &crate::display_map::DisplaySnapshot,
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    style: &gpui::TextStyle,
    font_size: Pixels,
    placeholder: &gpui::SharedString,
    masked: bool,
    marked_range: Option<&Range<usize>>,
    max_display_row: u32,
    top_row: u32,
    visible_row_count: u32,
    line_scroll_x: f64,
    has_content: bool,
    first_row_origin_y: Pixels,
    em_layout_width: Pixels,
    window: &mut Window,
    cx: &mut App,
) -> Vec<LineWithInvisibles> {
    let scroll_x_pixels = em_layout_width * line_scroll_x as f32;
    let mut lines = Vec::new();

    for visible_row_index in 0..visible_row_count {
        let row = top_row + visible_row_index;
        if row > max_display_row {
            break;
        }

        let row = crate::display_map::DisplayRow(row);
        let line_start_offset = display_snapshot
            .buffer_snapshot()
            .point_to_offset(text::Point::new(row.0, 0));
        let line_len = display_snapshot
            .buffer_snapshot()
            .line_len(MultiBufferRow(row.0)) as usize;
        let line_end_offset = line_start_offset + line_len;
        let mut line_text = String::new();
        let line_display_column_start = if masked {
            let mut line_exceeded_max_len = false;
            for line_chunk in display_snapshot.line_chunks(row) {
                let (mut chunk, has_newline) = if let Some(index) = line_chunk.find('\n') {
                    (&line_chunk[..index], true)
                } else {
                    (line_chunk, false)
                };

                if !chunk.is_empty() && !line_exceeded_max_len {
                    if line_text.len() + chunk.len() > MAX_LINE_LEN {
                        let mut chunk_len = MAX_LINE_LEN - line_text.len();
                        while !chunk.is_char_boundary(chunk_len) {
                            chunk_len -= 1;
                        }
                        chunk = &chunk[..chunk_len];
                        line_exceeded_max_len = true;
                    }

                    line_text.push_str(chunk);
                }

                if has_newline || line_exceeded_max_len {
                    break;
                }
            }

            0usize
        } else {
            let requested_start_column = line_scroll_x.floor().max(0.0) as u32;
            let mut line_display_column_start = display_snapshot
                .clip_point(
                    crate::display_map::DisplayPoint::new(row, requested_start_column),
                    text::Bias::Left,
                )
                .column() as usize;

            let line_display_len = display_snapshot.line_len(row) as usize;
            if line_display_len > 0 {
                line_display_column_start =
                    line_display_column_start.min(line_display_len.saturating_sub(1));
            } else {
                line_display_column_start = 0;
            }

            let target_end_column = line_display_column_start.saturating_add(MAX_LINE_LEN);
            let line_display_column_end = display_snapshot
                .clip_point(
                    crate::display_map::DisplayPoint::new(row, target_end_column as u32),
                    text::Bias::Right,
                )
                .column() as usize;

            if line_display_column_start < line_display_column_end {
                let chunk_start =
                    crate::display_map::TabPoint::new(row.0, line_display_column_start as u32);
                let chunk_end =
                    crate::display_map::TabPoint::new(row.0, line_display_column_end as u32);
                for line_chunk in display_snapshot
                    .tab_snapshot()
                    .chunks(chunk_start..chunk_end)
                {
                    let line_chunk_text = line_chunk.text;
                    let (mut chunk, has_newline) = if let Some(index) = line_chunk_text.find('\n') {
                        (&line_chunk_text[..index], true)
                    } else {
                        (line_chunk_text, false)
                    };

                    if !chunk.is_empty() {
                        if line_text.len() < MAX_LINE_LEN {
                            let remaining_capacity = MAX_LINE_LEN - line_text.len();
                            let mut bounded_end = remaining_capacity.min(chunk.len());
                            while bounded_end > 0 && !chunk.is_char_boundary(bounded_end) {
                                bounded_end -= 1;
                            }
                            if bounded_end > 0 {
                                chunk = &chunk[..bounded_end];
                                line_text.push_str(chunk);
                            }
                        }
                    }

                    if has_newline || line_text.len() >= MAX_LINE_LEN {
                        break;
                    }
                }
            }

            line_display_column_start
        };

        let (expanded, text_color): (gpui::SharedString, _) = if !has_content && row.0 == 0 {
            (placeholder.clone(), cx.theme().colors().text_placeholder)
        } else if masked {
            (mask_line(&line_text).into(), style.color)
        } else {
            (line_text.clone().into(), style.color)
        };
        let expanded_len = expanded.len();

        let origin = point(
            bounds.left() - scroll_x_pixels + em_layout_width * line_display_column_start as f32,
            first_row_origin_y + line_height * visible_row_index as f32,
        );

        let base_run = TextRun {
            len: expanded_len,
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };

        let runs = if let Some(marked_range) = marked_range {
            let marked_start = marked_range.start.max(line_start_offset.0);
            let marked_end = marked_range.end.min(line_end_offset.0);
            if marked_start < marked_end {
                let start_column = (marked_start - line_start_offset.0) as u32;
                let end_column = (marked_end - line_start_offset.0) as u32;
                let (mut display_start, mut display_end) = if masked {
                    let start_column = (start_column as usize).min(line_text.len());
                    let end_column = (end_column as usize).min(line_text.len());
                    (
                        line_text.get(..start_column).unwrap_or("").chars().count(),
                        line_text.get(..end_column).unwrap_or("").chars().count(),
                    )
                } else {
                    (
                        display_snapshot
                            .point_to_display_point(
                                text::Point::new(row.0, start_column),
                                text::Bias::Left,
                            )
                            .column() as usize,
                        display_snapshot
                            .point_to_display_point(
                                text::Point::new(row.0, end_column),
                                text::Bias::Right,
                            )
                            .column() as usize,
                    )
                };
                if !masked {
                    display_start = display_start.saturating_sub(line_display_column_start);
                    display_end = display_end.saturating_sub(line_display_column_start);
                }
                display_start = display_start.min(expanded_len);
                display_end = display_end.min(expanded_len);

                let mut runs = Vec::new();
                if display_start > 0 {
                    runs.push(TextRun {
                        len: display_start,
                        ..base_run.clone()
                    });
                }
                if display_end > display_start {
                    runs.push(TextRun {
                        len: display_end - display_start,
                        underline: Some(UnderlineStyle {
                            color: Some(base_run.color),
                            thickness: px(1.),
                            wavy: false,
                        }),
                        ..base_run.clone()
                    });
                }
                if display_end < expanded_len {
                    runs.push(TextRun {
                        len: expanded_len - display_end,
                        ..base_run.clone()
                    });
                }

                runs
            } else {
                vec![base_run.clone()]
            }
        } else {
            vec![base_run.clone()]
        };

        let shaped_line = window
            .text_system()
            .shape_line(expanded, font_size, &runs, None);
        let width = shaped_line.x_for_index(expanded_len);
        let line_text = if masked { line_text } else { String::new() };

        lines.push(LineWithInvisibles {
            row,
            origin,
            line_start_offset: line_start_offset.0,
            line_end_offset: line_end_offset.0,
            line_display_column_start,
            len: expanded_len,
            width,
            line_text,
            shaped_line,
        });
    }

    lines
}

pub fn register_action<T: Action>(
    editor: &Entity<Editor>,
    window: &mut Window,
    listener: impl Fn(&mut Editor, &T, &mut Window, &mut Context<Editor>) + 'static,
) {
    let editor = editor.clone();
    window.on_action(TypeId::of::<T>(), move |action, phase, window, cx| {
        if phase == DispatchPhase::Bubble {
            let action = action.downcast_ref().expect("action type to match");
            editor.update(cx, |editor, cx| {
                listener(editor, action, window, cx);
            });
        }
    })
}

fn measure_column_width(style: &gpui::TextStyle, window: &mut Window) -> Pixels {
    let sample_text: gpui::SharedString = " ".into();
    let run = gpui::TextRun {
        len: sample_text.len(),
        font: style.font(),
        color: style.color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };

    let font_size = style.font_size.to_pixels(window.rem_size());
    let shaped = window
        .text_system()
        .shape_line(sample_text, font_size, &[run], None);

    let width = shaped.x_for_index(1);
    if width == px(0.) { px(8.) } else { width }
}

fn mask_line(text: &str) -> String {
    let mut masked = String::with_capacity(text.len());
    for _ in text.chars() {
        masked.push('*');
    }
    masked
}
