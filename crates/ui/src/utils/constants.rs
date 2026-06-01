use gpui::Pixels;

#[cfg(target_os = "macos")]
pub const MACOS_TRAFFIC_LIGHT_PADDING: f32 = 78.0;
#[cfg(target_os = "macos")]
pub const MACOS_TRAFFIC_LIGHT_INSET: (Pixels, Pixels) = (gpui::px(9.5), gpui::px(9.5));

pub fn title_bar_height(rem_size: Pixels) -> Pixels {
    (1.75 * rem_size).max(gpui::px(32.0))
}
