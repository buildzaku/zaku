use gpui::Rems;

pub const BASE_REM_SIZE_IN_PX: f32 = 16.;

/// Returns a rem value derived from the provided pixel value and the base rem size (16px).
#[inline(always)]
pub fn rems_from_px(px: impl Into<f32>) -> Rems {
    gpui::rems(px.into() / BASE_REM_SIZE_IN_PX)
}
