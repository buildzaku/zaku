use once_cell::sync::Lazy;
use postcard;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::models::space::SpaceReference;
use crate::models::zaku::ZakuStore;

static STORE_ABSOLUTE_PATH: Lazy<PathBuf> = Lazy::new(|| {
    dirs::data_dir()
        .expect("Unable to get data directory")
        .join("Zaku/store")
        .with_extension("bin")
});

static ZAKU_STORE: Lazy<RwLock<ZakuStore>> = Lazy::new(|| {
    if STORE_ABSOLUTE_PATH.exists() {
        let mut file = fs::File::open(&*STORE_ABSOLUTE_PATH).expect("Failed to open Store file");
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .expect("Failed to read Store file contents");
        let store_content: ZakuStore = postcard::from_bytes(&buffer).unwrap_or_default();

        RwLock::new(store_content)
    } else {
        RwLock::new(ZakuStore::default())
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
        if let Some(parent) = STORE_ABSOLUTE_PATH.parent() {
            fs::create_dir_all(parent).expect("Failed to create parent directories");
        }

        let store_content = postcard::to_stdvec(self).expect("Failed to serialize store data");

        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&*STORE_ABSOLUTE_PATH)
            .and_then(|mut file| file.write_all(&store_content))
            .expect("Failed to write data");
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
