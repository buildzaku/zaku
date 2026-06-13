use gpui::{App, Global};
use std::{
    any::{Any, TypeId},
    collections::HashMap,
};

use settings_content::{ParseStatus, SettingsContent, merge_from::MergeFrom, parse_json};

pub struct RegisteredSetting {
    pub id: fn() -> TypeId,
    pub from_settings: fn(&SettingsContent) -> Box<dyn Any + Send + Sync>,
}

inventory::collect!(RegisteredSetting);

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
