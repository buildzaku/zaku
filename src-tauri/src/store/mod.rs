use serde::{Deserialize, Serialize};
use specta::Type;
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{error::Result, space::models::SpaceReference};

pub mod spaces;
pub mod user;
pub mod utils;

#[cfg(test)]
pub mod tests;

pub use spaces::{
    buffer::{ReqBuffer, SpaceBufferStore},
    cookie::SpaceCookieStore,
    settings::{AudioNotification, NotificationSettings, SpaceSettings, SpaceSettingsStore},
};
pub use user::settings::{UserSettings, UserSettingsStore};

#[derive(Debug, Serialize, Deserialize)]
pub struct Store {
    pub spaceref: Option<SpaceReference>,
    pub spacerefs: Vec<SpaceReference>,

    #[serde(skip)]
    abspath: PathBuf,
}

impl Store {
    pub fn new(store_abspath: PathBuf) -> Self {
        Self {
            spaceref: None,
            spacerefs: Vec::new(),
            abspath: store_abspath,
        }
    }

    fn init(store_abspath: &Path) -> Result<Store> {
        if !store_abspath.exists() {
            let default_store = Self::new(store_abspath.to_path_buf());
            default_store.fswrite()?;

            return Ok(default_store);
        }

        let store_content = fs::read_to_string(store_abspath)?;

        match serde_json::from_str::<Store>(&store_content) {
            Ok(mut store) => {
                store.abspath = store_abspath.to_path_buf();

                Ok(store)
            }
            Err(_) => {
                let default_store = Self::new(store_abspath.to_path_buf());
                default_store.fswrite()?;

                Ok(default_store)
            }
        }
    }

    fn fswrite(&self) -> Result<()> {
        if let Some(parent) = self.abspath.parent() {
            fs::create_dir_all(parent)?;
        }

        let serialized_store = serde_json::to_string_pretty(self)?;
        fs::write(&self.abspath, serialized_store)?;

        Ok(())
    }

    pub fn get(store_abspath: &Path) -> Result<Store> {
        Self::init(store_abspath)
    }

    pub fn update<F>(&mut self, mutator: F) -> Result<()>
    where
        F: FnOnce(&mut Store),
    {
        mutator(self);
        self.fswrite()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Type)]
pub enum Theme {
    System,
    Light,
    Dark,
}
