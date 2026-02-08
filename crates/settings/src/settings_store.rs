use gpui::{App, BorrowAppContext, Global, Pixels};
use serde::Deserialize;
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

#[with_fallible_options]
#[derive(Clone, Default, Deserialize, MergeFrom)]
pub struct SettingsContent {
    ui_density: Option<UiDensity>,
    ui_font_size: Option<Pixels>,
    buffer_font_size: Option<Pixels>,
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
        let (default_settings, parse_status) =
            parse_json::<SettingsContent>(default_settings_json.as_ref());
        let Some(default_settings) = default_settings else {
            match parse_status {
                ParseStatus::Failed { error } => {
                    panic!("failed to parse default settings: {error}")
                }
                ParseStatus::Success => panic!("failed to parse default settings"),
            }
        };

        if let ParseStatus::Failed { error } = parse_status {
            panic!("invalid default settings: {error}");
        }

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

    pub fn observe_active_settings_profile_name(cx: &mut App) -> gpui::Subscription {
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
        let (default_settings, parse_status) =
            parse_json::<SettingsContent>(default_settings_content);
        let Some(default_settings) = default_settings else {
            if let ParseStatus::Failed { error } = parse_status {
                eprintln!("settings: failed to parse default settings: {error}");
            }
            return;
        };

        if let ParseStatus::Failed { error } = parse_status {
            eprintln!("settings: invalid default settings: {error}");
        }

        self.default_settings = default_settings;
        self.recompute_values(cx);
    }

    pub fn set_user_settings(&mut self, user_settings_content: &str, cx: &mut App) {
        let (user_settings, parse_status) = parse_json::<SettingsContent>(user_settings_content);
        let Some(user_settings) = user_settings else {
            if let ParseStatus::Failed { error } = parse_status {
                eprintln!("settings: failed to parse user settings: {error}");
            }
            return;
        };

        if let ParseStatus::Failed { error } = parse_status {
            eprintln!("settings: invalid user settings: {error}");
        }

        self.user_settings = Some(user_settings);
        self.recompute_values(cx);
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test(_cx: &mut App) -> Self {
        Self::new(crate::test_settings())
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
