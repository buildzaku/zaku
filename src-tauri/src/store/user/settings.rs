use serde::{Deserialize, Serialize};
use specta::Type;
use std::{
    fs,
    ops::Deref,
    path::{Path, PathBuf},
};

use crate::{error::Result, store::Theme};

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct UserSettings {
    pub default_theme: Theme,

    #[serde(skip)]
    abspath: PathBuf,
}

#[derive(Debug, Clone)]
pub struct UserSettingsStore(UserSettings);

impl Deref for UserSettingsStore {
    type Target = UserSettings;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl UserSettingsStore {
    fn new(ust_store_abspath: PathBuf) -> Self {
        Self(UserSettings {
            default_theme: Theme::System,
            abspath: ust_store_abspath,
        })
    }

    fn init(ust_store_abspath: &Path) -> Result<Self> {
        if !ust_store_abspath.exists() {
            let default_settings = Self::new(ust_store_abspath.to_path_buf());
            default_settings.fswrite()?;
            return Ok(default_settings);
        }

        let content = fs::read_to_string(ust_store_abspath)?;

        match serde_json::from_str::<UserSettings>(&content) {
            Ok(mut settings) => {
                settings.abspath = ust_store_abspath.to_path_buf();
                Ok(Self(settings))
            }
            Err(_) => {
                let default_settings = Self::new(ust_store_abspath.to_path_buf());
                default_settings.fswrite()?;
                Ok(default_settings)
            }
        }
    }

    fn fswrite(&self) -> Result<()> {
        if let Some(parent) = self.0.abspath.parent() {
            fs::create_dir_all(parent)?;
        }

        let serialized_settings = serde_json::to_string_pretty(&self.0)?;
        fs::write(&self.0.abspath, serialized_settings)?;
        Ok(())
    }

    pub fn get(ust_store_abspath: &Path) -> Result<UserSettingsStore> {
        let store = Self::init(ust_store_abspath)?;

        Ok(store)
    }

    pub fn update<F>(&mut self, mutator: F) -> Result<()>
    where
        F: FnOnce(&mut UserSettings),
    {
        mutator(&mut self.0);
        self.fswrite()
    }

    /// Consumes the store and returns the inner `UserSettings`
    pub fn into_inner(self) -> UserSettings {
        self.0
    }
}
