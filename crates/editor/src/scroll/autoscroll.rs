use gpui::{Bounds, Context, Pixels, Point, Window};
use num_traits::ToPrimitive;
use std::cmp;

use multi_buffer::MultiBufferOffset;

use crate::{
    Editor, EditorMode,
    display_map::{DisplayPoint, DisplayRow},
    element::LineWithInvisibles,
    scroll::{ScrollOffset, WasScrolled},
};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum Autoscroll {
    Fit,
    Newest,
}

impl Autoscroll {
    pub(crate) fn fit() -> Self {
        Self::Fit
    }

    pub(crate) fn newest() -> Self {
        Self::Newest
    }
}

pub(crate) struct NeedsHorizontalAutoscroll(pub(crate) bool);

impl Editor {
    pub(crate) fn request_autoscroll(&mut self, autoscroll: Autoscroll, cx: &mut Context<Self>) {
        self.scroll_manager.autoscroll_request = Some((autoscroll, true));
        cx.notify();
    }

    pub(crate) fn autoscroll_vertically(
        &mut self,
        bounds: Bounds<Pixels>,
        line_height: Pixels,
        max_scroll_top: ScrollOffset,
        autoscroll_request: Option<(Autoscroll, bool)>,
        _window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> (NeedsHorizontalAutoscroll, WasScrolled) {
        let viewport_height = bounds.size.height;
        let visible_lines = ScrollOffset::from(viewport_height / line_height);
        let display_snapshot = self.display_snapshot(cx);

        let mut scroll_position = self.scroll_position(&display_snapshot);
        let original_y = scroll_position.y;

        if scroll_position.y > max_scroll_top {
            scroll_position.y = max_scroll_top;
        }

        let editor_was_scrolled = if matches!(
            original_y.partial_cmp(&scroll_position.y),
            Some(cmp::Ordering::Equal)
        ) {
            WasScrolled(false)
        } else {
            self.set_scroll_position(&display_snapshot, scroll_position, cx);
            WasScrolled(true)
        };

        let Some((autoscroll, local)) = autoscroll_request else {
            return (NeedsHorizontalAutoscroll(false), editor_was_scrolled);
        };

        let snapshot = display_snapshot.buffer_snapshot();
        let cursor_offset = self.cursor_offset(cx).min(snapshot.len().0);
        let cursor_row = snapshot
            .offset_to_point(MultiBufferOffset(cursor_offset))
            .row;
        let target_top = f64::from(cursor_row);
        let target_bottom = target_top + 1.0;

        let margin = if matches!(self.mode, EditorMode::AutoHeight { .. }) {
            0.0
        } else {
            ((visible_lines - (target_bottom - target_top)) / 2.0)
                .floor()
                .max(0.0)
        };

        let was_autoscrolled = match autoscroll {
            Autoscroll::Fit | Autoscroll::Newest => {
                let margin = margin.min(self.scroll_manager.vertical_scroll_margin);
                let target_top = (target_top - margin).max(0.0);
                let target_bottom = target_bottom + margin;

                let start_row = scroll_position.y;
                let end_row = start_row + visible_lines;

                let needs_scroll_up = target_top < start_row;
                let needs_scroll_down = target_bottom >= end_row;

                if needs_scroll_up && !needs_scroll_down {
                    scroll_position.y = target_top;
                } else if !needs_scroll_up && needs_scroll_down {
                    scroll_position.y = target_bottom - visible_lines;
                }

                if needs_scroll_up ^ needs_scroll_down {
                    scroll_position.y = scroll_position.y.clamp(0.0, max_scroll_top);
                    self.scroll_manager
                        .set_scroll_position(&display_snapshot, scroll_position);
                    if local {
                        cx.notify();
                    }
                    WasScrolled(true)
                } else {
                    WasScrolled(false)
                }
            }
        };

        let was_scrolled = WasScrolled(editor_was_scrolled.0 || was_autoscrolled.0);
        (NeedsHorizontalAutoscroll(true), was_scrolled)
    }

    pub(crate) fn autoscroll_horizontally(
        &mut self,
        start_row: DisplayRow,
        viewport_width: Pixels,
        scroll_width: Pixels,
        em_advance: Pixels,
        line_layouts: &[LineWithInvisibles],
        autoscroll_request: Option<(Autoscroll, bool)>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Point<ScrollOffset>> {
        let (_, local) = autoscroll_request?;

        if em_advance == gpui::px(0.0) {
            return None;
        }

        let em_advance = ScrollOffset::from(em_advance);
        let viewport_width = ScrollOffset::from(viewport_width);
        let scroll_width = ScrollOffset::from(scroll_width);

        let display_snapshot = self.display_snapshot(cx);
        let snapshot = display_snapshot.buffer_snapshot();
        let mut scroll_position = self.scroll_position(&display_snapshot);

        let mut target_left = f64::INFINITY;
        let mut target_right: f64 = 0.0;

        let cursor_offset = self.cursor_offset(cx).min(snapshot.len().0);
        let cursor_point = snapshot.offset_to_point(MultiBufferOffset(cursor_offset));
        let head = display_snapshot.point_to_display_point(cursor_point, text::Bias::Left);
        let visible_row_count =
            u32::try_from(line_layouts.len()).expect("visible row count should fit in u32");
        if head.row() >= start_row && head.row() < DisplayRow(start_row.0 + visible_row_count) {
            let row_index = head.row().0.saturating_sub(start_row.0) as usize;
            let layout = line_layouts.get(row_index)?;
            let start_column = head.column();
            let end_column = cmp::min(display_snapshot.line_len(head.row()), head.column());
            let line_display_column_start = display_snapshot
                .clip_point(
                    DisplayPoint::new(
                        head.row(),
                        self.scroll_manager
                            .offset()
                            .x
                            .floor()
                            .clamp(0.0, f64::from(u32::MAX))
                            .to_u32()
                            .expect("scroll column should fit in u32"),
                    ),
                    text::Bias::Left,
                )
                .column() as usize;

            let prefix_width = em_advance * line_display_column_start.to_f64().unwrap();
            let line_display_column_end = line_display_column_start.saturating_add(layout.len);

            let column_x = |display_column: usize| -> ScrollOffset {
                if display_column < line_display_column_start {
                    em_advance * display_column.to_f64().unwrap()
                } else if display_column <= line_display_column_end {
                    let local_column = display_column - line_display_column_start;
                    prefix_width + ScrollOffset::from(layout.x_for_index(local_column))
                } else {
                    let tail_columns = display_column - line_display_column_end;
                    prefix_width
                        + ScrollOffset::from(layout.x_for_index(layout.len))
                        + em_advance * tail_columns.to_f64().unwrap()
                }
            };

            target_left = target_left.min(column_x(start_column as usize));
            target_right = target_right.max(column_x(end_column as usize) + em_advance);
        } else {
            target_left = 0.0;
            target_right = 0.0;
        }

        let scroll_left = self.scroll_manager.offset().x * em_advance;
        let scroll_right = scroll_left + viewport_width;
        target_right = target_right.min(scroll_width);

        let was_scrolled = if target_right - target_left > viewport_width {
            WasScrolled(false)
        } else if target_left < scroll_left {
            scroll_position.x = target_left / em_advance;
            self.scroll_manager
                .set_scroll_position(&display_snapshot, scroll_position);
            if local {
                cx.notify();
            }
            WasScrolled(true)
        } else if target_right > scroll_right {
            scroll_position.x = (target_right - viewport_width) / em_advance;
            self.scroll_manager
                .set_scroll_position(&display_snapshot, scroll_position);
            if local {
                cx.notify();
            }
            WasScrolled(true)
        } else {
            WasScrolled(false)
        };

        if was_scrolled.0 {
            Some(scroll_position)
        } else {
            None
        }
    }
}
