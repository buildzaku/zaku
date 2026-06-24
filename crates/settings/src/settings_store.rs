use anyhow::Context;
use futures::{FutureExt, StreamExt, channel::mpsc, future::LocalBoxFuture};
use gpui::{App, AsyncApp, Global, Task};
use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::Arc,
};

use fs::Fs;
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
    setting_file_updates_tx: mpsc::UnboundedSender<
        Box<dyn FnOnce(AsyncApp) -> LocalBoxFuture<'static, anyhow::Result<()>>>,
    >,
    _setting_file_updates: Task<()>,
}

impl Global for SettingsStore {}

impl SettingsStore {
    pub fn new(cx: &mut App, default_settings_json: impl AsRef<str>) -> Self {
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
        let (setting_file_updates_tx, mut setting_file_updates_rx) = mpsc::unbounded();

        let mut store = Self {
            default_settings,
            user_settings: None,
            merged_settings,
            setting_factories: HashMap::new(),
            settings: HashMap::new(),
            setting_file_updates_tx,
            _setting_file_updates: cx.spawn(async move |cx| {
                while let Some(setting_file_update) = setting_file_updates_rx.next().await {
                    if let Err(error) = (setting_file_update)(cx.clone()).await {
                        log::warn!("Failed to update settings file: {error}");
                    }
                }
            }),
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

    #[cfg(any(test, feature = "test"))]
    pub fn test_new(cx: &mut App) -> Self {
        Self::new(cx, crate::default_settings())
    }

    pub async fn load_settings(fs: &Arc<dyn Fs>) -> anyhow::Result<String> {
        match fs.load(path::settings_file()).await {
            result @ Ok(_) => result,
            Err(error) => {
                if let Some(error) = error.downcast_ref::<std::io::Error>()
                    && error.kind() == std::io::ErrorKind::NotFound
                {
                    return Ok(crate::initial_user_settings().to_string());
                }
                Err(error)
            }
        }
    }

    pub fn update_settings_file(
        &self,
        fs: Arc<dyn Fs>,
        update: impl 'static + Send + FnOnce(&mut SettingsContent, &App),
    ) {
        if let Err(error) =
            self.setting_file_updates_tx
                .unbounded_send(Box::new(move |cx: AsyncApp| {
                    async move {
                        let old_text = Self::load_settings(&fs).await?;
                        let new_text = cx.read_global(|store: &SettingsStore, cx| {
                            store.new_text_for_update(&old_text, |content| update(content, cx))
                        })?;
                        let settings_path = path::settings_file();

                        fs.write(settings_path, new_text.as_bytes())
                            .await
                            .with_context(|| {
                                format!("Failed to write settings file {}", settings_path.display())
                            })?;

                        cx.update_global(|store: &mut SettingsStore, cx| {
                            let result = store.set_user_settings(&new_text, cx);
                            match result {
                                ParseStatus::Success => anyhow::Ok(()),
                                ParseStatus::Failed { error } => anyhow::bail!(error),
                            }
                        })?;

                        anyhow::Ok(())
                    }
                    .boxed_local()
                }))
        {
            log::warn!("Failed to update settings file: {error}");
        }
    }

    pub fn new_text_for_update(
        &self,
        old_text: &str,
        update: impl FnOnce(&mut SettingsContent),
    ) -> anyhow::Result<String> {
        let (old_content, parse_status) = if old_text.trim().is_empty() {
            parse_json::<SettingsContent>("{}")
        } else {
            parse_json::<SettingsContent>(old_text)
        };
        if let ParseStatus::Failed { error } = &parse_status {
            log::error!("Failed to parse settings for update: {error}");
        }
        let mut new_content = old_content
            .context("Settings file could not be parsed. Fix syntax errors before updating.")?;
        update(&mut new_content);
        serde_json::to_string_pretty(&new_content).context("Failed to serialize settings")
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

#[cfg(test)]
mod tests {
    use super::*;

    use indoc::indoc;

    use settings_content::ThemeAppearanceMode;

    #[gpui::test]
    fn test_update_theme_settings(cx: &mut App) {
        let store = SettingsStore::test_new(cx);
        let actual = store
            .new_text_for_update("{}", |content| {
                content.theme.get_or_insert_default().mode = Some(ThemeAppearanceMode::Dark);
            })
            .unwrap();

        assert_eq!(
            actual,
            indoc! {r#"
                {
                  "theme": {
                    "mode": "dark"
                  }
                }"#
            }
        );
    }
}
