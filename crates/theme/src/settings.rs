use gpui::{App, Font, FontFallbacks, FontFeatures, FontStyle, FontWeight, Pixels, Window};
use std::sync::Arc;

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
    pub buffer_line_height: BufferLineHeight,
}

impl Default for ThemeSettings {
    fn default() -> Self {
        let ui_font_size = gpui::px(14.);
        Self {
            ui_density: UiDensity::Default,
            ui_font_size,
            ui_font: Font::default(),
            buffer_font_size: ui_font_size,
            buffer_font: Font::default(),
            buffer_line_height: BufferLineHeight::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum BufferLineHeight {
    #[default]
    Comfortable,
    Standard,
    Custom(f32),
}

impl From<settings::BufferLineHeight> for BufferLineHeight {
    fn from(value: settings::BufferLineHeight) -> Self {
        match value {
            settings::BufferLineHeight::Comfortable => BufferLineHeight::Comfortable,
            settings::BufferLineHeight::Standard => BufferLineHeight::Standard,
            settings::BufferLineHeight::Custom(line_height) => {
                BufferLineHeight::Custom(line_height)
            }
        }
    }
}

impl BufferLineHeight {
    pub fn value(&self) -> f32 {
        match self {
            BufferLineHeight::Comfortable => 1.618,
            BufferLineHeight::Standard => 1.3,
            BufferLineHeight::Custom(line_height) => *line_height,
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

    pub fn line_height(&self) -> f32 {
        const MIN_LINE_HEIGHT: f32 = 1.;
        f32::max(self.buffer_line_height.value(), MIN_LINE_HEIGHT)
    }
}

pub fn setup_ui_font(window: &mut Window, cx: &mut App) -> Font {
    let (ui_font, ui_font_size) = {
        let settings = ThemeSettings::get_global(cx);
        (settings.ui_font.clone(), settings.ui_font_size(cx))
    };
    window.set_rem_size(ui_font_size);
    ui_font
}

fn font_fallbacks_from_settings(fallbacks: Option<&[String]>) -> Option<FontFallbacks> {
    fallbacks.map(|fallbacks| FontFallbacks::from_fonts(fallbacks.iter().cloned().collect()))
}

fn font_features_from_settings(features: Option<&settings::FontFeaturesContent>) -> FontFeatures {
    let list = features
        .map(|feature| {
            feature
                .0
                .iter()
                .map(|(tag, value)| (tag.clone(), *value))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    FontFeatures(Arc::new(list))
}

fn font_weight_from_settings(weight: Option<settings::FontWeightContent>) -> FontWeight {
    weight.unwrap_or_default().0.clamp(100., 950.).into()
}

impl settings::Settings for ThemeSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let ui_font_size = content.ui_font_size();
        Self {
            ui_density: content.ui_density().into(),
            ui_font_size,
            ui_font: Font {
                family: content
                    .ui_font_family()
                    .unwrap_or(".SystemUIFont")
                    .to_owned()
                    .into(),
                features: font_features_from_settings(content.ui_font_features()),
                fallbacks: font_fallbacks_from_settings(content.ui_font_fallbacks()),
                weight: font_weight_from_settings(content.ui_font_weight()),
                style: FontStyle::default(),
            },
            buffer_font_size: content.buffer_font_size(),
            buffer_font: Font {
                family: content
                    .buffer_font_family()
                    .unwrap_or(".SystemUIFont")
                    .to_owned()
                    .into(),
                features: font_features_from_settings(content.buffer_font_features()),
                fallbacks: font_fallbacks_from_settings(content.buffer_font_fallbacks()),
                weight: font_weight_from_settings(content.buffer_font_weight()),
                style: FontStyle::default(),
            },
            buffer_line_height: content.buffer_line_height().into(),
        }
    }
}
