use once_cell::sync::Lazy;
use std::fs::{self};
use std::path::PathBuf;
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::models::space::SpaceReference;
use crate::models::zaku::ZakuStore;

pub static STORE_ABSOLUTE_PATH: Lazy<PathBuf> = Lazy::new(|| {
    dirs::data_dir()
        .expect("Unable to get data directory")
        .join("Zaku/store")
        .with_extension("json")
});

static ZAKU_STORE: Lazy<RwLock<ZakuStore>> = Lazy::new(|| {
    if STORE_ABSOLUTE_PATH.exists() {
        let content = fs::read_to_string(&*STORE_ABSOLUTE_PATH).expect("Failed to read from store");
        let store: ZakuStore = serde_json::from_str(&content).expect("Failed to deserialize data");

        return RwLock::new(store);
    } else {
        return RwLock::new(ZakuStore::default());
    }
});

impl ZakuStore {
    fn acquire_read_lock() -> RwLockReadGuard<'static, Self> {
        ZAKU_STORE.read().expect("Failed to acquire read lock")
    }

    fn acquire_write_lock() -> RwLockWriteGuard<'static, Self> {
        ZAKU_STORE.write().expect("Failed to acquire write lock")
    }

    fn persist(&self) {
        let serialized_store =
            serde_json::to_string_pretty(self).expect("Failed to serialize store data");

        if let Some(parent) = STORE_ABSOLUTE_PATH.parent() {
            fs::create_dir_all(parent).expect("Failed to create parent directories");
        }

        fs::write(&*STORE_ABSOLUTE_PATH, serialized_store)
            .expect("Failed to write serialized store to disk");
    }
}

pub fn get_active_space_reference() -> Option<SpaceReference> {
    let zaku_store = ZakuStore::acquire_read_lock();

    return zaku_store.active_space_reference.clone();
}

pub fn set_active_space_reference(space_reference: SpaceReference) {
    let mut zaku_store = ZakuStore::acquire_write_lock();
    zaku_store.active_space_reference = Some(space_reference);

    ZakuStore::persist(&zaku_store);
}

pub fn get_space_references() -> Vec<SpaceReference> {
    let zaku_store = ZakuStore::acquire_read_lock();

    return zaku_store.space_references.clone();
}

pub fn set_space_references(space_references: Vec<SpaceReference>) {
    let mut zaku_store = ZakuStore::acquire_write_lock();
    zaku_store.space_references = space_references;

    ZakuStore::persist(&zaku_store);
}

pub fn insert_into_space_references_if_needed(space_reference: SpaceReference) {
    let mut zaku_store = ZakuStore::acquire_write_lock();

    let reference_exists = zaku_store
        .space_references
        .iter()
        .any(|reference| reference.path == space_reference.path);

    if !reference_exists {
        zaku_store.space_references.push(space_reference);

        ZakuStore::persist(&zaku_store);
    }
}

pub fn delete_space_reference(space_reference: SpaceReference) {
    let mut zaku_store = ZakuStore::acquire_write_lock();

    zaku_store
        .space_references
        .retain(|reference| reference.path != space_reference.path);

    if let Some(active_space) = &zaku_store.active_space_reference {
        if active_space.path == space_reference.path {
            zaku_store.active_space_reference = None;
        }
    }

    ZakuStore::persist(&zaku_store);
}
