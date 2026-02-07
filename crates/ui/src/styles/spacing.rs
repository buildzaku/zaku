use gpui::{App, Pixels, Rems, px, rems};
use theme::{ThemeSettings, UiDensity};
use ui_macros::derive_dynamic_spacing;

derive_dynamic_spacing![
    (0, 0, 0),
    (1, 1, 2),
    (1, 2, 4),
    (2, 3, 4),
    (2, 4, 6),
    (3, 6, 8),
    (4, 8, 10),
    (10, 12, 14),
    (14, 16, 18),
    (18, 20, 22),
    24,
    32,
    40,
    48
];

pub fn ui_density(cx: &mut App) -> UiDensity {
    ThemeSettings::get_global(cx).ui_density
}
