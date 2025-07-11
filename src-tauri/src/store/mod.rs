use once_cell::sync::Lazy;
use std::fs::{self};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use crate::{
    error::{Error, Result},
    space::models::SpaceReference,
    store::models::AppStore,
};

pub mod models;
pub mod spaces;

pub static APP_STORE_ABSPATH: Lazy<PathBuf> = Lazy::new(|| {
    dirs::data_dir()
        .expect("Unable to get data directory")
        .join("Zaku/store")
        .with_extension("json")
});

static APP_STORE: Lazy<RwLock<AppStore>> = Lazy::new(|| match AppStore::load(&APP_STORE_ABSPATH) {
    Ok(store) => RwLock::new(store),
    Err(_) => RwLock::new(AppStore::default()),
});

impl AppStore {
    fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .map_err(|_| Error::FileReadError("Failed to read store file".into()))?;
        let store = serde_json::from_str(&content)?;

        Ok(store)
    }

    fn persist(&self) -> Result<()> {
        if let Some(parent) = APP_STORE_ABSPATH.parent() {
            fs::create_dir_all(parent)?;
        }

        let serialized_store = serde_json::to_string_pretty(self)?;
        fs::write(&*APP_STORE_ABSPATH, serialized_store)?;

        Ok(())
    }
}

pub fn get_active_spaceref() -> Option<SpaceReference> {
    APP_STORE.read().ok()?.active_spaceref.clone()
}

pub fn set_active_spaceref(space_reference: SpaceReference) -> Result<()> {
    let mut app_store = APP_STORE
        .write()
        .map_err(|_| Error::LockError("Failed to acquire write lock".into()))?;
    app_store.active_spaceref = Some(space_reference);
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

    if let Some(active) = &app_store.active_spaceref {
        if active.path == space_reference.path {
            app_store.active_spaceref = None;
        }
    }

    app_store.persist()
}
