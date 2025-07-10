use once_cell::sync::Lazy;
use std::fs::{self};
use std::path::PathBuf;
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::{space::models::SpaceReference, store::models::ZakuStore};

pub mod models;
pub mod spaces;

pub static STORE_ABSPATH: Lazy<PathBuf> = Lazy::new(|| {
    dirs::data_dir()
        .expect("Unable to get data directory")
        .join("Zaku/store")
        .with_extension("json")
});

static ZAKU_STORE: Lazy<RwLock<ZakuStore>> = Lazy::new(|| {
    if STORE_ABSPATH.exists() {
        let content = fs::read_to_string(&*STORE_ABSPATH).expect("Failed to read from store");
        let store: ZakuStore = serde_json::from_str(&content).expect("Failed to deserialize data");

        RwLock::new(store)
    } else {
        RwLock::new(ZakuStore::default())
    }
});

impl ZakuStore {
    fn acq_rlock() -> RwLockReadGuard<'static, Self> {
        ZAKU_STORE.read().expect("Failed to acquire read lock")
    }

    fn acq_wlock() -> RwLockWriteGuard<'static, Self> {
        ZAKU_STORE.write().expect("Failed to acquire write lock")
    }

    fn persist(&self) {
        let serialized_store =
            serde_json::to_string_pretty(self).expect("Failed to serialize store data");

        if let Some(parent) = STORE_ABSPATH.parent() {
            fs::create_dir_all(parent).expect("Failed to create parent directories");
        }

        fs::write(&*STORE_ABSPATH, serialized_store)
            .expect("Failed to write serialized store to disk");
    }
}

pub fn get_active_spaceref() -> Option<SpaceReference> {
    let zaku_store = ZakuStore::acq_rlock();

    zaku_store.active_spaceref.clone()
}

pub fn set_active_spaceref(space_reference: SpaceReference) {
    let mut zaku_store = ZakuStore::acq_wlock();
    zaku_store.active_spaceref = Some(space_reference);

    ZakuStore::persist(&zaku_store);
}

pub fn get_spacerefs() -> Vec<SpaceReference> {
    let zaku_store = ZakuStore::acq_rlock();

    zaku_store.spacerefs.clone()
}

pub fn set_spacerefs(spacerefs: Vec<SpaceReference>) {
    let mut zaku_store = ZakuStore::acq_wlock();
    zaku_store.spacerefs = spacerefs;

    ZakuStore::persist(&zaku_store);
}

pub fn insert_spaceref_if_missing(space_reference: SpaceReference) {
    let mut zaku_store = ZakuStore::acq_wlock();

    let reference_exists = zaku_store
        .spacerefs
        .iter()
        .any(|reference| reference.path == space_reference.path);

    if !reference_exists {
        zaku_store.spacerefs.push(space_reference);

        ZakuStore::persist(&zaku_store);
    }
}

pub fn remove_spaceref(space_reference: SpaceReference) {
    let mut zaku_store = ZakuStore::acq_wlock();

    zaku_store
        .spacerefs
        .retain(|reference| reference.path != space_reference.path);

    if let Some(active_space) = &zaku_store.active_spaceref {
        if active_space.path == space_reference.path {
            zaku_store.active_spaceref = None;
        }
    }

    ZakuStore::persist(&zaku_store);
}
