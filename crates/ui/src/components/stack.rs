use gpui::Div;

use crate::StyledExt;

/// Horizontally stacks elements
#[track_caller]
pub fn h_flex() -> Div {
    gpui::div().h_flex()
}

/// Vertically stacks elements
#[track_caller]
pub fn v_flex() -> Div {
    gpui::div().v_flex()
}
