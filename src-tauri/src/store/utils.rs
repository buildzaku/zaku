use std::path::PathBuf;
use std::sync::OnceLock;

#[cfg(not(test))]
use dirs;

#[cfg(test)]
use tempfile;

#[cfg(test)]
static TEST_DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

#[cfg(not(test))]
static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

#[cfg(test)]
pub fn datadir_abspath() -> PathBuf {
    TEST_DATA_DIR
        .get_or_init(|| tempfile::tempdir().unwrap().path().join("Zaku"))
        .clone()
}

#[cfg(not(test))]
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
