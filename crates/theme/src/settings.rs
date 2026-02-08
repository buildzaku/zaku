use gpui::{App, Font, Pixels};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy, Default)]
pub enum UiDensity {
    Compact,
    #[default]
    Default,
    Comfortable,
}

impl From<settings::UiDensity> for UiDensity {
    fn from(value: settings::UiDensity) -> Self {
        match value {
            settings::UiDensity::Compact => Self::Compact,
            settings::UiDensity::Default => Self::Default,
            settings::UiDensity::Comfortable => Self::Comfortable,
        }
    }
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

impl ThemeSettings {
    pub fn init(cx: &mut App) {
        <Self as settings::Settings>::register(cx);
    }

    pub fn get_global(cx: &App) -> &Self {
        <Self as settings::Settings>::get_global(cx)
    }

    pub fn override_global(settings: ThemeSettings, cx: &mut App) {
        <Self as settings::Settings>::override_global(settings, cx);
    }

    pub fn ui_font_size(&self, _cx: &App) -> Pixels {
        self.ui_font_size
    }

    pub fn buffer_font_size(&self, _cx: &App) -> Pixels {
        self.buffer_font_size
    }
}

impl settings::Settings for ThemeSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let ui_font_size = content.ui_font_size();
        Self {
            ui_density: content.ui_density().into(),
            ui_font_size,
            ui_font: Font::default(),
            buffer_font_size: content.buffer_font_size(),
            buffer_font: Font::default(),
        }
    }
}
