use serde::{Deserialize, Serialize};
use specta::Type;
use std::{
    fs,
    ops::Deref,
    path::{Path, PathBuf},
};

use crate::{
    error::Result,
    store::{self, Theme, UserSettingsStore},
};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct AudioNotification {
    pub on_req_finish: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct NotificationSettings {
    pub audio: AudioNotification,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct SpaceSettings {
    pub theme: Theme,
    pub notifications: NotificationSettings,

    #[serde(skip)]
    abspath: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceSettingsStore(SpaceSettings);

impl Deref for SpaceSettingsStore {
    type Target = SpaceSettings;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl SpaceSettingsStore {
    fn new(sst_store_abspath: PathBuf) -> Self {
        let datadir_abspath = sst_store_abspath
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .expect("Invalid spacesettings path structure");

        let ust_store = UserSettingsStore::get(&store::utils::ust_store_abspath(datadir_abspath))
            .expect("Failed to get UserSettings");

        Self(SpaceSettings {
            theme: ust_store.default_theme.clone(),
            notifications: NotificationSettings {
                audio: AudioNotification {
                    on_req_finish: false,
                },
            },
            abspath: sst_store_abspath,
        })
    }

    fn init(sst_store_abspath: &Path) -> Result<Self> {
        if !sst_store_abspath.exists() {
            let sst_store = Self::new(sst_store_abspath.to_path_buf());
            sst_store.fswrite()?;

            return Ok(sst_store);
        }

        let file_content = fs::read_to_string(sst_store_abspath)?;

        match serde_json::from_str::<SpaceSettings>(&file_content) {
            Ok(mut space_settings) => {
                space_settings.abspath = sst_store_abspath.to_path_buf();

                Ok(Self(space_settings))
            }
            Err(_) => {
                let sst_store = Self::new(sst_store_abspath.to_path_buf());
                sst_store.fswrite()?;

                Ok(sst_store)
            }
        }
    }

    fn fswrite(&self) -> Result<()> {
        if let Some(parent_dir) = self.abspath.parent() {
            fs::create_dir_all(parent_dir)?;
        }

        let jsonstr = serde_json::to_string_pretty(self)?;
        fs::write(&self.abspath, jsonstr)?;

        Ok(())
    }

    pub fn get(sst_store_abspath: &Path) -> Result<SpaceSettingsStore> {
        Self::init(sst_store_abspath)
    }

    pub fn update<F>(&mut self, mutator: F) -> Result<()>
    where
        F: FnOnce(&mut SpaceSettings),
    {
        mutator(&mut self.0);
        self.fswrite()
    }

    /// Consumes the store and returns the inner `SpaceSettings`
    pub fn into_inner(self) -> SpaceSettings {
        self.0
    }
}
