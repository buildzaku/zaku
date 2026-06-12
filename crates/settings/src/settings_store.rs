use gpui::{App, Global, Pixels};
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

use crate::{
    editor::{CurrentLineHighlight, EditorSettingsContent, GutterContent},
    fallible_options::{ParseStatus, parse_json},
    merge_from::MergeFrom,
};

pub struct RegisteredSetting {
    pub id: fn() -> TypeId,
    pub from_settings: fn(&SettingsContent) -> Box<dyn Any + Send + Sync>,
}

inventory::collect!(RegisteredSetting);

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
                        log::error!("Invalid font feature tag: {key}");
                        continue;
                    }

                    let Some(value) = value else {
                        continue;
                    };

                    match value {
                        FeatureValue::Bool(enable) => {
                            feature_map.insert(key, u32::from(enable));
                        }
                        FeatureValue::Number(value) => {
                            if let Some(value) =
                                value.as_u64().and_then(|value| u32::try_from(value).ok())
                            {
                                feature_map.insert(key, value);
                            } else {
                                log::error!(
                                    "Invalid font feature value {value} for feature tag {key}",
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
            "editor.line_height.custom must be at least 1.0",
        ));
    }

    Ok(value)
}

#[with_fallible_options]
#[derive(Clone, Default, Deserialize, MergeFrom)]
pub struct UiSettingsContent {
    density: Option<UiDensity>,
    font_size: Option<Pixels>,
    font_family: Option<String>,
    font_fallbacks: Option<Vec<String>>,
    font_features: Option<FontFeaturesContent>,
    font_weight: Option<FontWeightContent>,
}

#[with_fallible_options]
#[derive(Clone, Default, Deserialize, MergeFrom)]
pub struct SettingsContent {
    ui: Option<UiSettingsContent>,
    editor: Option<EditorSettingsContent>,
    pub(crate) log: Option<HashMap<String, String>>,
}

impl SettingsContent {
    pub fn ui_density(&self) -> UiDensity {
        self.ui
            .as_ref()
            .and_then(|ui| ui.density)
            .unwrap_or_default()
    }

    pub fn ui_font_size(&self) -> Pixels {
        let font_size = self
            .ui
            .as_ref()
            .and_then(|ui| ui.font_size)
            .unwrap_or(gpui::px(13.0));
        clamp_font_size(font_size)
    }

    pub fn buffer_font_size(&self) -> Pixels {
        let font_size = self
            .editor
            .as_ref()
            .and_then(|editor| editor.font_size)
            .unwrap_or(gpui::px(13.0));
        clamp_font_size(font_size)
    }

    pub fn ui_font_family(&self) -> Option<&str> {
        self.ui.as_ref().and_then(|ui| ui.font_family.as_deref())
    }

    pub fn ui_font_fallbacks(&self) -> Option<&[String]> {
        self.ui.as_ref().and_then(|ui| ui.font_fallbacks.as_deref())
    }

    pub fn ui_font_features(&self) -> Option<&FontFeaturesContent> {
        self.ui.as_ref().and_then(|ui| ui.font_features.as_ref())
    }

    pub fn ui_font_weight(&self) -> Option<FontWeightContent> {
        self.ui.as_ref().and_then(|ui| ui.font_weight)
    }

    pub fn buffer_font_family(&self) -> Option<&str> {
        self.editor
            .as_ref()
            .and_then(|editor| editor.font_family.as_deref())
    }

    pub fn buffer_font_fallbacks(&self) -> Option<&[String]> {
        self.editor
            .as_ref()
            .and_then(|editor| editor.font_fallbacks.as_deref())
    }

    pub fn buffer_font_features(&self) -> Option<&FontFeaturesContent> {
        self.editor
            .as_ref()
            .and_then(|editor| editor.font_features.as_ref())
    }

    pub fn buffer_font_weight(&self) -> Option<FontWeightContent> {
        self.editor.as_ref().and_then(|editor| editor.font_weight)
    }

    pub fn buffer_line_height(&self) -> BufferLineHeight {
        self.editor
            .as_ref()
            .and_then(|editor| editor.line_height)
            .unwrap_or_default()
    }

    pub fn current_line_highlight(&self) -> Option<CurrentLineHighlight> {
        self.editor
            .as_ref()
            .and_then(|editor| editor.current_line_highlight)
    }

    pub fn gutter(&self) -> GutterContent {
        self.editor
            .as_ref()
            .and_then(|editor| editor.gutter.clone())
            .unwrap_or_default()
    }
}

fn clamp_font_size(value: Pixels) -> Pixels {
    const MIN_FONT_SIZE: Pixels = gpui::px(10.0);
    const MAX_FONT_SIZE: Pixels = gpui::px(64.0);

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
        let (default_settings, parse_status) =
            parse_json::<SettingsContent>(default_settings_json.as_ref());
        let default_settings = match (default_settings, parse_status) {
            (Some(default_settings), ParseStatus::Success) => default_settings,
            (Some(_), ParseStatus::Failed { error }) => {
                panic!("invalid default settings: {error}");
            }
            (None, ParseStatus::Failed { error }) => {
                panic!("failed to parse default settings: {error}")
            }
            (None, ParseStatus::Success) => {
                panic!("failed to parse default settings: missing parsed value")
            }
        };

        let merged_settings = default_settings.clone();

        let mut store = Self {
            default_settings,
            user_settings: None,
            merged_settings,
            setting_factories: HashMap::new(),
            settings: HashMap::new(),
        };
        store.load_settings_types();
        store
    }

    pub fn content(&self) -> &SettingsContent {
        &self.merged_settings
    }

    pub fn set_default_settings(&mut self, default_settings_content: &str, cx: &mut App) {
        let (default_settings, parse_status) =
            parse_json::<SettingsContent>(default_settings_content);
        let default_settings = match (default_settings, parse_status) {
            (Some(default_settings), ParseStatus::Success) => default_settings,
            (Some(default_settings), ParseStatus::Failed { error }) => {
                log::error!("Invalid default settings: {error}");
                default_settings
            }
            (None, ParseStatus::Failed { error }) => {
                log::error!("Failed to parse default settings: {error}");
                return;
            }
            (None, ParseStatus::Success) => {
                log::error!("Failed to parse default settings: missing parsed value");
                return;
            }
        };

        self.default_settings = default_settings;
        self.recompute_values(cx);
    }

    #[must_use]
    pub fn set_user_settings(&mut self, user_settings_content: &str, cx: &mut App) -> ParseStatus {
        let (user_settings, parse_status) = if user_settings_content.is_empty() {
            parse_json::<SettingsContent>("{}")
        } else {
            parse_json::<SettingsContent>(user_settings_content)
        };

        if let Some(user_settings) = user_settings {
            self.user_settings = Some(user_settings);
            self.recompute_values(cx);
        }

        parse_status
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test(_cx: &mut App) -> Self {
        Self::new(crate::default_settings())
    }

    pub fn register_setting<T: Settings>(&mut self) {
        fn build<T: Settings>(content: &SettingsContent) -> Box<dyn Any + Send + Sync> {
            Box::new(T::from_settings(content))
        }

        self.register_setting_internal(&RegisteredSetting {
            id: || TypeId::of::<T>(),
            from_settings: build::<T>,
        });
    }

    fn load_settings_types(&mut self) {
        for registered_setting in inventory::iter::<RegisteredSetting>() {
            self.register_setting_internal(registered_setting);
        }
    }

    fn register_setting_internal(&mut self, registered_setting: &RegisteredSetting) {
        let type_id = (registered_setting.id)();
        if self.settings.contains_key(&type_id) {
            return;
        }

        let from_settings = registered_setting.from_settings;
        self.setting_factories.insert(type_id, from_settings);
        self.settings.insert(type_id, from_settings(self.content()));
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

    fn recompute_values(&mut self, _cx: &mut App) {
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
    }
}
