use std::{fs, path::PathBuf};

use crate::{
    error::Result,
    store::models::SpaceSettings,
    utils::{hashed_filename, APP_DATA_DIR},
};

const SETTINGS_FILENAME: &str = "settings.json";

impl SpaceSettings {
    pub fn load(space_abspath: &str) -> Result<Self> {
        let settings_path = Self::filepath(space_abspath);
        let file_content = fs::read_to_string(&settings_path).unwrap_or_default();
        let settings = serde_json::from_str(&file_content).unwrap_or_default();

        Ok(settings)
    }

    pub fn persist(space_abspath: &str, settings: &Self) -> Result<()> {
        let settings_path = Self::filepath(space_abspath);

        if let Some(parent_dir) = settings_path.parent() {
            fs::create_dir_all(parent_dir)?;
        }

        let jsonstr = serde_json::to_string_pretty(settings)?;
        fs::write(&settings_path, jsonstr)?;

        Ok(())
    }

    fn filepath(space_abspath: &str) -> PathBuf {
        let hsh = hashed_filename(space_abspath);

        APP_DATA_DIR
            .join(super::SPACES_STORE_DIR)
            .join(hsh)
            .join(SETTINGS_FILENAME)
    }
}
