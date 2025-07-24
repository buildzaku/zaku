use dirs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::utils;

#[cfg(test)]
use crate::{
    space,
    space::models::{CreateSpaceDto, SpaceReference},
    state::SharedState,
    store::Store,
    store::UserSettingsStore,
};

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Returns the absolute path to the application's data directory.
pub fn datadir_abspath() -> PathBuf {
    DATA_DIR
        .get_or_init(|| {
            dirs::data_dir()
                .expect("Unable to get data directory")
                .join("Zaku")
        })
        .clone()
}

pub const SPACES_STORE_FSNAME: &str = "spaces";

/// Returns `Store`'s absolute path on the filesystem.
pub fn store_abspath(datadir_abspath: &Path) -> PathBuf {
    datadir_abspath.join("store.json")
}

/// Returns `UserSettingsStore`'s absolute path on the filesystem.
pub fn ust_store_abspath(datadir_abspath: &Path) -> PathBuf {
    datadir_abspath.join("user").join("settings.json")
}

/// Returns `SpaceBufferStore`'s absolute path on the filesystem for a given space.
pub fn sbf_store_abspath(datadir_abspath: &Path, space_abspath: &Path) -> PathBuf {
    let hsh = utils::hashed_filename(space_abspath);

    datadir_abspath
        .join(SPACES_STORE_FSNAME)
        .join(hsh)
        .join("buffer.json")
}

/// Returns `SpaceCookieStore`'s absolute path on the filesystem for a given space.
pub fn sck_store_abspath(datadir_abspath: &Path, space_abspath: &Path) -> PathBuf {
    let hsh = utils::hashed_filename(space_abspath);

    datadir_abspath
        .join(SPACES_STORE_FSNAME)
        .join(hsh)
        .join("cookies.json")
}

/// Returns `SpaceSettingsStore`'s absolute path on the filesystem for a given space.
pub fn sst_store_abspath(datadir_abspath: &Path, space_abspath: &Path) -> PathBuf {
    let hsh = utils::hashed_filename(space_abspath);

    datadir_abspath
        .join(SPACES_STORE_FSNAME)
        .join(hsh)
        .join("settings.json")
}

#[cfg(test)]
/// Creates a temporary space for testing purposes.
/// Returns the shared state, store, and space reference.
pub fn tmp_space(name: &str, tmp_path: &Path) -> (SharedState, Store, SpaceReference) {
    let ust_store_abspath = ust_store_abspath(tmp_path);
    let store_abspath = tmp_path.join("store.json");

    let dto = CreateSpaceDto {
        name: name.to_string(),
        location: tmp_path.to_string_lossy().to_string(),
    };

    let mut sharedstate = SharedState {
        space: None,
        spacerefs: Vec::new(),
        user_settings: UserSettingsStore::get(&ust_store_abspath)
            .expect("Failed to init user settings")
            .into_inner(),
    };

    let mut store = Store::get(&store_abspath).expect("Failed to init store");
    let space_ref = space::create_space(dto, &mut sharedstate, &mut store)
        .expect("Failed to create test space");

    (sharedstate, store, space_ref)
}
