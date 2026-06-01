mod with_rem_size;

pub use with_rem_size::*;

#[cfg(target_os = "macos")]
use gpui::App;
use gpui::Pixels;

pub fn reveal_in_file_manager_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "Reveal in Finder"
    } else if cfg!(target_os = "windows") {
        "Reveal in File Explorer"
    } else {
        "Reveal in File Manager"
    }
}

pub fn title_bar_height(rem_size: Pixels) -> Pixels {
    (1.75 * rem_size).max(gpui::px(32.0))
}

#[cfg(target_os = "macos")]
pub fn traffic_light_inset(title_bar_height: Pixels, cx: &App) -> (Pixels, Pixels) {
    let min_x_inset = gpui::px(9.5);
    let x_inset = crate::DynamicSpacing::Base08.px(cx).max(min_x_inset);
    let y_inset = (title_bar_height - gpui::px(12.5)) / 2.0;

    (x_inset, y_inset)
}

#[cfg(target_os = "macos")]
pub fn traffic_light_padding(title_bar_height: Pixels, cx: &App) -> Pixels {
    let traffic_light_width = gpui::px(60.0);
    let (x_inset, _) = traffic_light_inset(title_bar_height, cx);

    traffic_light_width + x_inset * 2.0
}
