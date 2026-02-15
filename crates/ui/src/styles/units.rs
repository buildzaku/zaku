use gpui::{Length, Rems, Window};

pub const BASE_REM_SIZE_IN_PX: f32 = 16.;

/// Returns a rem value derived from the provided pixel value and the base rem size (16px).
#[inline(always)]
pub fn rems_from_px(px: impl Into<f32>) -> Rems {
    gpui::rems(px.into() / BASE_REM_SIZE_IN_PX)
}

/// Returns a [`Length`] corresponding to the specified percentage of the viewport's width.
///
/// `percent` should be a value between `0.0` and `1.0`.
pub fn vw(percent: f32, window: &mut Window) -> Length {
    Length::from(window.viewport_size().width * percent)
}

/// Returns a [`Length`] corresponding to the specified percentage of the viewport's height.
///
/// `percent` should be a value between `0.0` and `1.0`.
pub fn vh(percent: f32, window: &mut Window) -> Length {
    Length::from(window.viewport_size().height * percent)
}
