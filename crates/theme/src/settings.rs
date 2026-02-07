use gpui::{App, Global, Pixels, px};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy, Default)]
pub enum UiDensity {
    Compact,
    #[default]
    Default,
    Comfortable,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ThemeSettings {
    pub ui_density: UiDensity,
    ui_font_size: Pixels,
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            ui_density: UiDensity::Default,
            ui_font_size: px(14.0),
        }
    }
}

#[derive(Default)]
struct GlobalThemeSettings(ThemeSettings);

impl Global for GlobalThemeSettings {}

impl ThemeSettings {
    pub fn init(cx: &mut App) {
        *cx.default_global::<GlobalThemeSettings>() = GlobalThemeSettings(ThemeSettings::default());
    }

    pub fn get_global(cx: &App) -> &Self {
        &cx.global::<GlobalThemeSettings>().0
    }

    pub fn override_global(settings: ThemeSettings, cx: &mut App) {
        *cx.global_mut::<GlobalThemeSettings>() = GlobalThemeSettings(settings);
    }

    pub fn ui_font_size(&self, _cx: &App) -> Pixels {
        self.ui_font_size
    }
}
