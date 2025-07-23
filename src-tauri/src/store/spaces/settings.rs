use serde::{Deserialize, Serialize};
use specta::Type;
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    error::Result,
    store::{self, models::Theme},
    utils,
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
}

impl Default for SpaceSettings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            notifications: NotificationSettings {
                audio: AudioNotification {
                    on_req_finish: false,
                },
            },
        }
    }
}

impl SpaceSettings {
    fn filename() -> &'static str {
        "settings.json"
    }

    pub fn filepath(space_abspath: &Path) -> PathBuf {
        let hsh = utils::hashed_filename(space_abspath);

        store::utils::datadir_abspath()
            .join(store::utils::SPACES_STORE_FSNAME)
            .join(hsh)
            .join(Self::filename())
    }

    fn init(space_abspath: &Path) -> Result<SpaceSettings> {
        let settings_filepath = Self::filepath(space_abspath);
        if !settings_filepath.exists() {
            let default_settings = Self::default();
            Self::fswrite(space_abspath, &default_settings)?;

            return Ok(default_settings);
        }

        let file_content = fs::read_to_string(&settings_filepath)?;

        match serde_json::from_str(&file_content) {
            Ok(settings) => Ok(settings),
            Err(_) => {
                // corrupt JSON, use default
                let default_settings = Self::default();
                Self::fswrite(space_abspath, &default_settings)?;

                Ok(default_settings)
            }
        }
    }

    fn fswrite(space_abspath: &Path, settings: &SpaceSettings) -> Result<()> {
        let settings_filepath = Self::filepath(space_abspath);

        if let Some(parent_dir) = settings_filepath.parent() {
            fs::create_dir_all(parent_dir)?;
        }

        let jsonstr = serde_json::to_string_pretty(settings)?;
        fs::write(&settings_filepath, jsonstr)?;

        Ok(())
    }

    pub fn get(space_abspath: &Path) -> Result<SpaceSettings> {
        Self::init(space_abspath)
    }

    pub fn update<F>(space_abspath: &Path, mutator: F) -> Result<SpaceSettings>
    where
        F: FnOnce(&mut SpaceSettings),
    {
        let mut space_settings = Self::get(space_abspath)?;
        mutator(&mut space_settings);
        Self::fswrite(space_abspath, &space_settings)?;

        Ok(space_settings)
    }
}
