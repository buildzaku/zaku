use std::path::{Path, PathBuf};

#[cfg(not(test))]
use std::sync::LazyLock;

#[cfg(test)]
use std::cell::RefCell;

use crate::utils;

#[cfg(test)]
use crate::{space, space::models::CreateSpaceDto, store::StateStore};

pub const SPACES_STORE_FSNAME: &str = "spaces";

#[cfg(not(test))]
pub static DATADIR_ABSPATH: LazyLock<PathBuf> = LazyLock::new(|| {
    dirs::data_dir()
        .expect("Unable to get data directory")
        .join("Zaku")
});

#[cfg(test)]
thread_local! {
    pub static DATADIR_ABSPATH: RefCell<PathBuf> = RefCell::new(PathBuf::from("/tmp"));
}

/// Returns `StateStore`'s absolute path on the filesystem.
pub fn state_store_abspath() -> PathBuf {
    #[cfg(not(test))]
    {
        DATADIR_ABSPATH.join("state.json")
    }

    #[cfg(test)]
    {
        DATADIR_ABSPATH.with(|path| path.borrow().join("state.json"))
    }
}

/// Returns `SpaceBufferStore`'s absolute path on the filesystem for a given space.
pub fn sbf_store_abspath(space_abspath: &Path) -> PathBuf {
    let hsh = utils::hashed_filename(space_abspath);

    #[cfg(not(test))]
    {
        DATADIR_ABSPATH
            .join(SPACES_STORE_FSNAME)
            .join(hsh)
            .join("buffer.json")
    }

    #[cfg(test)]
    {
        DATADIR_ABSPATH.with(|path| {
            path.borrow()
                .join(SPACES_STORE_FSNAME)
                .join(hsh)
                .join("buffer.json")
        })
    }
}

/// Returns `SpaceCookieStore`'s absolute path on the filesystem for a given space.
pub fn sck_store_abspath(space_abspath: &Path) -> PathBuf {
    let hsh = utils::hashed_filename(space_abspath);

    #[cfg(not(test))]
    {
        DATADIR_ABSPATH
            .join(SPACES_STORE_FSNAME)
            .join(hsh)
            .join("cookies.json")
    }

    #[cfg(test)]
    {
        DATADIR_ABSPATH.with(|path| {
            path.borrow()
                .join(SPACES_STORE_FSNAME)
                .join(hsh)
                .join("cookies.json")
        })
    }
}

/// Returns `SpaceSettingsStore`'s absolute path on the filesystem for a given space.
pub fn sst_store_abspath(space_abspath: &Path) -> PathBuf {
    let hsh = utils::hashed_filename(space_abspath);

    #[cfg(not(test))]
    {
        DATADIR_ABSPATH
            .join(SPACES_STORE_FSNAME)
            .join(hsh)
            .join("settings.json")
    }

    #[cfg(test)]
    {
        DATADIR_ABSPATH.with(|path| {
            path.borrow()
                .join(SPACES_STORE_FSNAME)
                .join(hsh)
                .join("settings.json")
        })
    }
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

    DATADIR_ABSPATH.with(|dir| {
        *dir.borrow_mut() = tmp_datadir.path().to_path_buf();
    });

    let dto = CreateSpaceDto {
        name: space_name.to_string(),
        location: tmp_spacedir.path().to_path_buf(),
    };
    let mut state_store = StateStore::get().expect("Failed to init state store");

    space::create_space(dto, &mut state_store).expect("Failed to create test space");

    (tmp_datadir, tmp_spacedir, state_store)
}
