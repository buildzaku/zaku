pub(crate) mod autoscroll;

use gpui::{Axis, Context, Pixels, Point};
use multi_buffer::Anchor;
use std::time::{Duration, Instant};

use crate::{
    Editor,
    display_map::{DisplayPoint, DisplayRow, DisplaySnapshot, ToDisplayPoint},
};

pub use autoscroll::Autoscroll;

pub type ScrollOffset = f64;

pub const SCROLL_EVENT_SEPARATION: Duration = Duration::from_millis(28);

pub struct WasScrolled(pub(crate) bool);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollAnchor {
    pub offset: Point<ScrollOffset>,
    pub anchor: Anchor,
}

impl ScrollAnchor {
    pub fn new() -> Self {
        Self {
            offset: Point::default(),
            anchor: Anchor::min(),
        }
    }

    fn scroll_position(&self, snapshot: &DisplaySnapshot) -> Point<ScrollOffset> {
        let mut position = self.offset;

        let scroll_top_row = if self.anchor == Anchor::min() {
            0.0
        } else {
            self.anchor.to_display_point(snapshot).row().0 as f64
        };
        position.y = (position.y + scroll_top_row).max(0.0);

        position
    }
}

#[derive(Clone, Copy, Debug)]
pub struct OngoingScroll {
    last_event: Instant,
    axis: Option<Axis>,
}

impl OngoingScroll {
    fn new() -> Self {
        Self {
            last_event: Instant::now() - SCROLL_EVENT_SEPARATION,
            axis: None,
        }
    }

    fn filter(&self, delta: &mut Point<Pixels>) -> Option<Axis> {
        const UNLOCK_PERCENT: f32 = 1.9;
        const UNLOCK_LOWER_BOUND: Pixels = gpui::px(6.0);
        let mut axis = self.axis;

        let x = delta.x.abs();
        let y = delta.y.abs();
        let duration = Instant::now().duration_since(self.last_event);
        if duration > SCROLL_EVENT_SEPARATION {
            axis = if x <= y {
                Some(Axis::Vertical)
            } else {
                Some(Axis::Horizontal)
            };
        } else if x.max(y) >= UNLOCK_LOWER_BOUND {
            match axis {
                Some(Axis::Vertical) => {
                    if x > y && x >= y * UNLOCK_PERCENT {
                        axis = None;
                    }
                }
                Some(Axis::Horizontal) => {
                    if y > x && y >= x * UNLOCK_PERCENT {
                        axis = None;
                    }
                }
                None => {}
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

pub struct ScrollManager {
    pub(crate) vertical_scroll_margin: ScrollOffset,
    scroll_anchor: ScrollAnchor,
    ongoing_scroll: OngoingScroll,
    autoscroll_request: Option<(Autoscroll, bool)>,
}

impl ScrollManager {
    pub fn new() -> Self {
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
        let row = DisplayRow(scroll_top.floor().clamp(0.0, max_row as f64) as u32);
        let display_point = snapshot.clip_point(
            DisplayPoint::new(row, position.x.max(0.0) as u32),
            text::Bias::Left,
        );
        let anchor = snapshot.display_point_to_anchor(display_point, text::Bias::Left);
        let anchor_row = anchor.to_display_point(snapshot).row().0 as f64;
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
        self.scroll_manager.vertical_scroll_margin as usize
    }

    pub fn set_vertical_scroll_margin(&mut self, margin_rows: usize, cx: &mut Context<Self>) {
        self.scroll_manager.vertical_scroll_margin = margin_rows as ScrollOffset;
        cx.notify();
    }
}
