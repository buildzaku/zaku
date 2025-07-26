use std::path::{Path, PathBuf};

use crate::utils;

#[cfg(test)]
use crate::{space, space::models::CreateSpaceDto, store::StateStore};

pub const SPACES_STORE_FSNAME: &str = "spaces";

/// Returns `StateStore`'s absolute path on the filesystem.
pub fn state_store_abspath(datadir_abspath: &Path) -> PathBuf {
    datadir_abspath.join("state.json")
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

/// Returns `SpaceCollectionsMetadataStore`'s absolute path on the filesystem for a given space.
pub fn scmt_store_abspath(space_abspath: &Path) -> PathBuf {
    space_abspath
        .join(".zaku")
        .join("collections")
        .join("name.toml")
}

#[cfg(test)]
/// Creates a temporary space for testing purposes.
/// Returns the temp directories and state store
pub fn temp_space(space_name: &str) -> (tempfile::TempDir, tempfile::TempDir, StateStore) {
    let tmp_datadir = tempfile::tempdir().unwrap();
    let tmp_spacedir = tempfile::tempdir().unwrap();

    let state_store_abspath = state_store_abspath(tmp_datadir.path());
    let dto = CreateSpaceDto {
        name: space_name.to_string(),
        location: tmp_spacedir.path().to_string_lossy().to_string(),
    };
    let mut state_store =
        StateStore::get(&state_store_abspath).expect("Failed to init state store");

    space::create_space(dto, &mut state_store).expect("Failed to create test space");

    (tmp_datadir, tmp_spacedir, state_store)
}
