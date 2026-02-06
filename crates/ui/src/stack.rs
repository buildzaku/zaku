use gpui::{Div, Styled};

/// Horizontally stacks elements. Sets `flex()`, `flex_row()`, `items_center()`.
#[track_caller]
pub fn h_flex() -> Div {
    gpui::div().flex().flex_row().items_center()
}

/// Vertically stacks elements. Sets `flex()`, `flex_col()`.
#[track_caller]
pub fn v_flex() -> Div {
    gpui::div().flex().flex_col()
}
