use serde::{Deserialize, Serialize};
use specta::Type;
use std::{fs, path::PathBuf};

use crate::{error::Result, space::models::SpaceReference, store};

pub mod spaces;
pub mod utils;

#[cfg(test)]
pub mod tests;

pub use spaces::{
    buffer::{ReqBuf, SpaceBuf},
    cookie::SpaceCookies,
    settings::{AudioNotification, NotificationSettings, SpaceSettings},
};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Store {
    pub spaceref: Option<SpaceReference>,
    pub spacerefs: Vec<SpaceReference>,
}

impl Store {
    fn filename() -> &'static str {
        "store.json"
    }

    pub fn filepath() -> PathBuf {
        store::utils::datadir_abspath().join(Self::filename())
    }

    fn init() -> Result<Store> {
        let store_path = Self::filepath();
        if !store_path.exists() {
            let default_store = Self::default();
            Self::fswrite(&default_store)?;

            return Ok(default_store);
        }

        let store_content = fs::read_to_string(&store_path)?;

        match serde_json::from_str(&store_content) {
            Ok(store) => Ok(store),
            Err(_) => {
                // corrupt JSON, use default
                let default_store = Self::default();
                Self::fswrite(&default_store)?;

                Ok(default_store)
            }
        }
    }

    fn fswrite(store: &Store) -> Result<()> {
        let store_path = Self::filepath();

        if let Some(parent) = store_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let serialized_store = serde_json::to_string_pretty(store)?;
        fs::write(&store_path, serialized_store)?;

        Ok(())
    }

    pub fn get() -> Result<Store> {
        Self::init()
    }

    /// Updates the main store using a mutator function and persists changes to filesystem
    ///
    /// Loads the current store, applies the mutator function to modify store data
    /// and writes the changes to the filesystem. Each update operation is atomic
    /// and ensures data consistency.
    ///
    /// - `mutator`: Function that receives mutable store and applies modifications
    ///
    /// Returns a `Result<Store>` containing the updated store
    pub fn update<F>(mutator: F) -> Result<Store>
    where
        F: FnOnce(&mut Store),
    {
        let mut store = Self::get()?;
        mutator(&mut store);
        Self::fswrite(&store)?;

        Ok(store)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Type)]
pub enum Theme {
    System,
    Light,
    Dark,
}

pub fn get_spaceref() -> Option<SpaceReference> {
    Store::get().ok()?.spaceref
}

pub fn set_spaceref(space_reference: SpaceReference) -> Result<()> {
    Store::update(|store| {
        store.spaceref = Some(space_reference);
    })?;

    Ok(())
}

pub fn get_spacerefs() -> Vec<SpaceReference> {
    Store::get().map(|s| s.spacerefs).unwrap_or_default()
}

pub fn set_spacerefs(spacerefs: Vec<SpaceReference>) -> Result<()> {
    Store::update(|store| {
        store.spacerefs = spacerefs;
    })?;

    Ok(())
}

pub fn insert_spaceref_if_missing(space_reference: SpaceReference) -> Result<()> {
    Store::update(|store| {
        let spaceref_exists = store
            .spacerefs
            .iter()
            .any(|r| r.path == space_reference.path);

        if !spaceref_exists {
            store.spacerefs.push(space_reference);
        }
    })?;

    Ok(())
}

pub fn remove_spaceref(space_reference: SpaceReference) -> Result<()> {
    Store::update(|store| {
        store.spacerefs.retain(|r| r.path != space_reference.path);

        if let Some(spaceref) = &store.spaceref {
            if spaceref.path == space_reference.path {
                store.spaceref = None;
            }
        }
    })?;

    Ok(())
}
