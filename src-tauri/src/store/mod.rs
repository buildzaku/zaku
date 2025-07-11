use once_cell::sync::Lazy;
use std::fs::{self};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use crate::{
    error::{Error, Result},
    space::models::SpaceReference,
    store::models::ZakuStore,
};

pub mod models;
pub mod spaces;

pub static STORE_ABSPATH: Lazy<PathBuf> = Lazy::new(|| {
    dirs::data_dir()
        .expect("Unable to get data directory")
        .join("Zaku/store")
        .with_extension("json")
});

static ZAKU_STORE: Lazy<RwLock<ZakuStore>> = Lazy::new(|| match ZakuStore::load(&STORE_ABSPATH) {
    Ok(store) => RwLock::new(store),
    Err(_) => RwLock::new(ZakuStore::default()),
});

impl ZakuStore {
    fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .map_err(|_| Error::FileReadError("Failed to read store file".into()))?;
        let store = serde_json::from_str(&content)?;

        Ok(store)
    }

    fn persist(&self) -> Result<()> {
        if let Some(parent) = STORE_ABSPATH.parent() {
            fs::create_dir_all(parent)?;
        }

        let serialized_store = serde_json::to_string_pretty(self)?;
        fs::write(&*STORE_ABSPATH, serialized_store)?;

        Ok(())
    }
}

pub fn get_active_spaceref() -> Option<SpaceReference> {
    ZAKU_STORE.read().ok()?.active_spaceref.clone()
}

pub fn set_active_spaceref(space_reference: SpaceReference) -> Result<()> {
    let mut zaku_store = ZAKU_STORE
        .write()
        .map_err(|_| Error::LockError("Failed to acquire write lock".into()))?;
    zaku_store.active_spaceref = Some(space_reference);
    zaku_store.persist()
}

pub fn get_spacerefs() -> Vec<SpaceReference> {
    ZAKU_STORE
        .read()
        .map(|s| s.spacerefs.clone())
        .unwrap_or_default()
}

pub fn set_spacerefs(spacerefs: Vec<SpaceReference>) -> Result<()> {
    let mut zaku_store = ZAKU_STORE
        .write()
        .map_err(|_| Error::LockError("Failed to acquire write lock".into()))?;
    zaku_store.spacerefs = spacerefs;
    zaku_store.persist()
}

pub fn insert_spaceref_if_missing(space_reference: SpaceReference) -> Result<()> {
    let mut zaku_store = ZAKU_STORE
        .write()
        .map_err(|_| Error::LockError("Failed to acquire write lock".into()))?;

    let spaceref_exists = zaku_store
        .spacerefs
        .iter()
        .any(|r| r.path == space_reference.path);

    if !spaceref_exists {
        zaku_store.spacerefs.push(space_reference);
        zaku_store.persist()?;
    }

    Ok(())
}

pub fn remove_spaceref(space_reference: SpaceReference) -> Result<()> {
    let mut zaku_store = ZAKU_STORE
        .write()
        .map_err(|_| Error::LockError("Failed to acquire write lock".into()))?;

    zaku_store
        .spacerefs
        .retain(|r| r.path != space_reference.path);

    if let Some(active) = &zaku_store.active_spaceref {
        if active.path == space_reference.path {
            zaku_store.active_spaceref = None;
        }
    }

    zaku_store.persist()
}
