use gpui::{
    AbsoluteLength, Action, App, Axis, BorderStyle, Bounds, ContentMask, Context, Corners,
    CursorStyle, DispatchPhase, Edges, Element, ElementId, ElementInputHandler, Entity,
    GlobalElementId, Hitbox, HitboxBehavior, InspectorElementId, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, ScrollDelta,
    ScrollWheelEvent, ShapedLine, SharedString, Size, Style, TextAlign, TextRun, TextStyle,
    UnderlineStyle, Window, prelude::*,
};
use multi_buffer::{MultiBufferOffset, MultiBufferRow};
use std::{any::TypeId, ops::Range};
use theme::ActiveTheme;

use crate::{Editor, EditorMode, EditorStyle, HandleInput, MAX_LINE_LEN, SizingBehavior};

const SCROLLBAR_THICKNESS: Pixels = gpui::px(15.);
const SCROLLBAR_MIN_THUMB_LEN: Pixels = gpui::px(25.);

pub(crate) struct PositionMap {
    pub size: Size<Pixels>,
    pub bounds: Bounds<Pixels>,
    pub line_height: Pixels,
    pub scroll_position: Point<crate::scroll::ScrollOffset>,
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
    pub(crate) fn point_for_position(&self, position: Point<Pixels>) -> PointForPosition {
        let local_position = position - self.bounds.origin;
        let y = local_position.y.max(gpui::px(0.)).min(self.size.height);
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
                (display_column as u32, gpui::px(0.))
            } else {
                let display_column = line
                    .line_display_column_start
                    .saturating_add(line.len)
                    .min(u32::MAX as usize);
                (
                    display_column as u32,
                    gpui::px(0.).max(x_relative_to_text - line.width),
                )
            }
        } else {
            (0, x.max(gpui::px(0.)))
        };

        let mut exact_unclipped =
            crate::display_map::DisplayPoint::new(crate::display_map::DisplayRow(row), column);
        let previous_valid = self.snapshot.clip_point(exact_unclipped, text::Bias::Left);
        let next_valid = self.snapshot.clip_point(exact_unclipped, text::Bias::Right);

        let column_overshoot_after_line_end = if self.em_layout_width == gpui::px(0.) {
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
    pub origin: Point<Pixels>,
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
            TextAlign::Left => gpui::px(0.),
            TextAlign::Center => ((content_width - self.width) / 2.).max(gpui::px(0.)),
            TextAlign::Right => (content_width - self.width).max(gpui::px(0.)),
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

    fn rem_size(&self, cx: &mut App) -> Option<Pixels> {
        match self.editor.read(cx).mode {
            EditorMode::Full {
                scale_ui_elements_with_buffer_font_size: true,
                ..
            } => {
                let buffer_font_size = self.style.text.font_size;
                match buffer_font_size {
                    AbsoluteLength::Pixels(pixels) => {
                        let default_font_size_scale = 14. / ui::BASE_REM_SIZE_IN_PX;
                        let default_font_size_delta = 1. - default_font_size_scale;
                        let rem_size_scale = 1. + default_font_size_delta;
                        Some(pixels * rem_size_scale)
                    }
                    AbsoluteLength::Rems(rems) => Some(rems.to_pixels(16f32.into())),
                }
            }
            _ => None,
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
    vertical_scrollbar: Option<ScrollbarPrepaint>,
    horizontal_scrollbar: Option<ScrollbarPrepaint>,
    line_height: Pixels,
    column_width: Pixels,
    scroll_max: Point<crate::scroll::ScrollOffset>,
    masked: bool,
    needs_scroll_clamp: bool,
    clamped_scroll_position: Point<crate::scroll::ScrollOffset>,
}

#[derive(Clone)]
struct ScrollbarPrepaint {
    track_bounds: Bounds<Pixels>,
    thumb_bounds: Option<Bounds<Pixels>>,
    track_hitbox: Hitbox,
    thumb_hitbox: Option<Hitbox>,
    track_quad: PaintQuad,
    thumb_quad: Option<PaintQuad>,
    scroll_max: crate::scroll::ScrollOffset,
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
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let rem_size = self.rem_size(cx);
        window.with_rem_size(rem_size, |window| {
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
        })
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let rem_size = self.rem_size(cx);
        window.with_rem_size(rem_size, |window| {
            let style = self.style.text.clone();
            let rem_size = window.rem_size();
            let font_id = window.text_system().resolve_font(&style.font());
            let font_size = style.font_size.to_pixels(rem_size);
            let line_height = style.line_height_in_pixels(rem_size);
            let fallback_column_width = measure_column_width(&style, window);
            let em_width = window
                .text_system()
                .em_width(font_id, font_size)
                .unwrap_or(fallback_column_width);
            let column_width = window
                .text_system()
                .em_advance(font_id, font_size)
                .unwrap_or(fallback_column_width);
            let em_layout_width = window.text_system().em_layout_width(font_id, font_size);
            let right_padding = em_width * 2.0;

            let (
                focus_handle,
                placeholder,
                mode,
                masked,
                scrollbar_drag,
                selection_range,
                marked_range,
                cursor_offset,
            ) = {
                let editor = self.editor.read(cx);
                (
                    editor.focus_handle.clone(),
                    editor.placeholder.clone(),
                    editor.mode.clone(),
                    editor.masked,
                    editor.scrollbar_drag,
                    editor.selected_range.clone(),
                    editor.marked_range.clone(),
                    editor.cursor_offset(),
                )
            };
            let display_snapshot = self
                .editor
                .update(cx, |editor, cx| editor.display_snapshot(cx));
            let height_in_lines = f64::from(bounds.size.height / line_height);
            let max_row = display_snapshot.buffer_snapshot().max_point().row as f64;
            let max_scroll_y = if matches!(
                mode,
                EditorMode::SingleLine
                    | EditorMode::AutoHeight { .. }
                    | EditorMode::Full {
                        sizing_behavior: SizingBehavior::ExcludeOverscrollMargin
                            | SizingBehavior::SizeByContent,
                        ..
                    }
            ) {
                (max_row - height_in_lines + 1.0).max(0.0)
            } else {
                max_row
            };
            let viewport_rows = height_in_lines;

            let show_vertical_scrollbar = matches!(mode, EditorMode::Full { .. });
            let viewport_width = (bounds.size.width
                - right_padding
                - if show_vertical_scrollbar {
                    SCROLLBAR_THICKNESS
                } else {
                    gpui::px(0.)
                })
            .max(gpui::px(0.));

            let longest_row = display_snapshot.longest_row();
            let content_columns = f64::from(display_snapshot.line_len(longest_row));
            let viewport_columns = (viewport_width / column_width) as f64;
            let scrollable_columns = content_columns;
            let max_scroll_x = (scrollable_columns - viewport_columns).max(0.0);

            let scroll_max = gpui::point(max_scroll_x, max_scroll_y);
            let scroll_width = column_width * scrollable_columns as f32;
            let (autoscroll_request, needs_horizontal_autoscroll, mut scroll_position) =
                self.editor.update(cx, |editor, cx| {
                    let autoscroll_request = editor.scroll_manager.take_autoscroll_request();
                    let (needs_horizontal_autoscroll, _) = editor.autoscroll_vertically(
                        bounds,
                        line_height,
                        max_scroll_y,
                        autoscroll_request,
                        window,
                        cx,
                    );
                    let scroll_position = editor.scroll_position(&display_snapshot);
                    (
                        autoscroll_request,
                        needs_horizontal_autoscroll,
                        scroll_position,
                    )
                });

            let show_horizontal_scrollbar = max_scroll_x > 0.0;

            let max_display_row = display_snapshot.buffer_snapshot().max_point().row;

            let cursor_point =
                display_snapshot
                    .buffer_snapshot()
                    .offset_to_point(MultiBufferOffset(
                        cursor_offset.min(display_snapshot.buffer_snapshot().len().0),
                    ));
            let cursor_row = cursor_point.row;

            let has_content = !display_snapshot.buffer_snapshot().is_empty();

            let clamped_scroll_position_y = scroll_position.y.clamp(0.0, scroll_max.y);
            let row_offset_y = clamped_scroll_position_y - clamped_scroll_position_y.floor();
            let scroll_y_pixels = line_height * row_offset_y as f32;

            let top_row = clamped_scroll_position_y.floor().max(0.0) as u32;
            let first_row_origin_y = bounds.top() - scroll_y_pixels;
            let visible_row_count =
                ((bounds.bottom() - bounds.top()) / line_height).ceil() as u32 + 1;
            let mut line_scroll_x = scroll_position.x.clamp(0.0, max_scroll_x);
            let mut lines = build_visible_lines(
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

            if needs_horizontal_autoscroll.0 && max_scroll_x > 0.0 {
                scroll_position = self.editor.update(cx, |editor, cx| {
                    editor.autoscroll_horizontally(
                        crate::display_map::DisplayRow(top_row),
                        viewport_width,
                        scroll_width,
                        column_width,
                        &lines,
                        autoscroll_request,
                        window,
                        cx,
                    );
                    editor.scroll_position(&display_snapshot)
                });
                let updated_line_scroll_x = scroll_position.x.clamp(0.0, max_scroll_x);
                if (updated_line_scroll_x - line_scroll_x).abs() > f64::EPSILON {
                    line_scroll_x = updated_line_scroll_x;
                    lines = build_visible_lines(
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
                }
            }

            let clamped_scroll_position = gpui::point(
                scroll_position.x.clamp(0.0, scroll_max.x),
                scroll_position.y.clamp(0.0, scroll_max.y),
            );
            let needs_scroll_clamp = scroll_position != clamped_scroll_position;
            let clamped_line_scroll_x = clamped_scroll_position.x.clamp(0.0, max_scroll_x);
            if (clamped_line_scroll_x - line_scroll_x).abs() > f64::EPSILON {
                line_scroll_x = clamped_line_scroll_x;
                lines = build_visible_lines(
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
            }

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
                            gpui::point(
                                line.origin.x + line.x_for_index(display_start),
                                line.origin.y,
                            ),
                            gpui::point(
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
                        Bounds::new(
                            gpui::point(cursor_x, line.origin.y),
                            gpui::size(gpui::px(2.), line_height),
                        ),
                        cx.theme().colors().editor_foreground,
                    ));
                }
            }

            let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
            window.set_focus_handle(&focus_handle, cx);

            let is_dragging_vertical =
                scrollbar_drag.is_some_and(|drag| drag.axis == Axis::Vertical);
            let is_dragging_horizontal =
                scrollbar_drag.is_some_and(|drag| drag.axis == Axis::Horizontal);

            let vertical_scrollbar = if show_vertical_scrollbar {
                Some(prepaint_scrollbar(
                    Axis::Vertical,
                    bounds,
                    show_horizontal_scrollbar,
                    is_dragging_vertical,
                    clamped_scroll_position.y,
                    max_scroll_y,
                    viewport_rows,
                    cx,
                    window,
                ))
            } else {
                None
            };

            let horizontal_scrollbar = if show_horizontal_scrollbar {
                Some(prepaint_scrollbar(
                    Axis::Horizontal,
                    bounds,
                    show_vertical_scrollbar,
                    is_dragging_horizontal,
                    clamped_scroll_position.x,
                    max_scroll_x,
                    viewport_columns,
                    cx,
                    window,
                ))
            } else {
                None
            };

            PrepaintState {
                line_layouts: lines,
                display_snapshot,
                cursor,
                selections,
                hitbox: Some(hitbox),
                vertical_scrollbar,
                horizontal_scrollbar,
                line_height,
                column_width,
                scroll_max,
                masked,
                needs_scroll_clamp,
                clamped_scroll_position,
            }
        })
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let rem_size = self.rem_size(cx);
        window.with_rem_size(rem_size, |window| {
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
            let column_width = prepaint.column_width;
            let scroll_max = prepaint.scroll_max;
            let clamped_scroll_position = prepaint.clamped_scroll_position;

            let vertical_scrollbar_prepaint = prepaint.vertical_scrollbar.as_ref().cloned();
            let horizontal_scrollbar_prepaint = prepaint.horizontal_scrollbar.as_ref().cloned();
            let rem_size = window.rem_size();
            let text_style = self.style.text.clone();
            let font_id = window.text_system().resolve_font(&text_style.font());
            let font_size = text_style.font_size.to_pixels(rem_size);
            let em_width = window
                .text_system()
                .em_width(font_id, font_size)
                .unwrap_or(column_width);
            let em_layout_width = window.text_system().em_layout_width(font_id, font_size);

            if let Some(scrollbar) = vertical_scrollbar_prepaint.as_ref() {
                window.set_cursor_style(CursorStyle::Arrow, &scrollbar.track_hitbox);
                if let Some(thumb_hitbox) = scrollbar.thumb_hitbox.as_ref() {
                    window.set_cursor_style(CursorStyle::Arrow, thumb_hitbox);
                }
            }
            if let Some(scrollbar) = horizontal_scrollbar_prepaint.as_ref() {
                window.set_cursor_style(CursorStyle::Arrow, &scrollbar.track_hitbox);
                if let Some(thumb_hitbox) = scrollbar.thumb_hitbox.as_ref() {
                    window.set_cursor_style(CursorStyle::Arrow, thumb_hitbox);
                }
            }
            if self.editor.read(cx).scrollbar_drag.is_some() {
                window.set_window_cursor_style(CursorStyle::Arrow);
            }

            window.on_mouse_event({
                let editor = self.editor.clone();
                let hitbox = hitbox.clone();

                move |event: &ScrollWheelEvent, phase, window, cx| {
                    if phase != DispatchPhase::Bubble {
                        return;
                    }
                    if !hitbox.should_handle_scroll(window) {
                        return;
                    }

                    editor.update(cx, |editor, cx| {
                        let snapshot = editor.display_snapshot(cx);
                        let current = editor.scroll_position(&snapshot);
                        let (delta_x, delta_y) = match event.delta {
                            ScrollDelta::Pixels(mut pixels) => {
                                let axis =
                                    editor.scroll_manager.ongoing_scroll().filter(&mut pixels);
                                editor.scroll_manager.update_ongoing_scroll(axis);
                                (
                                    (pixels.x / column_width) as f64,
                                    (pixels.y / line_height) as f64,
                                )
                            }
                            ScrollDelta::Lines(lines) => {
                                editor.scroll_manager.update_ongoing_scroll(None);
                                (lines.x as f64, lines.y as f64)
                            }
                        };

                        let next = gpui::point(
                            (current.x - delta_x).clamp(0.0, scroll_max.x),
                            (current.y - delta_y).clamp(0.0, scroll_max.y),
                        );

                        if next != current {
                            editor.set_scroll_position(&snapshot, next, cx);
                            cx.stop_propagation();
                        }
                    });
                }
            });

            window.on_mouse_event({
                let editor = self.editor.clone();
                let hitbox = hitbox.clone();
                let vertical_scrollbar_prepaint = vertical_scrollbar_prepaint.clone();
                let horizontal_scrollbar_prepaint = horizontal_scrollbar_prepaint.clone();

                move |event: &MouseDownEvent, phase, window, cx| {
                    if phase != DispatchPhase::Bubble {
                        return;
                    }

                    if event.button != MouseButton::Left {
                        return;
                    }

                    if let Some(scrollbar) = vertical_scrollbar_prepaint.as_ref()
                        && let Some(thumb_hitbox) = scrollbar.thumb_hitbox.as_ref()
                        && thumb_hitbox.is_hovered(window)
                        && let Some(thumb_bounds) = scrollbar.thumb_bounds
                    {
                        let pointer_offset = event.position.y - thumb_bounds.top();
                        editor.update(cx, |editor, cx| {
                            editor.focus_handle.focus(window, cx);
                            editor.selecting = false;
                            editor.scrollbar_drag = Some(crate::ScrollbarDrag {
                                axis: Axis::Vertical,
                                pointer_offset,
                            });
                            cx.stop_propagation();
                            cx.notify();
                        });
                        return;
                    }

                    if let Some(scrollbar) = horizontal_scrollbar_prepaint.as_ref()
                        && let Some(thumb_hitbox) = scrollbar.thumb_hitbox.as_ref()
                        && thumb_hitbox.is_hovered(window)
                        && let Some(thumb_bounds) = scrollbar.thumb_bounds
                    {
                        let pointer_offset = event.position.x - thumb_bounds.left();
                        editor.update(cx, |editor, cx| {
                            editor.focus_handle.focus(window, cx);
                            editor.selecting = false;
                            editor.scrollbar_drag = Some(crate::ScrollbarDrag {
                                axis: Axis::Horizontal,
                                pointer_offset,
                            });
                            cx.stop_propagation();
                            cx.notify();
                        });
                        return;
                    }

                    if let Some(scrollbar) = vertical_scrollbar_prepaint.as_ref()
                        && scrollbar.track_hitbox.is_hovered(window)
                    {
                        let scrollbar = scrollbar.clone();
                        editor.update(cx, |editor, cx| {
                            editor.focus_handle.focus(window, cx);
                            editor.selecting = false;
                            editor.scrollbar_drag = None;

                            if let Some(thumb_bounds) = scrollbar.thumb_bounds {
                                let track_bounds = scrollbar.track_bounds;
                                let thumb_len =
                                    (thumb_bounds.bottom() - thumb_bounds.top()).max(gpui::px(0.));
                                let pointer_offset = thumb_len / 2.0;
                                let available =
                                    (track_bounds.bottom() - track_bounds.top() - thumb_len)
                                        .max(gpui::px(0.));
                                let desired_thumb_start = (event.position.y - thumb_len / 2.0)
                                    .clamp(track_bounds.top(), track_bounds.bottom() - thumb_len);
                                let fraction = if available == gpui::px(0.) {
                                    0.0
                                } else {
                                    ((desired_thumb_start - track_bounds.top()) / available) as f64
                                };

                                let snapshot = editor.display_snapshot(cx);
                                let current = editor.scroll_position(&snapshot);
                                let next = gpui::point(current.x, fraction * scrollbar.scroll_max);
                                if next != current {
                                    editor.set_scroll_position(&snapshot, next, cx);
                                }

                                editor.scrollbar_drag = Some(crate::ScrollbarDrag {
                                    axis: Axis::Vertical,
                                    pointer_offset,
                                });
                            }

                            cx.stop_propagation();
                            cx.notify();
                        });
                        return;
                    }

                    if let Some(scrollbar) = horizontal_scrollbar_prepaint.as_ref()
                        && scrollbar.track_hitbox.is_hovered(window)
                    {
                        let scrollbar = scrollbar.clone();
                        editor.update(cx, |editor, cx| {
                            editor.focus_handle.focus(window, cx);
                            editor.selecting = false;
                            editor.scrollbar_drag = None;

                            if let Some(thumb_bounds) = scrollbar.thumb_bounds {
                                let track_bounds = scrollbar.track_bounds;
                                let thumb_len =
                                    (thumb_bounds.right() - thumb_bounds.left()).max(gpui::px(0.));
                                let pointer_offset = thumb_len / 2.0;
                                let available =
                                    (track_bounds.right() - track_bounds.left() - thumb_len)
                                        .max(gpui::px(0.));
                                let desired_thumb_start = (event.position.x - thumb_len / 2.0)
                                    .clamp(track_bounds.left(), track_bounds.right() - thumb_len);
                                let fraction = if available == gpui::px(0.) {
                                    0.0
                                } else {
                                    ((desired_thumb_start - track_bounds.left()) / available) as f64
                                };

                                let snapshot = editor.display_snapshot(cx);
                                let current = editor.scroll_position(&snapshot);
                                let next = gpui::point(fraction * scrollbar.scroll_max, current.y);
                                if next != current {
                                    editor.set_scroll_position(&snapshot, next, cx);
                                }

                                editor.scrollbar_drag = Some(crate::ScrollbarDrag {
                                    axis: Axis::Horizontal,
                                    pointer_offset,
                                });
                            }

                            cx.stop_propagation();
                            cx.notify();
                        });
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
                        if editor.scrollbar_drag.is_some() {
                            editor.scrollbar_drag = None;
                            cx.stop_propagation();
                            cx.notify();
                            return;
                        }
                        if editor.selecting || hitbox.is_hovered(window) {
                            editor.on_mouse_up(event, window, cx);
                        }
                    });
                }
            });

            window.on_mouse_event({
                let editor = self.editor.clone();
                let vertical_scrollbar_prepaint = vertical_scrollbar_prepaint.clone();
                let horizontal_scrollbar_prepaint = horizontal_scrollbar_prepaint.clone();

                move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase != DispatchPhase::Bubble {
                        return;
                    }

                    editor.update(cx, |editor, cx| {
                        if let Some(drag) = editor.scrollbar_drag {
                            let (track_bounds, thumb_bounds, scrollbar_scroll_max) = match drag.axis
                            {
                                Axis::Vertical => {
                                    let Some(scrollbar) = vertical_scrollbar_prepaint.as_ref()
                                    else {
                                        return;
                                    };
                                    let Some(thumb_bounds) = scrollbar.thumb_bounds else {
                                        return;
                                    };
                                    (scrollbar.track_bounds, thumb_bounds, scrollbar.scroll_max)
                                }
                                Axis::Horizontal => {
                                    let Some(scrollbar) = horizontal_scrollbar_prepaint.as_ref()
                                    else {
                                        return;
                                    };
                                    let Some(thumb_bounds) = scrollbar.thumb_bounds else {
                                        return;
                                    };
                                    (scrollbar.track_bounds, thumb_bounds, scrollbar.scroll_max)
                                }
                            };

                            let (track_start, track_end, thumb_len, mouse_pos) = match drag.axis {
                                Axis::Vertical => (
                                    track_bounds.top(),
                                    track_bounds.bottom(),
                                    thumb_bounds.bottom() - thumb_bounds.top(),
                                    event.position.y,
                                ),
                                Axis::Horizontal => (
                                    track_bounds.left(),
                                    track_bounds.right(),
                                    thumb_bounds.right() - thumb_bounds.left(),
                                    event.position.x,
                                ),
                            };

                            let available = (track_end - track_start - thumb_len).max(gpui::px(0.));
                            let desired_thumb_start = (mouse_pos - drag.pointer_offset)
                                .clamp(track_start, track_end - thumb_len);
                            let fraction = if available == gpui::px(0.) {
                                0.0
                            } else {
                                ((desired_thumb_start - track_start) / available) as f64
                            };
                            let snapshot = editor.display_snapshot(cx);
                            let current = editor.scroll_position(&snapshot);
                            let next = match drag.axis {
                                Axis::Vertical => {
                                    gpui::point(current.x, fraction * scrollbar_scroll_max)
                                }
                                Axis::Horizontal => {
                                    gpui::point(fraction * scrollbar_scroll_max, current.y)
                                }
                            };

                            editor.set_scroll_position(&snapshot, next, cx);
                            cx.stop_propagation();
                            return;
                        }

                        if !editor.selecting {
                            return;
                        }

                        editor.on_mouse_move(event, window, cx);

                        let mut scroll_delta = Point::<f32>::default();
                        let mut text_bounds = bounds;
                        if vertical_scrollbar_prepaint.is_some() {
                            text_bounds.size.width =
                                (text_bounds.size.width - SCROLLBAR_THICKNESS).max(gpui::px(0.));
                        }
                        if horizontal_scrollbar_prepaint.is_some() {
                            text_bounds.size.height =
                                (text_bounds.size.height - SCROLLBAR_THICKNESS).max(gpui::px(0.));
                        }

                        let vertical_margin = line_height.min(text_bounds.size.height / 3.0);
                        let top = text_bounds.top() + vertical_margin;
                        let bottom = text_bounds.bottom() - vertical_margin;
                        if event.position.y < top {
                            scroll_delta.y =
                                -scale_vertical_mouse_autoscroll_delta(top - event.position.y);
                        }
                        if event.position.y > bottom {
                            scroll_delta.y =
                                scale_vertical_mouse_autoscroll_delta(event.position.y - bottom);
                        }

                        let scroll_space = em_width * 5.0;
                        let left = text_bounds.left() + scroll_space;
                        let right = text_bounds.right() - scroll_space;
                        if event.position.x < left {
                            scroll_delta.x =
                                -scale_horizontal_mouse_autoscroll_delta(left - event.position.x);
                        }
                        if event.position.x > right {
                            scroll_delta.x =
                                scale_horizontal_mouse_autoscroll_delta(event.position.x - right);
                        }

                        if scroll_delta.x == 0.0 && scroll_delta.y == 0.0 {
                            return;
                        }

                        let snapshot = editor.display_snapshot(cx);
                        let current = editor.scroll_position(&snapshot);
                        let next = gpui::point(
                            (current.x + f64::from(scroll_delta.x)).clamp(0.0, scroll_max.x),
                            (current.y + f64::from(scroll_delta.y)).clamp(0.0, scroll_max.y),
                        );

                        if next != current {
                            editor.set_scroll_position(&snapshot, next, cx);
                            cx.stop_propagation();
                        }
                    });
                }
            });

            if !self.style.background.is_transparent() {
                window.paint_quad(gpui::fill(bounds, self.style.background));
            }

            let mut text_bounds = bounds;
            if vertical_scrollbar_prepaint.is_some() {
                text_bounds.size.width =
                    (text_bounds.size.width - SCROLLBAR_THICKNESS).max(gpui::px(0.));
            }
            if horizontal_scrollbar_prepaint.is_some() {
                text_bounds.size.height =
                    (text_bounds.size.height - SCROLLBAR_THICKNESS).max(gpui::px(0.));
            }

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

            if let Some(scrollbar) = prepaint.vertical_scrollbar.take() {
                window.paint_quad(scrollbar.track_quad);
                if let Some(thumb_quad) = scrollbar.thumb_quad {
                    window.paint_quad(thumb_quad);
                }
            }
            if let Some(scrollbar) = prepaint.horizontal_scrollbar.take() {
                window.paint_quad(scrollbar.track_quad);
                if let Some(thumb_quad) = scrollbar.thumb_quad {
                    window.paint_quad(thumb_quad);
                }
            }

            self.editor.update(cx, |editor, _cx| {
                editor.last_position_map = Some(std::rc::Rc::new(PositionMap {
                    size: bounds.size,
                    bounds,
                    line_height,
                    scroll_position: clamped_scroll_position,
                    em_layout_width,
                    snapshot: prepaint.display_snapshot.clone(),
                    text_align: TextAlign::Left,
                    content_width: bounds.size.width,
                    masked: prepaint.masked,
                    line_layouts: std::mem::take(&mut prepaint.line_layouts),
                }));
            });

            if prepaint.needs_scroll_clamp {
                self.editor.update(cx, |editor, cx| {
                    let snapshot = editor.display_snapshot(cx);
                    editor.set_scroll_position(&snapshot, clamped_scroll_position, cx);
                });
            }
        });
    }
}

fn build_visible_lines(
    display_snapshot: &crate::display_map::DisplaySnapshot,
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    style: &TextStyle,
    font_size: Pixels,
    placeholder: &SharedString,
    masked: bool,
    marked_range: Option<&Range<usize>>,
    max_display_row: u32,
    top_row: u32,
    visible_row_count: u32,
    line_scroll_x: crate::scroll::ScrollOffset,
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

        let (expanded, text_color): (SharedString, _) = if !has_content && row.0 == 0 {
            (placeholder.clone(), cx.theme().colors().text_placeholder)
        } else if masked {
            (mask_line(&line_text).into(), style.color)
        } else {
            (line_text.clone().into(), style.color)
        };
        let expanded_len = expanded.len();

        let origin = gpui::point(
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
                            thickness: gpui::px(1.),
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

fn scale_vertical_mouse_autoscroll_delta(delta: Pixels) -> f32 {
    (delta.pow(1.2) / 100.0).min(gpui::px(3.0)).into()
}

fn scale_horizontal_mouse_autoscroll_delta(delta: Pixels) -> f32 {
    (delta.pow(1.2) / 300.0).into()
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

fn measure_column_width(style: &TextStyle, window: &mut Window) -> Pixels {
    let sample_text: SharedString = " ".into();
    let run = TextRun {
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
    if width == gpui::px(0.) {
        gpui::px(8.)
    } else {
        width
    }
}

fn mask_line(text: &str) -> String {
    let mut masked = String::with_capacity(text.len());
    for _ in text.chars() {
        masked.push('*');
    }
    masked
}

fn prepaint_scrollbar(
    axis: Axis,
    bounds: Bounds<Pixels>,
    other_axis_present: bool,
    is_dragging: bool,
    scroll_position: crate::scroll::ScrollOffset,
    scroll_max: crate::scroll::ScrollOffset,
    viewport_size: f64,
    cx: &App,
    window: &mut Window,
) -> ScrollbarPrepaint {
    let thickness = SCROLLBAR_THICKNESS;

    let track_bounds = match axis {
        Axis::Vertical => Bounds::new(
            gpui::point(bounds.right() - thickness, bounds.top()),
            gpui::size(thickness, bounds.size.height),
        ),
        Axis::Horizontal => {
            let width = (bounds.size.width
                - if other_axis_present {
                    thickness
                } else {
                    gpui::px(0.)
                })
            .max(gpui::px(0.));
            Bounds::new(
                gpui::point(bounds.left(), bounds.bottom() - thickness),
                gpui::size(width, thickness),
            )
        }
    };
    let track_length = match axis {
        Axis::Vertical => track_bounds.bottom() - track_bounds.top(),
        Axis::Horizontal => track_bounds.right() - track_bounds.left(),
    };

    let track_hitbox = window.insert_hitbox(track_bounds, HitboxBehavior::Normal);

    let colors = cx.theme().colors();
    let scrollbar_edges = match axis {
        Axis::Horizontal => Edges {
            top: gpui::px(0.),
            right: gpui::px(0.),
            bottom: gpui::px(0.),
            left: gpui::px(0.),
        },
        Axis::Vertical => Edges {
            top: gpui::px(0.),
            right: gpui::px(0.),
            bottom: gpui::px(0.),
            left: gpui::px(1.),
        },
    };

    let track_quad = gpui::quad(
        track_bounds,
        Corners::default(),
        colors.scrollbar_track_background,
        scrollbar_edges,
        colors.scrollbar_track_border,
        BorderStyle::Solid,
    );

    let (thumb_bounds, thumb_hitbox, thumb_quad) = if scroll_max > 0.0 {
        let content = scroll_max + viewport_size;
        let ratio = if content <= 0.0 {
            1.0
        } else {
            (viewport_size / content).clamp(0.0, 1.0)
        };
        let thumb_len = (track_length * ratio as f32)
            .max(SCROLLBAR_MIN_THUMB_LEN)
            .min(track_length);
        let available = (track_length - thumb_len).max(gpui::px(0.));
        let thumb_start = available * (scroll_position / scroll_max).clamp(0.0, 1.0) as f32;

        let thumb_bounds = match axis {
            Axis::Vertical => Bounds::new(
                gpui::point(track_bounds.left(), track_bounds.top() + thumb_start),
                gpui::size(thickness, thumb_len),
            ),
            Axis::Horizontal => Bounds::new(
                gpui::point(track_bounds.left() + thumb_start, track_bounds.top()),
                gpui::size(thumb_len, thickness),
            ),
        };
        let thumb_hitbox = window.insert_hitbox(thumb_bounds, HitboxBehavior::Normal);

        let thumb_color = if is_dragging {
            colors.scrollbar_thumb_active_background
        } else if thumb_hitbox.is_hovered(window) {
            colors.scrollbar_thumb_hover_background
        } else {
            colors.scrollbar_thumb_background
        };

        let thumb_quad = gpui::quad(
            thumb_bounds,
            Corners::default(),
            thumb_color,
            scrollbar_edges,
            colors.scrollbar_thumb_border,
            BorderStyle::Solid,
        );

        (Some(thumb_bounds), Some(thumb_hitbox), Some(thumb_quad))
    } else {
        (None, None, None)
    };

    ScrollbarPrepaint {
        track_bounds,
        thumb_bounds,
        track_hitbox,
        thumb_hitbox,
        track_quad,
        thumb_quad,
        scroll_max,
    }
}
