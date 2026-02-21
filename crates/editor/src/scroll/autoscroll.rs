use gpui::{Bounds, Context, Pixels, Window};
use multi_buffer::{Anchor, MultiBufferOffset};
use std::cmp;

use crate::{
    Editor, EditorMode,
    display_map::ToDisplayPoint,
    element::LineWithInvisibles,
    scroll::{ScrollOffset, WasScrolled},
};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Autoscroll {
    Next,
    Strategy(AutoscrollStrategy, Option<Anchor>),
}

impl Autoscroll {
    pub fn fit() -> Self {
        Self::Strategy(AutoscrollStrategy::Fit, None)
    }

    pub fn newest() -> Self {
        Self::Strategy(AutoscrollStrategy::Newest, None)
    }

    pub fn center() -> Self {
        Self::Strategy(AutoscrollStrategy::Center, None)
    }

    pub fn focused() -> Self {
        Self::Strategy(AutoscrollStrategy::Focused, None)
    }

    pub fn top_relative(n: usize) -> Self {
        Self::Strategy(AutoscrollStrategy::TopRelative(n), None)
    }

    pub fn top() -> Self {
        Self::Strategy(AutoscrollStrategy::Top, None)
    }

    pub fn bottom_relative(n: usize) -> Self {
        Self::Strategy(AutoscrollStrategy::BottomRelative(n), None)
    }

    pub fn bottom() -> Self {
        Self::Strategy(AutoscrollStrategy::Bottom, None)
    }

    pub fn for_anchor(self, anchor: Anchor) -> Self {
        match self {
            Autoscroll::Next => self,
            Autoscroll::Strategy(strategy, _) => Autoscroll::Strategy(strategy, Some(anchor)),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Default, Clone, Copy)]
pub enum AutoscrollStrategy {
    Fit,
    Newest,
    #[default]
    Center,
    Focused,
    Top,
    Bottom,
    TopRelative(usize),
    BottomRelative(usize),
}

pub(crate) struct NeedsHorizontalAutoscroll(pub(crate) bool);

impl Editor {
    pub fn request_autoscroll(&mut self, autoscroll: Autoscroll, cx: &mut Context<Self>) {
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

        let mut scroll_position = self.scroll_manager.scroll_position(&display_snapshot);
        let original_y = scroll_position.y;

        if scroll_position.y > max_scroll_top {
            scroll_position.y = max_scroll_top;
        }

        let editor_was_scrolled = if original_y != scroll_position.y {
            self.scroll_manager
                .set_scroll_position(&display_snapshot, scroll_position);
            cx.notify();
            WasScrolled(true)
        } else {
            WasScrolled(false)
        };

        let Some((autoscroll, local)) = autoscroll_request else {
            return (NeedsHorizontalAutoscroll(false), editor_was_scrolled);
        };

        let target_top;
        let target_bottom;
        if let Autoscroll::Strategy(_, Some(anchor)) = autoscroll {
            target_top = anchor.to_display_point(&display_snapshot).row().0 as f64;
            target_bottom = target_top + 1.0;
        } else {
            let snapshot = display_snapshot.buffer_snapshot();
            let cursor_offset = self.cursor_offset().min(snapshot.len().0);
            let cursor_row = snapshot
                .offset_to_point(MultiBufferOffset(cursor_offset))
                .row;
            target_top = cursor_row as f64;
            target_bottom = target_top + 1.0;
        }

        let margin = if matches!(self.mode, EditorMode::AutoHeight { .. }) {
            0.0
        } else {
            ((visible_lines - (target_bottom - target_top)) / 2.0)
                .floor()
                .max(0.0)
        };

        let strategy = match autoscroll {
            Autoscroll::Strategy(strategy, _) => strategy,
            Autoscroll::Next => AutoscrollStrategy::default(),
        };

        let was_autoscrolled = match strategy {
            AutoscrollStrategy::Fit | AutoscrollStrategy::Newest => {
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
            AutoscrollStrategy::Center => {
                scroll_position.y = (target_top - margin).max(0.0).min(max_scroll_top);
                self.scroll_manager
                    .set_scroll_position(&display_snapshot, scroll_position);
                if local {
                    cx.notify();
                }
                WasScrolled(true)
            }
            AutoscrollStrategy::Focused => {
                let margin = margin.min(self.scroll_manager.vertical_scroll_margin);
                scroll_position.y = (target_top - margin).max(0.0).min(max_scroll_top);
                self.scroll_manager
                    .set_scroll_position(&display_snapshot, scroll_position);
                if local {
                    cx.notify();
                }
                WasScrolled(true)
            }
            AutoscrollStrategy::Top => {
                scroll_position.y = target_top.max(0.0).min(max_scroll_top);
                self.scroll_manager
                    .set_scroll_position(&display_snapshot, scroll_position);
                if local {
                    cx.notify();
                }
                WasScrolled(true)
            }
            AutoscrollStrategy::Bottom => {
                scroll_position.y = (target_bottom - visible_lines).max(0.0).min(max_scroll_top);
                self.scroll_manager
                    .set_scroll_position(&display_snapshot, scroll_position);
                if local {
                    cx.notify();
                }
                WasScrolled(true)
            }
            AutoscrollStrategy::TopRelative(lines) => {
                scroll_position.y = (target_top - lines as ScrollOffset)
                    .max(0.0)
                    .min(max_scroll_top);
                self.scroll_manager
                    .set_scroll_position(&display_snapshot, scroll_position);
                if local {
                    cx.notify();
                }
                WasScrolled(true)
            }
            AutoscrollStrategy::BottomRelative(lines) => {
                scroll_position.y = (target_bottom + lines as ScrollOffset)
                    .max(0.0)
                    .min(max_scroll_top);
                self.scroll_manager
                    .set_scroll_position(&display_snapshot, scroll_position);
                if local {
                    cx.notify();
                }
                WasScrolled(true)
            }
        };

        let was_scrolled = WasScrolled(editor_was_scrolled.0 || was_autoscrolled.0);
        (NeedsHorizontalAutoscroll(true), was_scrolled)
    }

    pub(crate) fn autoscroll_horizontally(
        &mut self,
        start_row: crate::display_map::DisplayRow,
        viewport_width: Pixels,
        scroll_width: Pixels,
        em_advance: Pixels,
        line_layouts: &[LineWithInvisibles],
        autoscroll_request: Option<(Autoscroll, bool)>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Point<ScrollOffset>> {
        let (_, local) = autoscroll_request?;

        if em_advance == gpui::px(0.) {
            return None;
        }

        let em_advance = ScrollOffset::from(em_advance);
        let viewport_width = ScrollOffset::from(viewport_width);
        let scroll_width = ScrollOffset::from(scroll_width);

        let display_snapshot = self.display_snapshot(cx);
        let snapshot = display_snapshot.buffer_snapshot();
        let mut scroll_position = self.scroll_manager.scroll_position(&display_snapshot);

        let mut target_left = f64::INFINITY;
        let mut target_right: f64 = 0.0;

        let cursor_offset = self.cursor_offset().min(snapshot.len().0);
        let cursor_point = snapshot.offset_to_point(MultiBufferOffset(cursor_offset));
        let head = display_snapshot.point_to_display_point(cursor_point, text::Bias::Left);
        if head.row() >= start_row
            && head.row() < crate::display_map::DisplayRow(start_row.0 + line_layouts.len() as u32)
        {
            let row_index = head.row().0.saturating_sub(start_row.0) as usize;
            let layout = line_layouts.get(row_index)?;
            let start_column = head.column();
            let end_column = cmp::min(display_snapshot.line_len(head.row()), head.column());
            let line_display_column_start = display_snapshot
                .clip_point(
                    crate::display_map::DisplayPoint::new(
                        head.row(),
                        self.scroll_manager.offset().x.floor().max(0.0) as u32,
                    ),
                    text::Bias::Left,
                )
                .column() as usize;

            let prefix_width = em_advance * line_display_column_start as ScrollOffset;
            let line_display_column_end = line_display_column_start.saturating_add(layout.len);

            let column_x = |display_column: usize| -> ScrollOffset {
                if display_column < line_display_column_start {
                    em_advance * display_column as ScrollOffset
                } else if display_column <= line_display_column_end {
                    let local_column = display_column - line_display_column_start;
                    prefix_width + ScrollOffset::from(layout.x_for_index(local_column))
                } else {
                    let tail_columns = display_column - line_display_column_end;
                    prefix_width
                        + ScrollOffset::from(layout.x_for_index(layout.len))
                        + em_advance * tail_columns as ScrollOffset
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
