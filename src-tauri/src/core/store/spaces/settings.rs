use serde::{Deserialize, Serialize};
use specta::Type;
use std::fs;
use std::path::PathBuf;

use crate::core::{
    store::spaces::SPACES_STORE_DIR,
    utils::{hashed_filename, ZAKU_DATA_DIR},
};

const SETTINGS_FILENAME: &str = "settings.json";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Type)]
pub enum Theme {
    System,
    Light,
    Dark,
}

impl Default for Theme {
    fn default() -> Self {
        Theme::System
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, Type)]
pub struct SpaceNotifSettings {
    pub play_success_sound: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, Type)]
pub struct SpaceSettings {
    pub theme: Theme,
    pub notification: SpaceNotifSettings,
}

impl SpaceSettings {
    pub fn load(space_abspath: &str) -> SpaceSettings {
        let settings_path = Self::filepath(space_abspath);
        let file_content = fs::read_to_string(&settings_path)
            .map_err(|error| {
                eprintln!("Failed to read settings file: {}", error);
            })
            .unwrap_or_default();
        let settings = serde_json::from_str(&file_content)
            .map_err(|error| {
                eprintln!("Failed to parse settings file: {}", error);
            })
            .unwrap_or_default();

        return settings;
    }

    pub fn persist(space_abspath: &str, settings: &SpaceSettings) -> Result<(), std::io::Error> {
        let settings_path = Self::filepath(space_abspath);
        if let Some(parent_dir) = settings_path.parent() {
            fs::create_dir_all(parent_dir).map_err(|error| {
                eprintln!("Could not create settings directory: {}", error);
                error
            })?;
        }

        let jsonstr = serde_json::to_string_pretty(settings).map_err(|error| {
            eprintln!("Could not serialize settings: {}", error);

            return std::io::Error::new(std::io::ErrorKind::Other, error);
        })?;

        fs::write(&settings_path, jsonstr).map_err(|error| {
            eprintln!("Could not write settings file: {}", error);

            return error;
        })?;

        Ok(())
    }

    fn filepath(space_abspath: &str) -> PathBuf {
        let hsh = hashed_filename(space_abspath);

        return ZAKU_DATA_DIR
            .join(SPACES_STORE_DIR)
            .join(hsh)
            .join(SETTINGS_FILENAME);
    }
}
