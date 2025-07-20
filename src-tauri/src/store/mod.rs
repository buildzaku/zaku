use once_cell::sync::Lazy;
use std::{fs, path::PathBuf, sync::RwLock};

use crate::{
    error::{Error, Result},
    space::models::SpaceReference,
    store::models::AppStore,
};

pub mod models;
pub mod spaces;

#[cfg(test)]
use tempfile;

#[cfg(test)]
static TEST_DIR: Lazy<tempfile::TempDir> = Lazy::new(|| tempfile::tempdir().unwrap());

#[cfg(test)]
static APP_STORE_ABSPATH: Lazy<PathBuf> =
    Lazy::new(|| TEST_DIR.path().join("Zaku/store").with_extension("json"));

#[cfg(not(test))]
static APP_STORE_ABSPATH: Lazy<PathBuf> = Lazy::new(|| {
    dirs::data_dir()
        .expect("Unable to get data directory")
        .join("Zaku/store")
        .with_extension("json")
});

static APP_STORE: Lazy<RwLock<AppStore>> = Lazy::new(|| match AppStore::init() {
    Ok(store) => RwLock::new(store),
    Err(_) => RwLock::new(AppStore::default()),
});

impl AppStore {
    fn init() -> Result<Self> {
        let store_abspath = APP_STORE_ABSPATH.as_path();
        let store_content = fs::read_to_string(store_abspath)
            .map_err(|_| Error::FileReadError("Failed to read store file".into()))?;
        let store = serde_json::from_str(&store_content)?;

        Ok(store)
    }

    fn persist(&self) -> Result<()> {
        let store_abspath = APP_STORE_ABSPATH.as_path();
        if let Some(parent) = store_abspath.parent() {
            fs::create_dir_all(parent)?;
        }

        let serialized_store = serde_json::to_string_pretty(self)?;
        fs::write(store_abspath, serialized_store)?;

        Ok(())
    }
}

pub fn get_spaceref() -> Option<SpaceReference> {
    APP_STORE.read().ok()?.spaceref.clone()
}

pub fn set_spaceref(space_reference: SpaceReference) -> Result<()> {
    let mut app_store = APP_STORE
        .write()
        .map_err(|_| Error::LockError("Failed to acquire write lock".into()))?;
    app_store.spaceref = Some(space_reference);
    app_store.persist()
}

pub fn get_spacerefs() -> Vec<SpaceReference> {
    APP_STORE
        .read()
        .map(|s| s.spacerefs.clone())
        .unwrap_or_default()
}

pub fn set_spacerefs(spacerefs: Vec<SpaceReference>) -> Result<()> {
    let mut app_store = APP_STORE
        .write()
        .map_err(|_| Error::LockError("Failed to acquire write lock".into()))?;
    app_store.spacerefs = spacerefs;
    app_store.persist()
}

pub fn insert_spaceref_if_missing(space_reference: SpaceReference) -> Result<()> {
    let mut app_store = APP_STORE
        .write()
        .map_err(|_| Error::LockError("Failed to acquire write lock".into()))?;

    let spaceref_exists = app_store
        .spacerefs
        .iter()
        .any(|r| r.path == space_reference.path);

    if !spaceref_exists {
        app_store.spacerefs.push(space_reference);
        app_store.persist()?;
    }

    Ok(())
}

pub fn remove_spaceref(space_reference: SpaceReference) -> Result<()> {
    let mut app_store = APP_STORE
        .write()
        .map_err(|_| Error::LockError("Failed to acquire write lock".into()))?;

    app_store
        .spacerefs
        .retain(|r| r.path != space_reference.path);

    if let Some(spaceref) = &app_store.spaceref {
        if spaceref.path == space_reference.path {
            app_store.spaceref = None;
        }
    }

    app_store.persist()
}
