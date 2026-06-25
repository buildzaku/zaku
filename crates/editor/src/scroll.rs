pub(crate) mod autoscroll;

pub(crate) use autoscroll::Autoscroll;

use gpui::{Axis, Context, Pixels, Point};
use num_traits::ToPrimitive;
use std::time::{Duration, Instant};

use multi_buffer::Anchor;

use crate::{
    Editor,
    display_map::{DisplayPoint, DisplayRow, DisplaySnapshot, ToDisplayPoint},
};

pub(crate) type ScrollOffset = f64;

const SCROLL_EVENT_SEPARATION: Duration = Duration::from_millis(28);

pub(crate) struct WasScrolled(pub(crate) bool);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ScrollAnchor {
    pub offset: Point<ScrollOffset>,
    pub anchor: Anchor,
}

impl ScrollAnchor {
    fn new() -> Self {
        Self {
            offset: Point::default(),
            anchor: Anchor::Min,
        }
    }

    fn scroll_position(&self, snapshot: &DisplaySnapshot) -> Point<ScrollOffset> {
        let mut position = self.offset;

        let scroll_top_row = if self.anchor == Anchor::Min {
            0.0
        } else {
            f64::from(self.anchor.to_display_point(snapshot).row().0)
        };
        position.y = (position.y + scroll_top_row).max(0.0);

        position
    }
}

#[derive(Clone, Copy, Debug)]
struct OngoingScroll {
    last_event: Instant,
    axis: Option<Axis>,
}

impl OngoingScroll {
    fn new() -> Self {
        Self {
            last_event: Instant::now()
                .checked_sub(SCROLL_EVENT_SEPARATION)
                .expect("current time should allow scroll event separation"),
            axis: None,
        }
    }

    fn filter(&self, delta: &mut Point<Pixels>) -> Option<Axis> {
        const UNLOCK_PERCENT: f32 = 1.9;
        const UNLOCK_LOWER_BOUND: Pixels = gpui::px(6.0);
        let mut axis = self.axis;

        let x_delta = delta.x.abs();
        let y_delta = delta.y.abs();
        let duration = Instant::now().duration_since(self.last_event);
        if duration > SCROLL_EVENT_SEPARATION {
            axis = if x_delta <= y_delta {
                Some(Axis::Vertical)
            } else {
                Some(Axis::Horizontal)
            };
        } else if x_delta.max(y_delta) >= UNLOCK_LOWER_BOUND {
            match axis {
                Some(Axis::Vertical)
                    if x_delta > y_delta && x_delta >= y_delta * UNLOCK_PERCENT =>
                {
                    axis = None;
                }
                Some(Axis::Horizontal)
                    if y_delta > x_delta && y_delta >= x_delta * UNLOCK_PERCENT =>
                {
                    axis = None;
                }
                Some(Axis::Vertical | Axis::Horizontal) | None => {}
            }
        }

        match axis {
            Some(Axis::Vertical) => {
                *delta = gpui::point(gpui::px(0.0), delta.y);
            }
            Some(Axis::Horizontal) => {
                *delta = gpui::point(delta.x, gpui::px(0.0));
            }
            None => {}
        }

        axis
    }
}

pub(crate) struct ScrollManager {
    pub(crate) vertical_scroll_margin: ScrollOffset,
    scroll_anchor: ScrollAnchor,
    ongoing_scroll: OngoingScroll,
    autoscroll_request: Option<(Autoscroll, bool)>,
}

impl ScrollManager {
    pub(crate) fn new() -> Self {
        Self {
            vertical_scroll_margin: 3.0,
            scroll_anchor: ScrollAnchor::new(),
            ongoing_scroll: OngoingScroll::new(),
            autoscroll_request: None,
        }
    }

    fn ongoing_scroll(&self) -> OngoingScroll {
        self.ongoing_scroll
    }

    fn update_ongoing_scroll(&mut self, axis: Option<Axis>) {
        self.ongoing_scroll.last_event = Instant::now();
        self.ongoing_scroll.axis = axis;
    }

    fn offset(&self) -> Point<ScrollOffset> {
        self.scroll_anchor.offset
    }

    fn scroll_position(&self, snapshot: &DisplaySnapshot) -> Point<ScrollOffset> {
        self.scroll_anchor.scroll_position(snapshot)
    }

    fn take_autoscroll_request(&mut self) -> Option<(Autoscroll, bool)> {
        self.autoscroll_request.take()
    }

    fn set_scroll_position(&mut self, snapshot: &DisplaySnapshot, position: Point<ScrollOffset>) {
        let max_row = snapshot.buffer_snapshot().max_point().row;
        let scroll_top = position.y.max(0.0);
        let row = DisplayRow(
            scroll_top
                .floor()
                .clamp(0.0, f64::from(max_row))
                .to_u32()
                .expect("scroll row should fit in u32"),
        );
        let display_point = snapshot.clip_point(
            DisplayPoint::new(
                row,
                position
                    .x
                    .clamp(0.0, f64::from(u32::MAX))
                    .to_u32()
                    .expect("scroll column should fit in u32"),
            ),
            text::Bias::Left,
        );
        let anchor = snapshot.display_point_to_anchor(display_point, text::Bias::Left);
        let anchor_row = f64::from(anchor.to_display_point(snapshot).row().0);
        let offset_y = scroll_top - anchor_row;

        self.autoscroll_request.take();
        self.scroll_anchor = ScrollAnchor {
            offset: Point {
                x: position.x.max(0.0),
                y: offset_y,
            },
            anchor,
        };
    }
}

impl Editor {
    pub(crate) fn scroll_position(&self, snapshot: &DisplaySnapshot) -> Point<ScrollOffset> {
        self.scroll_manager.scroll_position(snapshot)
    }

    pub(crate) fn set_scroll_position(
        &mut self,
        snapshot: &DisplaySnapshot,
        position: Point<ScrollOffset>,
        cx: &mut Context<Self>,
    ) {
        self.scroll_manager.set_scroll_position(snapshot, position);
        cx.notify();
    }

    pub(crate) fn filter_ongoing_scroll(&mut self, delta: &mut Point<Pixels>) {
        let axis = self.scroll_manager.ongoing_scroll().filter(delta);
        self.scroll_manager.update_ongoing_scroll(axis);
    }

    pub(crate) fn clear_ongoing_scroll(&mut self) {
        self.scroll_manager.update_ongoing_scroll(None);
    }

    pub(crate) fn take_autoscroll_request(&mut self) -> Option<(Autoscroll, bool)> {
        self.scroll_manager.take_autoscroll_request()
    }

    pub fn vertical_scroll_margin(&self) -> usize {
        self.scroll_manager
            .vertical_scroll_margin
            .max(0.0)
            .to_usize()
            .expect("vertical scroll margin should fit in usize")
    }

    pub fn set_vertical_scroll_margin(&mut self, margin_rows: usize, cx: &mut Context<Self>) {
        self.scroll_manager.vertical_scroll_margin = margin_rows
            .to_f64()
            .expect("vertical scroll margin should fit in f64");
        cx.notify();
    }
}
