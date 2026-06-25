use gpui::{
    AbsoluteLength, Action, App, Axis, BorderStyle, Bounds, ContentMask, Context, Corners,
    CursorStyle, DispatchPhase, Edges, Element, ElementId, ElementInputHandler, Entity,
    GlobalElementId, Hitbox, HitboxBehavior, Hsla, InspectorElementId, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, PathBuilder, Pixels, Point,
    ScrollDelta, ScrollWheelEvent, ShapedLine, SharedString, Size, Style, TextAlign, TextRun,
    TextStyle, UnderlineStyle, Window, prelude::*,
};
use num_traits::ToPrimitive;
use smallvec::SmallVec;
use std::{
    any::TypeId,
    borrow::Cow,
    cmp::{self, Ordering},
    collections::{BTreeMap, HashMap},
    fmt::Write,
    ops::Range,
    panic,
    rc::Rc,
    sync::Arc,
};

use language::LanguageAwareStyling;
use multi_buffer::{MultiBufferOffset, MultiBufferRow, RowInfo};
use settings::Settings;
use theme::ActiveTheme;
use util::ResultExt;

use crate::{
    CurrentLineHighlight, Editor, EditorMode, EditorSettings, EditorSnapshot, EditorStyle,
    GutterDimensions, MAX_LINE_LEN, ScrollbarDrag, SizingBehavior,
    display_map::{DisplayPoint, DisplayRow, DisplaySnapshot, TabPoint},
    scroll::ScrollOffset,
};

const SCROLLBAR_THICKNESS: Pixels = gpui::px(15.0);
const SCROLLBAR_MIN_THUMB_LEN: Pixels = gpui::px(25.0);

#[derive(Clone, Copy, Default)]
struct LineHighlightSpec {
    selection: bool,
}

#[derive(Debug, Copy, Clone)]
pub(crate) struct PointForPosition {
    pub previous_valid: DisplayPoint,
    pub next_valid: DisplayPoint,
    pub nearest_valid: DisplayPoint,
    pub exact_unclipped: DisplayPoint,
    pub column_overshoot_after_line_end: u32,
}

impl PointForPosition {
    pub(crate) fn as_valid(&self) -> Option<DisplayPoint> {
        if self.previous_valid == self.exact_unclipped && self.next_valid == self.exact_unclipped {
            Some(self.previous_valid)
        } else {
            None
        }
    }
}

pub(crate) struct PositionMap {
    pub size: Size<Pixels>,
    pub line_height: Pixels,
    pub scroll_position: Point<ScrollOffset>,
    pub scroll_max: Point<ScrollOffset>,
    pub em_layout_width: Pixels,
    pub visible_row_range: Range<DisplayRow>,
    pub line_layouts: Vec<LineWithInvisibles>,
    pub snapshot: EditorSnapshot,
    pub text_align: TextAlign,
    pub content_width: Pixels,
    pub text_hitbox: Hitbox,
    pub gutter_hitbox: Hitbox,
    pub masked: bool,
}

impl PositionMap {
    pub(crate) fn point_for_position(&self, position: Point<Pixels>) -> PointForPosition {
        let text_bounds = self.text_hitbox.bounds;
        let local_position = position - text_bounds.origin;
        let y = local_position.y.clamp(gpui::px(0.0), self.size.height);
        let scroll_x_pixels =
            Pixels::from(self.scroll_position.x * f64::from(self.em_layout_width));
        let x = local_position.x + scroll_x_pixels;
        let scroll_y = self.scroll_position.y.max(0.0);
        let row = (f64::from(y / self.line_height) + scroll_y)
            .to_u32()
            .expect("display row should fit in u32");

        let (column, x_overshoot_after_line_end) = if let Some(line_index) =
            row.checked_sub(self.visible_row_range.start.0)
            && let Some(line) = self.line_layouts.get(line_index as usize)
        {
            let x_relative_to_text = x
                - line.alignment_offset(self.text_align, self.content_width)
                - self.em_layout_width
                    * line
                        .line_display_column_start
                        .to_f32()
                        .expect("line display column start should fit in f32");
            if let Some(index) = line.index_for_x(x_relative_to_text) {
                let display_column = line
                    .line_display_column_start
                    .saturating_add(index)
                    .min(u32::MAX as usize);
                (
                    u32::try_from(display_column).expect("display column should fit in u32"),
                    gpui::px(0.0),
                )
            } else {
                let display_column = line
                    .line_display_column_start
                    .saturating_add(line.len)
                    .min(u32::MAX as usize);
                (
                    u32::try_from(display_column).expect("display column should fit in u32"),
                    gpui::px(0.0).max(x_relative_to_text - line.width),
                )
            }
        } else {
            (0, x.max(gpui::px(0.0)))
        };

        let mut exact_unclipped = DisplayPoint::new(DisplayRow(row), column);
        let previous_valid = self.snapshot.clip_point(exact_unclipped, text::Bias::Left);
        let next_valid = self.snapshot.clip_point(exact_unclipped, text::Bias::Right);
        let nearest_valid = previous_valid;

        let column_overshoot_after_line_end = if self.em_layout_width == gpui::px(0.0) {
            0
        } else {
            (x_overshoot_after_line_end / self.em_layout_width)
                .to_u32()
                .expect("column overshoot should fit in u32")
        };
        *exact_unclipped.column_mut() += column_overshoot_after_line_end;

        PointForPosition {
            previous_valid,
            next_valid,
            nearest_valid,
            exact_unclipped,
            column_overshoot_after_line_end,
        }
    }
}

pub(crate) struct LineWithInvisibles {
    pub row: DisplayRow,
    pub origin: Point<Pixels>,
    pub line_start_offset: usize,
    pub line_display_column_start: usize,
    pub len: usize,
    pub width: Pixels,
    pub line_text: String,
    pub shaped_line: ShapedLine,
}

impl LineWithInvisibles {
    pub(crate) fn x_for_index(&self, index: usize) -> Pixels {
        self.shaped_line.x_for_index(index.min(self.len))
    }

    pub(crate) fn index_for_x(&self, x: Pixels) -> Option<usize> {
        self.shaped_line
            .index_for_x(x)
            .map(|index| index.min(self.len))
    }

    pub(crate) fn alignment_offset(&self, text_align: TextAlign, content_width: Pixels) -> Pixels {
        match text_align {
            TextAlign::Left => gpui::px(0.0),
            TextAlign::Center => ((content_width - self.width) / 2.0).max(gpui::px(0.0)),
            TextAlign::Right => (content_width - self.width).max(gpui::px(0.0)),
        }
    }
}

pub struct EditorLayout {
    position_map: Rc<PositionMap>,
    hitbox: Hitbox,
    gutter_hitbox: Hitbox,
    line_numbers: Arc<HashMap<MultiBufferRow, LineNumberLayout>>,
    active_line_background: Option<PaintQuad>,
    cursor: Option<PaintQuad>,
    selection_ranges: Vec<Range<DisplayPoint>>,
    vertical_scrollbar: Option<ScrollbarPrepaint>,
    horizontal_scrollbar: Option<ScrollbarPrepaint>,
    em_width: Pixels,
    column_width: Pixels,
    needs_scroll_clamp: bool,
    clamped_scroll_position: Point<ScrollOffset>,
}

#[derive(Clone)]
struct ScrollbarPrepaint {
    track_bounds: Bounds<Pixels>,
    thumb_bounds: Option<Bounds<Pixels>>,
    track_hitbox: Hitbox,
    thumb_hitbox: Option<Hitbox>,
    track_quad: PaintQuad,
    thumb_quad: Option<PaintQuad>,
    scroll_max: ScrollOffset,
}

#[derive(Debug)]
struct LineNumberSegment {
    shaped_line: ShapedLine,
    hitbox: Option<Hitbox>,
}

#[derive(Debug)]
struct LineNumberLayout {
    segments: SmallVec<[LineNumberSegment; 1]>,
}

struct Gutter<'a> {
    line_height: Pixels,
    range: Range<DisplayRow>,
    scroll_position: Point<ScrollOffset>,
    dimensions: &'a GutterDimensions,
    hitbox: &'a Hitbox,
    snapshot: &'a EditorSnapshot,
    row_infos: &'a [RowInfo],
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
                        let default_font_size_scale = 14.0 / ui::BASE_REM_SIZE_IN_PX;
                        let default_font_size_delta = 1.0 - default_font_size_scale;
                        let rem_size_scale = 1.0 + default_font_size_delta;
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
        register_action(editor, window, Editor::toggle_line_numbers);
        register_action(editor, window, Editor::move_to_beginning_of_line);
        register_action(editor, window, Editor::move_to_end_of_line);
        register_action(editor, window, Editor::select_to_beginning_of_line);
        register_action(editor, window, Editor::select_to_end_of_line);
        register_action(editor, window, Editor::delete_to_beginning_of_line);
        register_action(editor, window, Editor::delete_to_end_of_line);
        register_action(
            editor,
            window,
            |editor, action: &actions::editor::HandleInput, window, cx| {
                editor.handle_input(&action.0, window, cx);
            },
        );
    }

    fn layout_scrollbars(
        bounds: Bounds<Pixels>,
        show_vertical_scrollbar: bool,
        show_horizontal_scrollbar: bool,
        scrollbar_drag: Option<ScrollbarDrag>,
        clamped_scroll_position: Point<ScrollOffset>,
        max_scroll_x: ScrollOffset,
        max_scroll_y: ScrollOffset,
        viewport_columns: f64,
        viewport_rows: f64,
        window: &mut Window,
        cx: &App,
    ) -> (Option<ScrollbarPrepaint>, Option<ScrollbarPrepaint>) {
        let is_dragging_vertical = scrollbar_drag.is_some_and(|drag| drag.axis == Axis::Vertical);
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

        (vertical_scrollbar, horizontal_scrollbar)
    }

    fn paint_highlights(
        layout: &EditorLayout,
        text_bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let corner_radius = 0.15 * layout.position_map.line_height;
        for range in &layout.selection_ranges {
            Self::paint_highlighted_range(
                range.clone(),
                true,
                cx.theme().colors().element_selection_background,
                corner_radius,
                corner_radius * 2.0,
                layout,
                text_bounds,
                window,
            );
        }
    }

    fn layout_line_numbers(
        &self,
        gutter: &Gutter<'_>,
        active_rows: &BTreeMap<DisplayRow, LineHighlightSpec>,
        current_selection_head: Option<DisplayRow>,
        window: &mut Window,
        cx: &mut App,
    ) -> Arc<HashMap<MultiBufferRow, LineNumberLayout>> {
        let _ = current_selection_head;
        let include_line_numbers = gutter
            .snapshot
            .show_line_numbers
            .unwrap_or_else(|| EditorSettings::get_global(cx).gutter.line_numbers);
        if !include_line_numbers {
            return Arc::default();
        }

        let mut line_number = String::new();
        let segments = gutter
            .row_infos
            .iter()
            .enumerate()
            .filter_map(|(index, row_info)| {
                let row_offset = u32::try_from(index).ok()?;
                let display_row = DisplayRow(gutter.range.start.0.checked_add(row_offset)?);
                line_number.clear();
                let number = row_info.buffer_row? + 1;
                write!(&mut line_number, "{number}").expect("writing to string should succeed");

                let color = if active_rows
                    .get(&display_row)
                    .is_some_and(|spec| spec.selection)
                {
                    cx.theme().colors().editor_active_line_number
                } else {
                    cx.theme().colors().editor_line_number
                };
                let shaped_line =
                    self.shape_line_number(SharedString::from(&line_number), color, window);
                let scroll_top = gutter.scroll_position.y * f64::from(gutter.line_height);
                let line_origin = gutter.hitbox.origin
                    + gpui::point(
                        gutter.hitbox.size.width
                            - shaped_line.width
                            - gutter.dimensions.right_padding,
                        Pixels::from(f64::from(row_offset) * f64::from(gutter.line_height))
                            - Pixels::from(scroll_top % f64::from(gutter.line_height)),
                    );
                #[cfg(not(test))]
                let hitbox = Some(window.insert_hitbox(
                    Bounds::new(
                        line_origin,
                        gpui::size(shaped_line.width, gutter.line_height),
                    ),
                    HitboxBehavior::Normal,
                ));
                #[cfg(test)]
                let hitbox = {
                    let _ = line_origin;
                    None
                };
                let segment = LineNumberSegment {
                    shaped_line,
                    hitbox,
                };
                let buffer_row = DisplayPoint::new(display_row, 0)
                    .to_point(gutter.snapshot)
                    .row;
                let multi_buffer_row = MultiBufferRow(buffer_row);

                Some((multi_buffer_row, segment))
            });

        let mut line_numbers: HashMap<MultiBufferRow, LineNumberLayout> = HashMap::default();
        for (buffer_row, segment) in segments {
            line_numbers
                .entry(buffer_row)
                .or_insert_with(|| LineNumberLayout {
                    segments: SmallVec::default(),
                })
                .segments
                .push(segment);
        }

        Arc::new(line_numbers)
    }

    fn paint_line_numbers(layout: &mut EditorLayout, window: &mut Window, cx: &mut App) {
        let line_height = layout.position_map.line_height;
        window.set_cursor_style(CursorStyle::Arrow, &layout.gutter_hitbox);

        for line_layout in layout.line_numbers.values() {
            for LineNumberSegment {
                shaped_line,
                hitbox,
            } in &line_layout.segments
            {
                let Some(hitbox) = hitbox else {
                    continue;
                };

                let Some(()) = shaped_line
                    .paint(
                        hitbox.origin,
                        line_height,
                        TextAlign::Left,
                        None,
                        window,
                        cx,
                    )
                    .log_err()
                else {
                    continue;
                };

                window.set_cursor_style(CursorStyle::Arrow, hitbox);
            }
        }
    }

    fn mouse_left_down(
        editor: &mut Editor,
        event: &MouseDownEvent,
        position_map: &PositionMap,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        if window.default_prevented() {
            return;
        }

        let text_hitbox = &position_map.text_hitbox;
        let gutter_hitbox = &position_map.gutter_hitbox;
        let point_for_position = position_map.point_for_position(event.position);
        let mut click_count = event.click_count;

        if gutter_hitbox.is_hovered(window) {
            click_count = 3;
        } else if !text_hitbox.is_hovered(window) {
            return;
        }

        editor.on_mouse_down(
            event,
            click_count,
            point_for_position.nearest_valid,
            window,
            cx,
        );
    }

    fn shape_line_number(
        &self,
        text: SharedString,
        color: Hsla,
        window: &mut Window,
    ) -> ShapedLine {
        let run = TextRun {
            len: text.len(),
            font: self.style.text.font(),
            color,
            ..Default::default()
        };
        window.text_system().shape_line(
            text,
            self.style.text.font_size.to_pixels(window.rem_size()),
            &[run],
            None,
        )
    }

    fn paint_highlighted_range(
        range: Range<DisplayPoint>,
        fill: bool,
        color: Hsla,
        corner_radius: Pixels,
        line_end_overshoot: Pixels,
        layout: &EditorLayout,
        text_bounds: Bounds<Pixels>,
        window: &mut Window,
    ) {
        if range.start >= range.end {
            return;
        }

        let Some(first_line) = layout.position_map.line_layouts.first() else {
            return;
        };
        let start_row = first_line.row;
        let visible_row_count = u32::try_from(layout.position_map.line_layouts.len())
            .expect("visible row count should fit in u32");
        let end_row = start_row + visible_row_count;
        let row_range = if range.end.column() == 0 {
            cmp::max(range.start.row(), start_row)..cmp::min(range.end.row(), end_row)
        } else {
            cmp::max(range.start.row(), start_row)..cmp::min(range.end.row() + 1, end_row)
        };
        if row_range.start >= row_range.end {
            return;
        }

        let start_index = row_range.start.0.saturating_sub(start_row.0) as usize;
        let Some(first_selected_line) = layout.position_map.line_layouts.get(start_index) else {
            return;
        };
        let start_y = first_selected_line.origin.y;

        let mut lines =
            Vec::with_capacity(row_range.end.0.saturating_sub(row_range.start.0) as usize);
        for row in row_range.start.0..row_range.end.0 {
            let row = DisplayRow(row);
            let line_index = row.0.saturating_sub(start_row.0) as usize;
            let Some(line_layout) = layout.position_map.line_layouts.get(line_index) else {
                continue;
            };

            let start_x = if row == range.start.row() {
                let start_column = range.start.column() as usize;
                let start_column = start_column
                    .saturating_sub(line_layout.line_display_column_start)
                    .min(line_layout.len);
                line_layout.origin.x + line_layout.x_for_index(start_column)
            } else {
                line_layout.origin.x
            };
            let end_x = if row == range.end.row() {
                let end_column = range.end.column() as usize;
                let end_column = end_column
                    .saturating_sub(line_layout.line_display_column_start)
                    .min(line_layout.len);
                line_layout.origin.x + line_layout.x_for_index(end_column)
            } else {
                line_layout.origin.x + line_layout.width + line_end_overshoot
            };
            lines.push(HighlightedRangeLine {
                start_x,
                end_x: end_x.max(start_x),
            });
        }
        if lines.is_empty() {
            return;
        }

        let highlighted_range = HighlightedRange {
            start_y,
            line_height: layout.position_map.line_height,
            lines,
            color,
            corner_radius,
        };
        highlighted_range.paint(fill, text_bounds, window);
    }

    fn paint_mouse_listeners(&mut self, layout: &EditorLayout, window: &mut Window) {
        let hitbox = layout.hitbox.clone();
        let position_map = layout.position_map.clone();
        let text_bounds = position_map.text_hitbox.bounds;
        let line_height = layout.position_map.line_height;
        let em_width = layout.em_width;
        let scroll_max = layout.position_map.scroll_max;

        window.on_mouse_event({
            let editor = self.editor.clone();
            let position_map = position_map.clone();

            move |event: &MouseDownEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble {
                    return;
                }

                if event.button != MouseButton::Left {
                    return;
                }

                editor.update(cx, |editor, cx| {
                    Self::mouse_left_down(editor, event, &position_map, window, cx);
                });
            }
        });

        window.on_mouse_event({
            let editor = self.editor.clone();
            let hitbox = hitbox.clone();
            let position_map = position_map.clone();

            move |event: &MouseUpEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble {
                    return;
                }

                if event.button != MouseButton::Left {
                    return;
                }

                editor.update(cx, |editor, cx| {
                    if editor.selecting
                        || (hitbox.is_hovered(window)
                            && (position_map.text_hitbox.is_hovered(window)
                                || position_map.gutter_hitbox.is_hovered(window)))
                    {
                        editor.on_mouse_up(event, window, cx);
                    }
                });
            }
        });

        window.on_mouse_event({
            let editor = self.editor.clone();
            let position_map = position_map.clone();

            move |event: &MouseMoveEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble {
                    return;
                }

                editor.update(cx, |editor, cx| {
                    if !editor.selecting {
                        return;
                    }

                    let point_for_position = position_map.point_for_position(event.position);
                    editor.on_mouse_move(point_for_position.nearest_valid, window, cx);

                    let mut scroll_delta = Point::<f32>::default();
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
    }

    fn paint_scrollbars(&mut self, layout: &mut EditorLayout, window: &mut Window, cx: &mut App) {
        let editor = self.editor.clone();
        let is_scrollbar_dragging = self.editor.read(cx).scrollbar_drag.is_some();
        let hitbox = layout.hitbox.clone();
        let column_width = layout.column_width;
        let line_height = layout.position_map.line_height;
        let scroll_max = layout.position_map.scroll_max;
        let vertical_scrollbar_prepaint = layout.vertical_scrollbar.clone();
        let horizontal_scrollbar_prepaint = layout.horizontal_scrollbar.clone();

        if let Some(scrollbar) = layout.vertical_scrollbar.as_ref() {
            window.set_cursor_style(CursorStyle::Arrow, &scrollbar.track_hitbox);
            if let Some(thumb_hitbox) = scrollbar.thumb_hitbox.as_ref() {
                window.set_cursor_style(CursorStyle::Arrow, thumb_hitbox);
            }
        }
        if let Some(scrollbar) = layout.horizontal_scrollbar.as_ref() {
            window.set_cursor_style(CursorStyle::Arrow, &scrollbar.track_hitbox);
            if let Some(thumb_hitbox) = scrollbar.thumb_hitbox.as_ref() {
                window.set_cursor_style(CursorStyle::Arrow, thumb_hitbox);
            }
        }
        if is_scrollbar_dragging {
            window.set_window_cursor_style(CursorStyle::Arrow);
        }

        window.on_mouse_event({
            let editor = editor.clone();

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
                            editor.filter_ongoing_scroll(&mut pixels);
                            (
                                f64::from(pixels.x / column_width),
                                f64::from(pixels.y / line_height),
                            )
                        }
                        ScrollDelta::Lines(lines) => {
                            editor.clear_ongoing_scroll();
                            (f64::from(lines.x), f64::from(lines.y))
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
            let editor = editor.clone();
            let vertical_scrollbar_prepaint = vertical_scrollbar_prepaint.clone();
            let horizontal_scrollbar_prepaint = horizontal_scrollbar_prepaint.clone();

            move |event: &MouseDownEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble {
                    return;
                }

                if event.button != MouseButton::Left {
                    return;
                }

                editor.update(cx, |editor, cx| {
                    if let Some(scrollbar) = vertical_scrollbar_prepaint.as_ref()
                        && let Some(thumb_hitbox) = scrollbar.thumb_hitbox.as_ref()
                        && thumb_hitbox.is_hovered(window)
                        && let Some(thumb_bounds) = scrollbar.thumb_bounds
                    {
                        let pointer_offset = event.position.y - thumb_bounds.top();
                        editor.focus_handle.focus(window, cx);
                        editor.selecting = false;
                        editor.scrollbar_drag = Some(ScrollbarDrag {
                            axis: Axis::Vertical,
                            pointer_offset,
                        });
                        cx.stop_propagation();
                        cx.notify();
                        return;
                    }

                    if let Some(scrollbar) = horizontal_scrollbar_prepaint.as_ref()
                        && let Some(thumb_hitbox) = scrollbar.thumb_hitbox.as_ref()
                        && thumb_hitbox.is_hovered(window)
                        && let Some(thumb_bounds) = scrollbar.thumb_bounds
                    {
                        let pointer_offset = event.position.x - thumb_bounds.left();
                        editor.focus_handle.focus(window, cx);
                        editor.selecting = false;
                        editor.scrollbar_drag = Some(ScrollbarDrag {
                            axis: Axis::Horizontal,
                            pointer_offset,
                        });
                        cx.stop_propagation();
                        cx.notify();
                        return;
                    }

                    if let Some(scrollbar) = vertical_scrollbar_prepaint.as_ref()
                        && scrollbar.track_hitbox.is_hovered(window)
                    {
                        let scrollbar = scrollbar.clone();
                        editor.focus_handle.focus(window, cx);
                        editor.selecting = false;
                        editor.scrollbar_drag = None;

                        if let Some(thumb_bounds) = scrollbar.thumb_bounds {
                            let track_bounds = scrollbar.track_bounds;
                            let thumb_len =
                                (thumb_bounds.bottom() - thumb_bounds.top()).max(gpui::px(0.0));
                            let pointer_offset = thumb_len / 2.0;
                            let available =
                                (track_bounds.bottom() - track_bounds.top() - thumb_len)
                                    .max(gpui::px(0.0));
                            let desired_thumb_start = (event.position.y - thumb_len / 2.0)
                                .clamp(track_bounds.top(), track_bounds.bottom() - thumb_len);
                            let fraction = if available == gpui::px(0.0) {
                                0.0
                            } else {
                                f64::from((desired_thumb_start - track_bounds.top()) / available)
                            };

                            let snapshot = editor.display_snapshot(cx);
                            let current = editor.scroll_position(&snapshot);
                            let next = gpui::point(current.x, fraction * scrollbar.scroll_max);
                            if next != current {
                                editor.set_scroll_position(&snapshot, next, cx);
                            }

                            editor.scrollbar_drag = Some(ScrollbarDrag {
                                axis: Axis::Vertical,
                                pointer_offset,
                            });
                        }

                        cx.stop_propagation();
                        cx.notify();
                        return;
                    }

                    if let Some(scrollbar) = horizontal_scrollbar_prepaint.as_ref()
                        && scrollbar.track_hitbox.is_hovered(window)
                    {
                        let scrollbar = scrollbar.clone();
                        editor.focus_handle.focus(window, cx);
                        editor.selecting = false;
                        editor.scrollbar_drag = None;

                        if let Some(thumb_bounds) = scrollbar.thumb_bounds {
                            let track_bounds = scrollbar.track_bounds;
                            let thumb_len =
                                (thumb_bounds.right() - thumb_bounds.left()).max(gpui::px(0.0));
                            let pointer_offset = thumb_len / 2.0;
                            let available =
                                (track_bounds.right() - track_bounds.left() - thumb_len)
                                    .max(gpui::px(0.0));
                            let desired_thumb_start = (event.position.x - thumb_len / 2.0)
                                .clamp(track_bounds.left(), track_bounds.right() - thumb_len);
                            let fraction = if available == gpui::px(0.0) {
                                0.0
                            } else {
                                f64::from((desired_thumb_start - track_bounds.left()) / available)
                            };

                            let snapshot = editor.display_snapshot(cx);
                            let current = editor.scroll_position(&snapshot);
                            let next = gpui::point(fraction * scrollbar.scroll_max, current.y);
                            if next != current {
                                editor.set_scroll_position(&snapshot, next, cx);
                            }

                            editor.scrollbar_drag = Some(ScrollbarDrag {
                                axis: Axis::Horizontal,
                                pointer_offset,
                            });
                        }

                        cx.stop_propagation();
                        cx.notify();
                    }
                });
            }
        });

        window.on_mouse_event({
            let editor = editor.clone();

            move |event: &MouseUpEvent, phase, _window, cx| {
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
                    }
                });
            }
        });

        window.on_mouse_event({
            let editor = editor.clone();
            let vertical_scrollbar_prepaint = vertical_scrollbar_prepaint.clone();
            let horizontal_scrollbar_prepaint = horizontal_scrollbar_prepaint.clone();

            move |event: &MouseMoveEvent, phase, _window, cx| {
                if phase != DispatchPhase::Bubble {
                    return;
                }

                editor.update(cx, |editor, cx| {
                    let Some(drag) = editor.scrollbar_drag else {
                        return;
                    };

                    let (track_bounds, thumb_bounds, scrollbar_scroll_max) = match drag.axis {
                        Axis::Vertical => {
                            let Some(scrollbar) = vertical_scrollbar_prepaint.as_ref() else {
                                return;
                            };
                            let Some(thumb_bounds) = scrollbar.thumb_bounds else {
                                return;
                            };
                            (scrollbar.track_bounds, thumb_bounds, scrollbar.scroll_max)
                        }
                        Axis::Horizontal => {
                            let Some(scrollbar) = horizontal_scrollbar_prepaint.as_ref() else {
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

                    let available = (track_end - track_start - thumb_len).max(gpui::px(0.0));
                    let desired_thumb_start =
                        (mouse_pos - drag.pointer_offset).clamp(track_start, track_end - thumb_len);
                    let fraction = if available == gpui::px(0.0) {
                        0.0
                    } else {
                        f64::from((desired_thumb_start - track_start) / available)
                    };
                    let snapshot = editor.display_snapshot(cx);
                    let current = editor.scroll_position(&snapshot);
                    let next = match drag.axis {
                        Axis::Vertical => gpui::point(current.x, fraction * scrollbar_scroll_max),
                        Axis::Horizontal => gpui::point(fraction * scrollbar_scroll_max, current.y),
                    };

                    editor.set_scroll_position(&snapshot, next, cx);
                    cx.stop_propagation();
                });
            }
        });

        if let Some(scrollbar) = layout.vertical_scrollbar.take() {
            window.paint_quad(scrollbar.track_quad);
            if let Some(thumb_quad) = scrollbar.thumb_quad {
                window.paint_quad(thumb_quad);
            }
        }
        if let Some(scrollbar) = layout.horizontal_scrollbar.take() {
            window.paint_quad(scrollbar.track_quad);
            if let Some(thumb_quad) = scrollbar.thumb_quad {
                window.paint_quad(thumb_quad);
            }
        }
    }
}

impl IntoElement for EditorElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for EditorElement {
    type RequestLayoutState = ();
    type PrepaintState = EditorLayout;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static panic::Location<'static>> {
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
            style.size.width = gpui::relative(1.0).into();
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
                    let line_count = editor.buffer_snapshot(cx).max_point().row as usize + 1;
                    let line_count = line_count.max(min_lines);
                    let line_count =
                        max_lines.map_or(line_count, |max_lines| line_count.min(max_lines));
                    style.size.height = (line_height
                        * line_count.to_f32().expect("line count should fit in f32"))
                    .into();
                }
                EditorMode::Full { .. } => {
                    style.size.height = gpui::relative(1.0).into();
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
                muted,
                mode,
                show_scrollbars,
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
                    editor.muted,
                    editor.mode.clone(),
                    editor.show_scrollbars,
                    editor.masked,
                    editor.scrollbar_drag,
                    editor.selected_range(cx),
                    editor.marked_range(cx),
                    editor.cursor_offset(cx),
                )
            };
            let placeholder_color = if muted {
                style.color.opacity(0.4)
            } else {
                cx.theme().colors().text_placeholder
            };
            let (snapshot, gutter_dimensions) = self.editor.update(cx, |editor, cx| {
                let snapshot = editor.snapshot(window, cx);
                let gutter_dimensions =
                    snapshot.gutter_dimensions(font_id, font_size, &self.style, window, cx);
                editor.gutter_dimensions = gutter_dimensions;
                (snapshot, gutter_dimensions)
            });
            let current_line_highlight = snapshot.current_line_highlight;
            let display_snapshot = snapshot.display_snapshot.clone();
            let height_in_lines = f64::from(bounds.size.height / line_height);
            let max_row = f64::from(display_snapshot.buffer_snapshot().max_point().row);
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
            let show_vertical_scrollbar = show_scrollbars.vertical;
            let scrollbar_width = if show_vertical_scrollbar {
                SCROLLBAR_THICKNESS
            } else {
                gpui::px(0.0)
            };

            let gutter_bounds = gutter_bounds(bounds, gutter_dimensions);
            let text_width = (bounds.size.width - gutter_dimensions.width).max(gpui::px(0.0));
            let mut text_bounds = Bounds {
                origin: gutter_bounds.top_right(),
                size: gpui::size(text_width, bounds.size.height),
            };
            let scrollbar_bounds = text_bounds;
            text_bounds.size.width = (text_bounds.size.width - scrollbar_width).max(gpui::px(0.0));
            let mut content_bounds = text_bounds;
            content_bounds.origin.x += gutter_dimensions.margin;
            content_bounds.size.width =
                (content_bounds.size.width - gutter_dimensions.margin).max(gpui::px(0.0));

            let viewport_width = (content_bounds.size.width - right_padding).max(gpui::px(0.0));

            let longest_row = display_snapshot.longest_row();
            let content_columns = f64::from(display_snapshot.line_len(longest_row));
            let viewport_columns = f64::from(viewport_width / column_width);
            let scrollable_columns = content_columns;
            let max_scroll_x = (scrollable_columns - viewport_columns).max(0.0);

            let scroll_max = gpui::point(max_scroll_x, max_scroll_y);
            let scroll_width = Pixels::from(f64::from(column_width) * scrollable_columns);
            let (autoscroll_request, needs_horizontal_autoscroll, mut scroll_position) =
                self.editor.update(cx, |editor, cx| {
                    let autoscroll_request = editor.take_autoscroll_request();
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

            let show_horizontal_scrollbar = show_scrollbars.horizontal && max_scroll_x > 0.0;
            let max_display_row = display_snapshot.buffer_snapshot().max_point().row;

            let cursor_point =
                display_snapshot
                    .buffer_snapshot()
                    .offset_to_point(MultiBufferOffset(
                        cursor_offset.min(display_snapshot.buffer_snapshot().len().0),
                    ));
            let cursor_row = cursor_point.row;
            let cursor_display_point =
                display_snapshot.point_to_display_point(cursor_point, text::Bias::Left);
            let cursor_display_row = cursor_display_point.row();
            let has_ime_marked_range = marked_range.is_some();
            let show_active_line_background = match mode {
                EditorMode::Full {
                    show_active_line_background,
                    ..
                } => {
                    show_active_line_background
                        && (selection_range.is_empty() || has_ime_marked_range)
                        && !matches!(current_line_highlight, CurrentLineHighlight::None)
                }
                _ => false,
            };
            let has_content = !display_snapshot.buffer_snapshot().is_empty();

            let clamped_scroll_position_y = scroll_position.y.clamp(0.0, scroll_max.y);
            let row_offset_y = clamped_scroll_position_y - clamped_scroll_position_y.floor();
            let scroll_y_pixels = Pixels::from(f64::from(line_height) * row_offset_y);

            let top_row = clamped_scroll_position_y
                .floor()
                .max(0.0)
                .to_u32()
                .expect("top row should fit in u32");
            let first_row_origin_y = bounds.top() - scroll_y_pixels;
            let viewport_height = bounds.bottom() - bounds.top();
            let visible_rows = (viewport_height / line_height).ceil() + 1.0;
            let visible_row_count = visible_rows
                .to_u32()
                .expect("visible row count should fit in u32");
            let mut line_scroll_x = scroll_position.x.clamp(0.0, max_scroll_x);
            let mut lines = build_visible_lines(
                &display_snapshot,
                content_bounds,
                line_height,
                &self.style,
                font_size,
                &placeholder,
                placeholder_color,
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
            );

            if needs_horizontal_autoscroll.0 && max_scroll_x > 0.0 {
                scroll_position = self.editor.update(cx, |editor, cx| {
                    editor.autoscroll_horizontally(
                        DisplayRow(top_row),
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
                        content_bounds,
                        line_height,
                        &self.style,
                        font_size,
                        &placeholder,
                        placeholder_color,
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
                    content_bounds,
                    line_height,
                    &self.style,
                    font_size,
                    &placeholder,
                    placeholder_color,
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
                );
            }

            if show_horizontal_scrollbar {
                text_bounds.size.height =
                    (text_bounds.size.height - SCROLLBAR_THICKNESS).max(gpui::px(0.0));
                content_bounds.size.height =
                    (content_bounds.size.height - SCROLLBAR_THICKNESS).max(gpui::px(0.0));
            }

            let mut selection_ranges = Vec::new();
            let mut active_rows: BTreeMap<DisplayRow, LineHighlightSpec> = BTreeMap::new();
            active_rows.entry(cursor_display_row).or_default().selection = true;
            if !selection_range.is_empty() {
                let buffer_snapshot = display_snapshot.buffer_snapshot();
                let max_offset = buffer_snapshot.len().0;
                let selection_start = buffer_snapshot.clip_offset(
                    MultiBufferOffset(selection_range.start.min(max_offset)),
                    text::Bias::Left,
                );
                let selection_end = buffer_snapshot.clip_offset(
                    MultiBufferOffset(selection_range.end.min(max_offset)),
                    text::Bias::Right,
                );
                let selection_start_point = buffer_snapshot.offset_to_point(selection_start);
                let selection_end_point = buffer_snapshot.offset_to_point(selection_end);
                let selection_start = display_snapshot
                    .point_to_display_point(selection_start_point, text::Bias::Left);
                let selection_end =
                    display_snapshot.point_to_display_point(selection_end_point, text::Bias::Right);
                if selection_start < selection_end {
                    let active_start = selection_start.row().0;
                    let active_end = if selection_end.column() == 0 {
                        selection_end.row().0.saturating_sub(1)
                    } else {
                        selection_end.row().0
                    };
                    for row in active_start..=active_end {
                        active_rows.entry(DisplayRow(row)).or_default().selection = true;
                    }
                    selection_ranges.push(selection_start..selection_end);
                }
            }

            let mut active_line_background = None;
            let mut cursor = None;
            for line in &lines {
                let line_text = line.line_text.as_str();
                let line_display_column_start = line.line_display_column_start;

                if show_active_line_background
                    && active_line_background.is_none()
                    && cursor_display_row == line.row
                {
                    let highlight_range = match current_line_highlight {
                        CurrentLineHighlight::Gutter => Some(bounds.left()..gutter_bounds.right()),
                        CurrentLineHighlight::Line => Some(text_bounds.left()..text_bounds.right()),
                        CurrentLineHighlight::All => Some(bounds.left()..bounds.right()),
                        CurrentLineHighlight::None => None,
                    };
                    if let Some(highlight_range) = highlight_range {
                        active_line_background = Some(gpui::fill(
                            Bounds::new(
                                gpui::point(highlight_range.start, line.origin.y),
                                gpui::size(
                                    highlight_range.end - highlight_range.start,
                                    line_height,
                                ),
                            ),
                            cx.theme().colors().editor_active_line_background,
                        ));
                    }
                }

                if cursor.is_none() && cursor_row == line.row.0 {
                    let cursor_column =
                        u32::try_from(cursor_offset.saturating_sub(line.line_start_offset))
                            .expect("cursor column should fit in u32");
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
                            let leading_columns = line_display_column_start - cursor_display_column;
                            let leading_width = Pixels::from(
                                f64::from(column_width)
                                    * leading_columns
                                        .to_f64()
                                        .expect("leading column count should fit in f64"),
                            );
                            line.origin.x - leading_width
                        } else if cursor_display_column <= line_display_column_end {
                            let local_column = cursor_display_column - line_display_column_start;
                            line.origin.x + line.x_for_index(local_column.min(line.len))
                        } else {
                            let trailing_columns = cursor_display_column - line_display_column_end;
                            let trailing_width = Pixels::from(
                                f64::from(column_width)
                                    * trailing_columns
                                        .to_f64()
                                        .expect("trailing column count should fit in f64"),
                            );
                            line.origin.x + line.width + trailing_width
                        }
                    };
                    cursor = Some(gpui::fill(
                        Bounds::new(
                            gpui::point(cursor_x, line.origin.y),
                            gpui::size(gpui::px(2.0), line_height),
                        ),
                        cx.theme().colors().editor_foreground,
                    ));
                }
            }

            let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
            let gutter_hitbox = window.insert_hitbox(gutter_bounds, HitboxBehavior::Normal);
            let text_hitbox = window.insert_hitbox(text_bounds, HitboxBehavior::Normal);
            let row_infos = display_snapshot
                .row_infos(DisplayRow(top_row))
                .take(lines.len())
                .collect::<Vec<_>>();
            let visible_row_count =
                u32::try_from(row_infos.len()).expect("visible row count should fit in u32");
            let gutter = Gutter {
                line_height,
                range: DisplayRow(top_row)..DisplayRow(top_row + visible_row_count),
                scroll_position: clamped_scroll_position,
                dimensions: &gutter_dimensions,
                hitbox: &gutter_hitbox,
                snapshot: &snapshot,
                row_infos: &row_infos,
            };
            let line_numbers = self.layout_line_numbers(
                &gutter,
                &active_rows,
                Some(cursor_display_row),
                window,
                cx,
            );
            window.set_focus_handle(&focus_handle, cx);

            let (vertical_scrollbar, horizontal_scrollbar) = Self::layout_scrollbars(
                scrollbar_bounds,
                show_vertical_scrollbar,
                show_horizontal_scrollbar,
                scrollbar_drag,
                clamped_scroll_position,
                max_scroll_x,
                max_scroll_y,
                viewport_columns,
                viewport_rows,
                window,
                cx,
            );

            let position_map = Rc::new(PositionMap {
                size: content_bounds.size,
                line_height,
                scroll_position: clamped_scroll_position,
                scroll_max,
                em_layout_width,
                visible_row_range: DisplayRow(top_row)..DisplayRow(top_row + visible_row_count),
                snapshot,
                text_align: TextAlign::Left,
                content_width: text_hitbox.size.width,
                text_hitbox,
                gutter_hitbox: gutter_hitbox.clone(),
                masked,
                line_layouts: lines,
            });

            self.editor.update(cx, |editor, _cx| {
                editor.last_position_map = Some(position_map.clone());
            });

            EditorLayout {
                position_map,
                hitbox,
                gutter_hitbox,
                line_numbers,
                active_line_background,
                cursor,
                selection_ranges,
                vertical_scrollbar,
                horizontal_scrollbar,
                em_width,
                column_width,
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
        layout: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let rem_size = self.rem_size(cx);
        window.with_rem_size(rem_size, |window| {
            let focus_handle = self.editor.read(cx).focus_handle.clone();
            let hitbox = layout.hitbox.clone();

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

            let line_height = layout.position_map.line_height;
            let clamped_scroll_position = layout.clamped_scroll_position;
            let text_bounds = layout.position_map.text_hitbox.bounds;

            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                self.paint_mouse_listeners(layout, window);

                if !self.style.background.is_transparent() {
                    window.paint_quad(gpui::fill(bounds, self.style.background));
                }
                window.paint_quad(gpui::fill(
                    layout.gutter_hitbox.bounds,
                    cx.theme().colors().editor_gutter_background,
                ));
                if let Some(active_line_background) = layout.active_line_background.take() {
                    window.paint_quad(active_line_background);
                }
                window.with_content_mask(
                    Some(ContentMask {
                        bounds: layout.gutter_hitbox.bounds,
                    }),
                    |window| {
                        Self::paint_line_numbers(layout, window, cx);
                    },
                );

                window.with_content_mask(
                    Some(ContentMask {
                        bounds: text_bounds,
                    }),
                    |window| {
                        Self::paint_highlights(layout, text_bounds, window, cx);

                        for line in &layout.position_map.line_layouts {
                            line.shaped_line
                                .paint(line.origin, line_height, TextAlign::Left, None, window, cx)
                                .log_err();
                        }

                        if focus_handle.is_focused(window)
                            && let Some(cursor) = layout.cursor.take()
                        {
                            window.paint_quad(cursor);
                        }
                    },
                );

                self.paint_scrollbars(layout, window, cx);
            });

            if layout.needs_scroll_clamp {
                self.editor.update(cx, |editor, cx| {
                    editor.set_scroll_position(
                        &layout.position_map.snapshot,
                        clamped_scroll_position,
                        cx,
                    );
                });
            }
        });
    }
}

fn gutter_bounds(
    editor_bounds: Bounds<Pixels>,
    gutter_dimensions: GutterDimensions,
) -> Bounds<Pixels> {
    Bounds {
        origin: editor_bounds.origin,
        size: gpui::size(gutter_dimensions.width, editor_bounds.size.height),
    }
}

fn build_visible_lines(
    display_snapshot: &DisplaySnapshot,
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    editor_style: &EditorStyle,
    font_size: Pixels,
    placeholder: &SharedString,
    placeholder_color: Hsla,
    masked: bool,
    marked_range: Option<&Range<usize>>,
    max_display_row: u32,
    top_row: u32,
    visible_row_count: u32,
    line_scroll_x: ScrollOffset,
    has_content: bool,
    first_row_origin_y: Pixels,
    em_layout_width: Pixels,
    window: &mut Window,
) -> Vec<LineWithInvisibles> {
    let style = &editor_style.text;
    let scroll_x_pixels = Pixels::from(f64::from(em_layout_width) * line_scroll_x);
    let mut lines = Vec::new();

    for visible_row_index in 0..visible_row_count {
        let row = top_row + visible_row_index;
        if row > max_display_row {
            break;
        }

        let row = DisplayRow(row);
        let line_start_offset = display_snapshot
            .buffer_snapshot()
            .point_to_offset(text::Point::new(row.0, 0));
        let line_len = display_snapshot
            .buffer_snapshot()
            .line_len(MultiBufferRow(row.0)) as usize;
        let line_end_offset = line_start_offset + line_len;
        let mut line_text = String::new();
        let mut runs = Vec::new();
        let line_display_column_start = if masked {
            let mut line_exceeded_max_len = false;
            for text_chunk in display_snapshot.text_chunks(row) {
                let (mut chunk, has_newline) = if let Some(index) = text_chunk.find('\n') {
                    (
                        text_chunk
                            .get(..index)
                            .expect("newline index should be valid"),
                        true,
                    )
                } else {
                    (text_chunk, false)
                };

                if !chunk.is_empty() && !line_exceeded_max_len {
                    if line_text.len() + chunk.len() > MAX_LINE_LEN {
                        let mut chunk_len = MAX_LINE_LEN - line_text.len();
                        while !chunk.is_char_boundary(chunk_len) {
                            chunk_len -= 1;
                        }
                        chunk = chunk
                            .get(..chunk_len)
                            .expect("chunk boundary should be valid");
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
            let requested_start_column = line_scroll_x
                .floor()
                .clamp(0.0, f64::from(u32::MAX))
                .to_u32()
                .expect("requested start column should fit in u32");
            let mut line_display_column_start = display_snapshot
                .clip_point(
                    DisplayPoint::new(row, requested_start_column),
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

            let target_end_column = line_display_column_start
                .saturating_add(MAX_LINE_LEN)
                .min(usize::try_from(u32::MAX).expect("u32 max should fit in usize"));
            let line_display_column_end = display_snapshot
                .clip_point(
                    DisplayPoint::new(
                        row,
                        u32::try_from(target_end_column).expect("display column should fit in u32"),
                    ),
                    text::Bias::Right,
                )
                .column() as usize;

            if line_display_column_start < line_display_column_end {
                let chunk_start = TabPoint::new(
                    row.0,
                    u32::try_from(line_display_column_start)
                        .expect("display column should fit in u32"),
                );
                let chunk_end = TabPoint::new(
                    row.0,
                    u32::try_from(line_display_column_end)
                        .expect("display column should fit in u32"),
                );
                for highlighted_chunk in display_snapshot.highlighted_chunks(
                    chunk_start..chunk_end,
                    LanguageAwareStyling {
                        tree_sitter: true,
                        diagnostics: false,
                    },
                    editor_style,
                ) {
                    let chunk_text = highlighted_chunk.text;
                    let (mut chunk_text, has_newline) = if let Some(index) = chunk_text.find('\n') {
                        (
                            chunk_text
                                .get(..index)
                                .expect("newline index should be valid"),
                            true,
                        )
                    } else {
                        (chunk_text, false)
                    };

                    if !chunk_text.is_empty() && line_text.len() < MAX_LINE_LEN {
                        let remaining_capacity = MAX_LINE_LEN - line_text.len();
                        let mut bounded_end = remaining_capacity.min(chunk_text.len());
                        while bounded_end > 0 && !chunk_text.is_char_boundary(bounded_end) {
                            bounded_end -= 1;
                        }
                        if bounded_end > 0 {
                            chunk_text = chunk_text
                                .get(..bounded_end)
                                .expect("chunk boundary should be valid");
                            line_text.push_str(chunk_text);
                            let text_style = if let Some(highlight_style) = highlighted_chunk.style
                            {
                                Cow::Owned(style.clone().highlight(highlight_style))
                            } else {
                                Cow::Borrowed(style)
                            };
                            runs.push(TextRun {
                                len: chunk_text.len(),
                                font: text_style.font(),
                                color: text_style.color,
                                background_color: text_style.background_color,
                                underline: text_style.underline,
                                strikethrough: text_style.strikethrough,
                            });
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
            (placeholder.clone(), placeholder_color)
        } else if masked {
            (mask_line(&line_text).into(), style.color)
        } else {
            (line_text.clone().into(), style.color)
        };
        let expanded_len = expanded.len();
        let mut base_style = style.clone();
        base_style.color = text_color;

        let line_x_offset = Pixels::from(
            f64::from(em_layout_width)
                * line_display_column_start
                    .to_f64()
                    .expect("line display column start should fit in f64"),
        );
        let line_y_offset = Pixels::from(
            f64::from(line_height)
                * visible_row_index
                    .to_f64()
                    .expect("visible row index should fit in f64"),
        );
        let origin = gpui::point(
            bounds.left() - scroll_x_pixels + line_x_offset,
            first_row_origin_y + line_y_offset,
        );

        if runs.is_empty() {
            runs.push(TextRun {
                len: expanded_len,
                font: base_style.font(),
                color: base_style.color,
                background_color: base_style.background_color,
                underline: base_style.underline,
                strikethrough: base_style.strikethrough,
            });
        }

        if let Some(marked_range) = marked_range {
            let marked_start = marked_range.start.max(line_start_offset.0);
            let marked_end = marked_range.end.min(line_end_offset.0);
            if marked_start < marked_end {
                let start_column = u32::try_from(marked_start - line_start_offset.0)
                    .expect("marked range start column should fit in u32");
                let end_column = u32::try_from(marked_end - line_start_offset.0)
                    .expect("marked range end column should fit in u32");
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

                if display_end > display_start {
                    let range = display_start..display_end;
                    let mut offset = 0;
                    let mut underlined_runs = Vec::with_capacity(runs.len() + 2);
                    for run in runs {
                        let run_start = offset;
                        let run_end = offset + run.len;
                        offset = run_end;

                        if run_end <= range.start || run_start >= range.end {
                            underlined_runs.push(run);
                            continue;
                        }

                        if run_start < range.start {
                            underlined_runs.push(TextRun {
                                len: range.start - run_start,
                                ..run.clone()
                            });
                        }

                        let underline_start = cmp::max(run_start, range.start);
                        let underline_end = cmp::min(run_end, range.end);
                        if underline_start < underline_end {
                            underlined_runs.push(TextRun {
                                len: underline_end - underline_start,
                                underline: Some(UnderlineStyle {
                                    color: Some(run.color),
                                    thickness: gpui::px(1.0),
                                    wavy: false,
                                }),
                                ..run.clone()
                            });
                        }

                        if underline_end < run_end {
                            underlined_runs.push(TextRun {
                                len: run_end - underline_end,
                                ..run
                            });
                        }
                    }
                    runs = underlined_runs;
                }
            }
        }

        let shaped_line = window
            .text_system()
            .shape_line(expanded, font_size, &runs, None);
        let width = shaped_line.x_for_index(expanded_len);
        let line_text = if masked { line_text } else { String::new() };

        lines.push(LineWithInvisibles {
            row,
            origin,
            line_start_offset: line_start_offset.0,
            line_display_column_start,
            len: expanded_len,
            width,
            line_text,
            shaped_line,
        });
    }

    lines
}

#[derive(Debug)]
pub(crate) struct HighlightedRangeLine {
    pub start_x: Pixels,
    pub end_x: Pixels,
}

#[derive(Debug)]
pub(crate) struct HighlightedRange {
    pub start_y: Pixels,
    pub line_height: Pixels,
    pub lines: Vec<HighlightedRangeLine>,
    pub color: Hsla,
    pub corner_radius: Pixels,
}

impl HighlightedRange {
    pub(crate) fn paint(&self, fill: bool, bounds: Bounds<Pixels>, window: &mut Window) {
        if let Some((first_line, remaining_lines)) = self.lines.split_first()
            && let Some(second_line) = remaining_lines.first()
            && first_line.start_x > second_line.end_x
        {
            self.paint_lines(
                self.start_y,
                std::slice::from_ref(first_line),
                fill,
                bounds,
                window,
            );
            self.paint_lines(
                self.start_y + self.line_height,
                remaining_lines,
                fill,
                bounds,
                window,
            );
        } else {
            self.paint_lines(self.start_y, &self.lines, fill, bounds, window);
        }
    }

    fn paint_lines(
        &self,
        start_y: Pixels,
        lines: &[HighlightedRangeLine],
        fill: bool,
        _bounds: Bounds<Pixels>,
        window: &mut Window,
    ) {
        let Some(first_line) = lines.first() else {
            return;
        };
        let Some(last_line) = lines.last() else {
            return;
        };

        let first_top_left = gpui::point(first_line.start_x, start_y);
        let first_top_right = gpui::point(first_line.end_x, start_y);

        let curve_height = gpui::point(Pixels::ZERO, self.corner_radius);
        let curve_width = |start_x: Pixels, end_x: Pixels| {
            let max = (end_x - start_x) / 2.0;
            let width = if max < self.corner_radius {
                max
            } else {
                self.corner_radius
            };

            gpui::point(width, Pixels::ZERO)
        };

        let top_curve_width = curve_width(first_line.start_x, first_line.end_x);
        let mut builder = if fill {
            PathBuilder::fill()
        } else {
            PathBuilder::stroke(gpui::px(1.0))
        };
        builder.move_to(first_top_right - top_curve_width);
        builder.curve_to(first_top_right + curve_height, first_top_right);

        let mut iter = lines.iter().enumerate().peekable();
        while let Some((index, line)) = iter.next() {
            let line_count = index + 1;
            let line_height_offset = Pixels::from(
                line_count.to_f64().expect("line count should fit in f64")
                    * f64::from(self.line_height),
            );
            let bottom_right = gpui::point(line.end_x, start_y + line_height_offset);

            if let Some((_, next_line)) = iter.peek() {
                let next_top_right = gpui::point(next_line.end_x, bottom_right.y);

                match next_top_right
                    .x
                    .partial_cmp(&bottom_right.x)
                    .unwrap_or(Ordering::Equal)
                {
                    Ordering::Equal => {
                        builder.line_to(bottom_right);
                    }
                    Ordering::Less => {
                        let curve_width = curve_width(next_top_right.x, bottom_right.x);
                        builder.line_to(bottom_right - curve_height);
                        if self.corner_radius > Pixels::ZERO {
                            builder.curve_to(bottom_right - curve_width, bottom_right);
                        }
                        builder.line_to(next_top_right + curve_width);
                        if self.corner_radius > Pixels::ZERO {
                            builder.curve_to(next_top_right + curve_height, next_top_right);
                        }
                    }
                    Ordering::Greater => {
                        let curve_width = curve_width(bottom_right.x, next_top_right.x);
                        builder.line_to(bottom_right - curve_height);
                        if self.corner_radius > Pixels::ZERO {
                            builder.curve_to(bottom_right + curve_width, bottom_right);
                        }
                        builder.line_to(next_top_right - curve_width);
                        if self.corner_radius > Pixels::ZERO {
                            builder.curve_to(next_top_right + curve_height, next_top_right);
                        }
                    }
                }
            } else {
                let curve_width = curve_width(line.start_x, line.end_x);
                builder.line_to(bottom_right - curve_height);
                if self.corner_radius > Pixels::ZERO {
                    builder.curve_to(bottom_right - curve_width, bottom_right);
                }

                let bottom_left = gpui::point(line.start_x, bottom_right.y);
                builder.line_to(bottom_left + curve_width);
                if self.corner_radius > Pixels::ZERO {
                    builder.curve_to(bottom_left - curve_height, bottom_left);
                }
            }
        }

        if first_line.start_x > last_line.start_x {
            let curve_width = curve_width(last_line.start_x, first_line.start_x);
            let second_top_left = gpui::point(last_line.start_x, start_y + self.line_height);
            builder.line_to(second_top_left + curve_height);
            if self.corner_radius > Pixels::ZERO {
                builder.curve_to(second_top_left + curve_width, second_top_left);
            }
            let first_bottom_left = gpui::point(first_line.start_x, second_top_left.y);
            builder.line_to(first_bottom_left - curve_width);
            if self.corner_radius > Pixels::ZERO {
                builder.curve_to(first_bottom_left - curve_height, first_bottom_left);
            }
        }

        builder.line_to(first_top_left + curve_height);
        if self.corner_radius > Pixels::ZERO {
            builder.curve_to(first_top_left + top_curve_width, first_top_left);
        }
        builder.line_to(first_top_right - top_curve_width);

        if let Ok(path) = builder.build() {
            window.paint_path(path, self.color);
        }
    }
}

fn scale_vertical_mouse_autoscroll_delta(delta: Pixels) -> f32 {
    (delta.pow(1.2) / 100.0).min(gpui::px(3.0)).into()
}

fn scale_horizontal_mouse_autoscroll_delta(delta: Pixels) -> f32 {
    (delta.pow(1.2) / 300.0).into()
}

fn register_action<T: Action>(
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
    });
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
    if width == gpui::px(0.0) {
        gpui::px(8.0)
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
    scroll_position: ScrollOffset,
    scroll_max: ScrollOffset,
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
                    gpui::px(0.0)
                })
            .max(gpui::px(0.0));
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
            top: gpui::px(0.0),
            right: gpui::px(0.0),
            bottom: gpui::px(0.0),
            left: gpui::px(0.0),
        },
        Axis::Vertical => Edges {
            top: gpui::px(0.0),
            right: gpui::px(0.0),
            bottom: gpui::px(0.0),
            left: gpui::px(1.0),
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
        let thumb_len = Pixels::from(f64::from(track_length) * ratio)
            .max(SCROLLBAR_MIN_THUMB_LEN)
            .min(track_length);
        let available = (track_length - thumb_len).max(gpui::px(0.0));
        let scroll_ratio = (scroll_position / scroll_max).clamp(0.0, 1.0);
        let thumb_start = Pixels::from(f64::from(available) * scroll_ratio);

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
