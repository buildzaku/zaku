use gpui::{App, BorrowAppContext, Global, Pixels, Subscription};
use indexmap::IndexMap;
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{MapAccess, Visitor},
};
use std::{
    any::{Any, TypeId},
    collections::HashMap,
};

use settings_macros::{MergeFrom, with_fallible_options};

use crate::merge_from::MergeFrom;
use crate::{
    fallible_options::{ParseStatus, parse_json},
    paths,
};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiDensity {
    Compact,
    #[default]
    Default,
    Comfortable,
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Deserialize, MergeFrom)]
#[serde(transparent)]
pub struct FontWeightContent(pub f32);

impl Default for FontWeightContent {
    fn default() -> Self {
        Self(400.0)
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum FeatureValue {
    Bool(bool),
    Number(serde_json::Number),
}

fn is_valid_feature_tag(tag: &str) -> bool {
    tag.len() == 4 && tag.chars().all(|c| c.is_ascii_alphanumeric())
}

/// OpenType font features as a map of feature tag to value.
///
/// Values can be specified as booleans (true=1, false=0) or integers.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, MergeFrom)]
#[serde(transparent)]
pub struct FontFeaturesContent(pub IndexMap<String, u32>);

impl FontFeaturesContent {
    pub fn new() -> Self {
        Self(IndexMap::default())
    }
}

impl<'de> Deserialize<'de> for FontFeaturesContent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct FontFeaturesVisitor;

        impl<'de> Visitor<'de> for FontFeaturesVisitor {
            type Value = FontFeaturesContent;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a map of font features")
            }

            fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut feature_map = IndexMap::default();

                while let Some((key, value)) =
                    access.next_entry::<String, Option<FeatureValue>>()?
                {
                    if !is_valid_feature_tag(&key) {
                        eprintln!("settings: incorrect font feature tag: {key}");
                        continue;
                    }

                    let Some(value) = value else {
                        continue;
                    };

                    match value {
                        FeatureValue::Bool(enable) => {
                            feature_map.insert(key, if enable { 1 } else { 0 });
                        }
                        FeatureValue::Number(value) => {
                            if let Some(value) = value.as_u64() {
                                feature_map.insert(key, value as u32);
                            } else {
                                eprintln!(
                                    "settings: incorrect font feature value {value} for feature tag {key}",
                                );
                            }
                        }
                    }
                }

                Ok(FontFeaturesContent(feature_map))
            }
        }

        deserializer.deserialize_map(FontFeaturesVisitor)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, MergeFrom, Default)]
#[serde(rename_all = "snake_case")]
pub enum BufferLineHeight {
    #[default]
    Comfortable,
    Standard,
    Custom(#[serde(deserialize_with = "deserialize_line_height")] f32),
}

fn deserialize_line_height<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = f32::deserialize(deserializer)?;
    if value < 1.0 {
        return Err(serde::de::Error::custom(
            "buffer_line_height.custom must be at least 1.0",
        ));
    }

    Ok(value)
}

#[with_fallible_options]
#[derive(Clone, Default, Deserialize, MergeFrom)]
pub struct SettingsContent {
    ui_density: Option<UiDensity>,
    ui_font_size: Option<Pixels>,
    ui_font_family: Option<String>,
    ui_font_fallbacks: Option<Vec<String>>,
    ui_font_features: Option<FontFeaturesContent>,
    ui_font_weight: Option<FontWeightContent>,
    buffer_font_size: Option<Pixels>,
    buffer_font_family: Option<String>,
    buffer_font_fallbacks: Option<Vec<String>>,
    buffer_font_features: Option<FontFeaturesContent>,
    buffer_font_weight: Option<FontWeightContent>,
    buffer_line_height: Option<BufferLineHeight>,
}

impl SettingsContent {
    pub fn ui_density(&self) -> UiDensity {
        self.ui_density.unwrap_or_default()
    }

    pub fn ui_font_size(&self) -> Pixels {
        clamp_font_size(self.ui_font_size.unwrap_or(gpui::px(14.0)))
    }

    pub fn buffer_font_size(&self) -> Pixels {
        clamp_font_size(self.buffer_font_size.unwrap_or(gpui::px(14.0)))
    }

    pub fn ui_font_family(&self) -> Option<&str> {
        self.ui_font_family.as_deref()
    }

    pub fn ui_font_fallbacks(&self) -> Option<&[String]> {
        self.ui_font_fallbacks.as_deref()
    }

    pub fn ui_font_features(&self) -> Option<&FontFeaturesContent> {
        self.ui_font_features.as_ref()
    }

    pub fn ui_font_weight(&self) -> Option<FontWeightContent> {
        self.ui_font_weight
    }

    pub fn buffer_font_family(&self) -> Option<&str> {
        self.buffer_font_family.as_deref()
    }

    pub fn buffer_font_fallbacks(&self) -> Option<&[String]> {
        self.buffer_font_fallbacks.as_deref()
    }

    pub fn buffer_font_features(&self) -> Option<&FontFeaturesContent> {
        self.buffer_font_features.as_ref()
    }

    pub fn buffer_font_weight(&self) -> Option<FontWeightContent> {
        self.buffer_font_weight
    }

    pub fn buffer_line_height(&self) -> BufferLineHeight {
        self.buffer_line_height.unwrap_or_default()
    }
}

fn clamp_font_size(value: Pixels) -> Pixels {
    const MIN_FONT_SIZE: Pixels = gpui::px(6.0);
    const MAX_FONT_SIZE: Pixels = gpui::px(100.0);

    if value < MIN_FONT_SIZE {
        MIN_FONT_SIZE
    } else if value > MAX_FONT_SIZE {
        MAX_FONT_SIZE
    } else {
        value
    }
}

pub trait Settings: 'static + Send + Sync + Sized {
    fn from_settings(content: &SettingsContent) -> Self;

    #[track_caller]
    fn register(cx: &mut App) {
        cx.global_mut::<SettingsStore>().register_setting::<Self>();
    }

    #[track_caller]
    fn get_global(cx: &App) -> &Self {
        cx.global::<SettingsStore>().get::<Self>()
    }

    #[track_caller]
    fn override_global(settings: Self, cx: &mut App) {
        cx.global_mut::<SettingsStore>()
            .override_setting::<Self>(settings);
    }
}

pub struct SettingsStore {
    default_settings: SettingsContent,
    user_settings: Option<SettingsContent>,
    merged_settings: SettingsContent,
    setting_factories: HashMap<TypeId, fn(&SettingsContent) -> Box<dyn Any + Send + Sync>>,
    settings: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl Global for SettingsStore {}

impl SettingsStore {
    pub fn new(default_settings_json: impl AsRef<str>) -> Self {
        let default_settings = match parse_json::<SettingsContent>(default_settings_json.as_ref()) {
            ParseStatus::Ok(default_settings) => default_settings,
            ParseStatus::OkWithErrors { error, .. } => {
                panic!("invalid default settings: {error}");
            }
            ParseStatus::Err { error } => {
                panic!("failed to parse default settings: {error}")
            }
        };

        let merged_settings = default_settings.clone();

        Self {
            default_settings,
            user_settings: None,
            merged_settings,
            setting_factories: HashMap::new(),
            settings: HashMap::new(),
        }
    }

    pub fn content(&self) -> &SettingsContent {
        &self.merged_settings
    }

    pub fn observe_active_settings_profile_name(cx: &mut App) -> Subscription {
        cx.observe_global::<crate::ActiveSettingsProfileName>(|cx| {
            cx.update_global::<Self, _>(|store, cx| {
                store.recompute_values(cx);
            });
        })
    }

    pub fn load_settings() -> anyhow::Result<String> {
        let settings_path = paths::settings_file().as_path();
        match std::fs::read_to_string(settings_path) {
            Ok(text) => Ok(text),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(crate::default_user_settings().into_owned())
            }
            Err(error) => Err(error.into()),
        }
    }

    pub fn set_default_settings(&mut self, default_settings_content: &str, cx: &mut App) {
        let default_settings = match parse_json::<SettingsContent>(default_settings_content) {
            ParseStatus::Ok(default_settings) => default_settings,
            ParseStatus::OkWithErrors { value, error } => {
                eprintln!("settings: invalid default settings: {error}");
                value
            }
            ParseStatus::Err { error } => {
                eprintln!("settings: failed to parse default settings: {error}");
                return;
            }
        };

        self.default_settings = default_settings;
        self.recompute_values(cx);
    }

    pub fn set_user_settings(&mut self, user_settings_content: &str, cx: &mut App) {
        let user_settings = match parse_json::<SettingsContent>(user_settings_content) {
            ParseStatus::Ok(user_settings) => user_settings,
            ParseStatus::OkWithErrors { value, error } => {
                eprintln!("settings: invalid user settings: {error}");
                value
            }
            ParseStatus::Err { error } => {
                eprintln!("settings: failed to parse user settings: {error}");
                return;
            }
        };

        self.user_settings = Some(user_settings);
        self.recompute_values(cx);
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test(_cx: &mut App) -> Self {
        Self::new(crate::default_settings())
    }

    pub fn register_setting<T: Settings>(&mut self) {
        fn build<T: Settings>(content: &SettingsContent) -> Box<dyn Any + Send + Sync> {
            Box::new(T::from_settings(content))
        }

        let type_id = TypeId::of::<T>();
        self.setting_factories.insert(type_id, build::<T>);
        self.settings.insert(type_id, build::<T>(self.content()));
    }

    pub fn get<T: Settings>(&self) -> &T {
        self.settings
            .get(&TypeId::of::<T>())
            .and_then(|value| value.downcast_ref::<T>())
            .expect("setting was accessed before it was registered")
    }

    pub fn override_setting<T: Settings>(&mut self, value: T) {
        self.settings.insert(
            TypeId::of::<T>(),
            Box::new(value) as Box<dyn Any + Send + Sync>,
        );
    }

    fn recompute_values(&mut self, cx: &mut App) {
        let mut merged_settings = self.default_settings.clone();
        if let Some(user_settings) = self.user_settings.as_ref() {
            merged_settings.merge_from(user_settings);
        }
        self.merged_settings = merged_settings;

        let factories = self
            .setting_factories
            .iter()
            .map(|(type_id, factory)| (*type_id, *factory))
            .collect::<Vec<_>>();

        for (type_id, factory) in factories {
            let value = factory(self.content());
            self.settings.insert(type_id, value);
        }

        cx.refresh_windows();
    }
}
