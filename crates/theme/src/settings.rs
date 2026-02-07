use gpui::{App, Font, Global, Pixels};

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
    pub ui_font: Font,
    buffer_font_size: Pixels,
    pub buffer_font: Font,
}

impl Default for ThemeSettings {
    fn default() -> Self {
        let ui_font_size = gpui::px(14.0);
        Self {
            ui_density: UiDensity::Default,
            ui_font_size,
            ui_font: Font::default(),
            buffer_font_size: ui_font_size,
            buffer_font: Font::default(),
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

    pub fn buffer_font_size(&self, _cx: &App) -> Pixels {
        self.buffer_font_size
    }
}
