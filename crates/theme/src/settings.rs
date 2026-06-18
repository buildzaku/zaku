use gpui::{App, Font, FontFallbacks, FontFeatures, FontStyle, FontWeight, Pixels, Window};
use std::sync::Arc;

use settings::{IntoGpui, RegisterSetting};

const MIN_FONT_SIZE: Pixels = gpui::px(10.0);
const MAX_FONT_SIZE: Pixels = gpui::px(64.0);

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy, Default)]
pub enum UiDensity {
    #[default]
    Default,
    Compact,
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

#[derive(Debug, Clone, PartialEq, RegisterSetting)]
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
    pub fn get_global(cx: &App) -> &Self {
        <Self as settings::Settings>::get_global(cx)
    }

    pub fn override_global(settings: ThemeSettings, cx: &mut App) {
        <Self as settings::Settings>::override_global(settings, cx);
    }

    pub fn ui_font_size(&self, _cx: &App) -> Pixels {
        clamp_font_size(self.ui_font_size)
    }

    pub fn buffer_font_size(&self, _cx: &App) -> Pixels {
        clamp_font_size(self.buffer_font_size)
    }

    pub fn line_height(&self) -> f32 {
        const MIN_LINE_HEIGHT: f32 = 1.;
        f32::max(self.buffer_line_height.value(), MIN_LINE_HEIGHT)
    }
}

pub fn clamp_font_size(size: Pixels) -> Pixels {
    if size < MIN_FONT_SIZE {
        MIN_FONT_SIZE
    } else if size > MAX_FONT_SIZE {
        MAX_FONT_SIZE
    } else {
        size
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

fn font_fallbacks_from_settings(
    fallbacks: Option<&[settings::FontFamilyName]>,
) -> Option<FontFallbacks> {
    fallbacks.map(|fallbacks| {
        FontFallbacks::from_fonts(fallbacks.iter().cloned().map(String::from).collect())
    })
}

fn font_features_from_settings(features: Option<&settings::FontFeaturesContent>) -> FontFeatures {
    features
        .cloned()
        .map_or_else(|| FontFeatures(Arc::new(Vec::new())), IntoGpui::into_gpui)
}

fn font_weight_from_settings(weight: Option<settings::FontWeightContent>) -> FontWeight {
    weight.unwrap_or_default().into_gpui()
}

impl settings::Settings for ThemeSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let ui = content.ui.as_ref();
        let editor = content.editor.as_ref();
        let ui_font_size = clamp_font_size(
            ui.and_then(|ui| ui.font_size)
                .map_or_else(|| gpui::px(13.0), IntoGpui::into_gpui),
        );
        Self {
            ui_density: ui.and_then(|ui| ui.density).unwrap_or_default().into(),
            ui_font_size,
            ui_font: Font {
                family: ui
                    .and_then(|ui| ui.font_family.clone())
                    .map_or_else(|| ".SystemUIFont".into(), IntoGpui::into_gpui),
                features: font_features_from_settings(ui.and_then(|ui| ui.font_features.as_ref())),
                fallbacks: font_fallbacks_from_settings(
                    ui.and_then(|ui| ui.font_fallbacks.as_deref()),
                ),
                weight: font_weight_from_settings(ui.and_then(|ui| ui.font_weight)),
                style: FontStyle::default(),
            },
            buffer_font_size: clamp_font_size(
                editor
                    .and_then(|editor| editor.font_size)
                    .map_or_else(|| gpui::px(13.0), IntoGpui::into_gpui),
            ),
            buffer_font: Font {
                family: editor
                    .and_then(|editor| editor.font_family.clone())
                    .map_or_else(|| ".SystemUIFont".into(), IntoGpui::into_gpui),
                features: font_features_from_settings(
                    editor.and_then(|editor| editor.font_features.as_ref()),
                ),
                fallbacks: font_fallbacks_from_settings(
                    editor.and_then(|editor| editor.font_fallbacks.as_deref()),
                ),
                weight: font_weight_from_settings(editor.and_then(|editor| editor.font_weight)),
                style: FontStyle::default(),
            },
            buffer_line_height: editor
                .and_then(|editor| editor.line_height)
                .unwrap_or_default()
                .into(),
        }
    }
}
