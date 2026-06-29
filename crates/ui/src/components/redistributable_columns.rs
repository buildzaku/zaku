use gpui::{
    AbsoluteLength, AnyElement, App, AppContext, Bounds, Context, DefiniteLength, Div,
    DragMoveEvent, Empty, Entity, EntityId, Length, Pixels, Stateful, WeakEntity, Window,
    prelude::*,
};
use std::rc::Rc;

use theme::ActiveTheme;

use super::data_table::{
    ResizableColumnsState,
    table_row::{IntoTableRow, TableRow},
};

pub(crate) const RESIZE_COLUMN_WIDTH: f32 = 8.0;
pub(crate) const RESIZE_DIVIDER_WIDTH: f32 = 1.0;

#[derive(Debug)]
pub(crate) struct DraggedColumn {
    pub(crate) column_index: usize,
    pub(crate) state_id: EntityId,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TableResizeBehavior {
    None,
    Resizable,
    MinSize(f32),
}

impl TableResizeBehavior {
    pub fn is_resizable(&self) -> bool {
        *self != TableResizeBehavior::None
    }

    pub fn min_size(&self) -> Option<f32> {
        match self {
            TableResizeBehavior::None => None,
            TableResizeBehavior::Resizable => Some(0.05),
            TableResizeBehavior::MinSize(min_size) => Some(*min_size),
        }
    }
}

#[derive(Clone)]
pub(crate) enum ColumnsStateRef {
    Redistributable(WeakEntity<RedistributableColumnsState>),
    Resizable(WeakEntity<ResizableColumnsState>),
}

#[derive(Clone)]
pub struct HeaderResizeInfo {
    pub(crate) columns_state: ColumnsStateRef,
    pub resize_behavior: TableRow<TableResizeBehavior>,
}

impl HeaderResizeInfo {
    pub fn from_redistributable(
        columns_state: &Entity<RedistributableColumnsState>,
        cx: &App,
    ) -> Self {
        let resize_behavior = columns_state.read(cx).resize_behavior().clone();
        Self {
            columns_state: ColumnsStateRef::Redistributable(columns_state.downgrade()),
            resize_behavior,
        }
    }

    pub fn from_resizable(columns_state: &Entity<ResizableColumnsState>, cx: &App) -> Self {
        let resize_behavior = columns_state.read(cx).resize_behavior().clone();
        Self {
            columns_state: ColumnsStateRef::Resizable(columns_state.downgrade()),
            resize_behavior,
        }
    }

    pub fn reset_column(&self, column_index: usize, window: &mut Window, cx: &mut App) {
        match &self.columns_state {
            ColumnsStateRef::Redistributable(weak) => {
                if let Err(error) = weak.update(cx, |state, cx| {
                    state.reset_column_to_initial_width(column_index, window);
                    cx.notify();
                }) {
                    log::trace!("Failed to reset table column: {error:?}");
                }
            }
            ColumnsStateRef::Resizable(weak) => {
                if let Err(error) = weak.update(cx, |state, cx| {
                    state.reset_column_to_initial_width(column_index);
                    cx.notify();
                }) {
                    log::trace!("Failed to reset table column: {error:?}");
                }
            }
        }
    }
}

pub struct RedistributableColumnsState {
    pub(crate) initial_widths: TableRow<DefiniteLength>,
    pub(crate) committed_widths: TableRow<DefiniteLength>,
    pub(crate) preview_widths: TableRow<DefiniteLength>,
    pub(crate) resize_behavior: TableRow<TableResizeBehavior>,
    pub(crate) cached_container_width: Pixels,
}

impl RedistributableColumnsState {
    pub fn new(
        cols: usize,
        initial_widths: Vec<impl Into<DefiniteLength>>,
        resize_behavior: Vec<TableResizeBehavior>,
    ) -> Self {
        let widths: TableRow<DefiniteLength> = initial_widths
            .into_iter()
            .map(Into::into)
            .collect::<Vec<_>>()
            .into_table_row(cols);
        Self {
            initial_widths: widths.clone(),
            committed_widths: widths.clone(),
            preview_widths: widths,
            resize_behavior: resize_behavior.into_table_row(cols),
            cached_container_width: Pixels::default(),
        }
    }

    pub fn cols(&self) -> usize {
        self.committed_widths.cols()
    }

    pub fn initial_widths(&self) -> &TableRow<DefiniteLength> {
        &self.initial_widths
    }

    pub fn preview_widths(&self) -> &TableRow<DefiniteLength> {
        &self.preview_widths
    }

    pub fn resize_behavior(&self) -> &TableRow<TableResizeBehavior> {
        &self.resize_behavior
    }

    pub fn widths_to_render(&self) -> TableRow<Length> {
        self.preview_widths.map_cloned(Length::Definite)
    }

    pub fn preview_fractions(&self, rem_size: Pixels) -> TableRow<f32> {
        if self.cached_container_width > gpui::px(0.0) {
            self.preview_widths
                .map_ref(|length| Self::get_fraction(length, self.cached_container_width, rem_size))
        } else {
            self.preview_widths.map_ref(|length| match length {
                DefiniteLength::Fraction(fraction) => *fraction,
                DefiniteLength::Absolute(_) => 0.0,
            })
        }
    }

    pub fn preview_column_width(&self, column_index: usize, window: &Window) -> Option<Pixels> {
        let width = self.preview_widths().as_slice().get(column_index)?;
        match width {
            DefiniteLength::Fraction(fraction) if self.cached_container_width > gpui::px(0.0) => {
                Some(self.cached_container_width * *fraction)
            }
            DefiniteLength::Fraction(_) => None,
            DefiniteLength::Absolute(AbsoluteLength::Pixels(pixels)) => Some(*pixels),
            DefiniteLength::Absolute(AbsoluteLength::Rems(rems_width)) => {
                Some(rems_width.to_pixels(window.rem_size()))
            }
        }
    }

    pub fn cached_container_width(&self) -> Pixels {
        self.cached_container_width
    }

    pub fn set_cached_container_width(&mut self, width: Pixels) {
        self.cached_container_width = width;
    }

    pub fn commit_preview(&mut self) {
        self.committed_widths = self.preview_widths.clone();
    }

    pub fn reset_column_to_initial_width(&mut self, column_index: usize, window: &Window) {
        let bounds_width = self.cached_container_width;
        if bounds_width <= gpui::px(0.0) {
            return;
        }

        let rem_size = window.rem_size();
        let initial_sizes = self
            .initial_widths
            .map_ref(|length| Self::get_fraction(length, bounds_width, rem_size));
        let widths = self
            .committed_widths
            .map_ref(|length| Self::get_fraction(length, bounds_width, rem_size));

        let updated_widths = Self::reset_to_initial_size(
            column_index,
            widths,
            &initial_sizes,
            &self.resize_behavior,
        );
        self.committed_widths = updated_widths.map(DefiniteLength::Fraction);
        self.preview_widths = self.committed_widths.clone();
    }

    fn get_fraction(length: &DefiniteLength, bounds_width: Pixels, rem_size: Pixels) -> f32 {
        match length {
            DefiniteLength::Absolute(AbsoluteLength::Pixels(pixels)) => *pixels / bounds_width,
            DefiniteLength::Absolute(AbsoluteLength::Rems(rems_width)) => {
                rems_width.to_pixels(rem_size) / bounds_width
            }
            DefiniteLength::Fraction(fraction) => *fraction,
        }
    }

    pub(crate) fn reset_to_initial_size(
        column_index: usize,
        mut widths: TableRow<f32>,
        initial_sizes: &TableRow<f32>,
        resize_behavior: &TableRow<TableResizeBehavior>,
    ) -> TableRow<f32> {
        let initial_size = *initial_sizes.expect_get(column_index);
        let width = *widths.expect_get(column_index);
        let diff = initial_size - width;

        let initial_left_sum = initial_sizes
            .as_slice()
            .get(..column_index)
            .expect("table row range should exist")
            .iter()
            .sum::<f32>();
        let left_sum = widths
            .as_slice()
            .get(..column_index)
            .expect("table row range should exist")
            .iter()
            .sum::<f32>();
        let left_diff = initial_left_sum - left_sum;

        let initial_right_sum = initial_sizes
            .as_slice()
            .get(column_index + 1..)
            .expect("table row range should exist")
            .iter()
            .sum::<f32>();
        let right_sum = widths
            .as_slice()
            .get(column_index + 1..)
            .expect("table row range should exist")
            .iter()
            .sum::<f32>();
        let right_diff = initial_right_sum - right_sum;

        let go_left_first = if diff < 0.0 {
            left_diff > right_diff
        } else {
            left_diff < right_diff
        };

        if go_left_first {
            let diff_remaining =
                Self::propagate_resize_diff(diff, column_index, &mut widths, resize_behavior, -1);

            if diff_remaining != 0.0 {
                Self::propagate_resize_diff(
                    diff_remaining,
                    column_index,
                    &mut widths,
                    resize_behavior,
                    1,
                );
            }
        } else {
            let diff_remaining =
                Self::propagate_resize_diff(diff, column_index, &mut widths, resize_behavior, 1);

            if diff_remaining != 0.0 && column_index > 0 {
                Self::propagate_resize_diff(
                    diff_remaining,
                    column_index,
                    &mut widths,
                    resize_behavior,
                    -1,
                );
            }
        }

        widths
    }

    fn on_drag_move(
        &mut self,
        drag_event: &DragMoveEvent<DraggedColumn>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let drag_position = drag_event.event.position;
        let bounds = drag_event.bounds;
        let bounds_width = bounds.right() - bounds.left();
        if bounds_width <= gpui::px(0.0) {
            return;
        }

        let mut column_position = 0.0;
        let rem_size = window.rem_size();
        let column_index = drag_event.drag(cx).column_index;

        let divider_width = Self::get_fraction(
            &DefiniteLength::Absolute(AbsoluteLength::Pixels(gpui::px(RESIZE_DIVIDER_WIDTH))),
            bounds_width,
            rem_size,
        );

        let mut widths = self
            .committed_widths
            .map_ref(|length| Self::get_fraction(length, bounds_width, rem_size));

        let left_widths = widths
            .as_slice()
            .get(..=column_index)
            .expect("table row range should exist");
        for length in left_widths {
            column_position += *length + divider_width;
        }

        let mut total_length_ratio = column_position;
        let right_widths = widths
            .as_slice()
            .get(column_index + 1..)
            .expect("table row range should exist");
        for length in right_widths {
            total_length_ratio += *length;
        }
        let trailing_columns =
            u16::try_from(right_widths.len()).expect("table column count should fit in u16");
        total_length_ratio += f32::from(trailing_columns) * divider_width;

        let drag_fraction = (drag_position.x - bounds.left()) / bounds_width;
        let drag_fraction = drag_fraction * total_length_ratio;
        let diff = drag_fraction - column_position - divider_width / 2.0;

        Self::drag_column_handle(diff, column_index, &mut widths, &self.resize_behavior);

        self.preview_widths = widths.map(DefiniteLength::Fraction);
    }

    pub(crate) fn drag_column_handle(
        diff: f32,
        column_index: usize,
        widths: &mut TableRow<f32>,
        resize_behavior: &TableRow<TableResizeBehavior>,
    ) {
        if diff > 0.0 {
            Self::propagate_resize_diff(diff, column_index, widths, resize_behavior, 1);
        } else {
            Self::propagate_resize_diff(-diff, column_index + 1, widths, resize_behavior, -1);
        }
    }

    pub(crate) fn propagate_resize_diff(
        diff: f32,
        column_index: usize,
        widths: &mut TableRow<f32>,
        resize_behavior: &TableRow<TableResizeBehavior>,
        direction: i8,
    ) -> f32 {
        let mut diff_remaining = diff;
        if resize_behavior
            .expect_get(column_index)
            .min_size()
            .is_none()
        {
            return diff;
        }

        let step_right;
        let step_left;
        if direction < 0 {
            step_right = 0;
            step_left = 1;
        } else {
            step_right = 1;
            step_left = 0;
        }

        if column_index == 0 && direction < 0 {
            return diff;
        }

        let mut current_column = column_index + step_right - step_left;

        while diff_remaining != 0.0 && current_column < widths.cols() {
            let Some(min_size) = resize_behavior.expect_get(current_column).min_size() else {
                if current_column == 0 {
                    break;
                }
                current_column -= step_left;
                current_column += step_right;
                continue;
            };

            let current_width = *widths.expect_get(current_column) - diff_remaining;
            *widths.expect_get_mut(current_column) = current_width;

            if min_size > current_width {
                diff_remaining = min_size - current_width;
                *widths.expect_get_mut(current_column) = min_size;
            } else {
                diff_remaining = 0.0;
                break;
            }

            if current_column == 0 {
                break;
            }
            current_column -= step_left;
            current_column += step_right;
        }
        *widths.expect_get_mut(column_index) += diff - diff_remaining;

        diff_remaining
    }
}

pub fn bind_redistributable_columns(
    container: Div,
    columns_state: Entity<RedistributableColumnsState>,
) -> Div {
    container
        .on_drag_move::<DraggedColumn>({
            let columns_state = columns_state.clone();
            move |event, window, cx| {
                if event.drag(cx).state_id != columns_state.entity_id() {
                    return;
                }
                columns_state.update(cx, |columns, cx| {
                    columns.on_drag_move(event, window, cx);
                });
            }
        })
        .on_children_prepainted({
            let columns_state = columns_state.clone();
            move |bounds, _, cx| {
                if let Some(width) = child_bounds_width(&bounds) {
                    columns_state.update(cx, |columns, _| {
                        columns.set_cached_container_width(width);
                    });
                }
            }
        })
        .on_drop::<DraggedColumn>(move |_, _, cx| {
            columns_state.update(cx, |columns, _| {
                columns.commit_preview();
            });
        })
}

pub fn render_redistributable_columns_resize_handles(
    columns_state: &Entity<RedistributableColumnsState>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let (column_widths, resize_behavior) = {
        let state = columns_state.read(cx);
        (state.widths_to_render(), state.resize_behavior().clone())
    };

    let mut column_index = 0;
    let resize_behavior = Rc::new(resize_behavior);
    let dividers = itertools::intersperse_with(
        column_widths
            .as_slice()
            .iter()
            .copied()
            .map(|width| resize_spacer(width).into_any_element()),
        || {
            let current_column_index = column_index;
            let resize_behavior = Rc::clone(&resize_behavior);
            let columns_state = columns_state.clone();
            column_index += 1;

            {
                let divider = gpui::div().id(current_column_index).relative().top_0();
                let entity_id = columns_state.entity_id();
                let on_reset: Rc<dyn Fn(&mut Window, &mut App)> = {
                    let columns_state = columns_state.clone();
                    Rc::new(move |window, cx| {
                        columns_state.update(cx, |columns, cx| {
                            columns.reset_column_to_initial_width(current_column_index, window);
                            cx.notify();
                        });
                    })
                };
                let on_drag_end: Option<Rc<dyn Fn(&mut App)>> = {
                    Some(Rc::new(move |cx| {
                        columns_state.update(cx, |state, _| state.commit_preview());
                    }))
                };
                render_column_resize_divider(
                    divider,
                    current_column_index,
                    resize_behavior
                        .expect_get(current_column_index)
                        .is_resizable(),
                    entity_id,
                    on_reset,
                    on_drag_end,
                    window,
                    cx,
                )
            }
        },
    );

    gpui::div()
        .flex()
        .flex_row()
        .id("resize-handles")
        .absolute()
        .inset_0()
        .w_full()
        .children(dividers)
        .into_any_element()
}

pub(crate) fn render_column_resize_divider(
    divider: Stateful<Div>,
    column_index: usize,
    is_resizable: bool,
    entity_id: EntityId,
    on_reset: Rc<dyn Fn(&mut Window, &mut App)>,
    on_drag_end: Option<Rc<dyn Fn(&mut App)>>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    window.with_id(column_index, |window| {
        let mut resize_divider = divider.w(gpui::px(RESIZE_DIVIDER_WIDTH)).h_full().bg(cx
            .theme()
            .colors()
            .border
            .opacity(0.8));

        let mut resize_handle = gpui::div()
            .id("column-resize-handle")
            .absolute()
            .left_neg_0p5()
            .w(gpui::px(RESIZE_COLUMN_WIDTH))
            .h_full();

        if is_resizable {
            let is_highlighted = window.use_state(cx, |_window, _cx| false);

            resize_divider = resize_divider.when(*is_highlighted.read(cx), |div| {
                div.bg(cx.theme().colors().border_focused)
            });

            resize_handle = resize_handle
                .on_hover({
                    let is_highlighted = is_highlighted.clone();
                    move |&was_hovered, _, cx| is_highlighted.write(cx, was_hovered)
                })
                .cursor_col_resize()
                .on_click(move |event, window, cx| {
                    if event.click_count() >= 2 {
                        on_reset(window, cx);
                    }
                    cx.stop_propagation();
                })
                .on_drag(
                    DraggedColumn {
                        column_index,
                        state_id: entity_id,
                    },
                    {
                        let is_highlighted = is_highlighted.clone();
                        move |_, _offset, _window, cx| {
                            is_highlighted.write(cx, true);
                            cx.new(|_cx| Empty)
                        }
                    },
                )
                .on_drop::<DraggedColumn>(move |_, _, cx| {
                    is_highlighted.write(cx, false);
                    if let Some(on_drag_end) = &on_drag_end {
                        on_drag_end(cx);
                    }
                });
        }

        resize_divider.child(resize_handle).into_any_element()
    })
}

fn resize_spacer(width: Length) -> Div {
    gpui::div().w(width).h_full()
}

fn child_bounds_width(bounds: &[Bounds<Pixels>]) -> Option<Pixels> {
    let first_bounds = bounds.first()?;
    let mut left = first_bounds.left();
    let mut right = first_bounds.right();

    for bound in bounds.iter().skip(1) {
        left = left.min(bound.left());
        right = right.max(bound.right());
    }

    Some(right - left)
}
