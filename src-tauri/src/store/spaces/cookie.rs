use cookie_store::serde::json as cookie_json;
use reqwest_cookie_store::{CookieStore, CookieStoreMutex};
use std::{
    fs,
    io::{BufReader, BufWriter},
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    error::{Error, Result},
    store::{self},
    utils,
};

pub struct SpaceCookies;

impl SpaceCookies {
    fn filename() -> &'static str {
        "cookies.json"
    }

    pub fn filepath(space_abspath: &Path) -> PathBuf {
        let hsh = utils::hashed_filename(space_abspath);

        store::utils::datadir_abspath()
            .join(store::utils::SPACES_STORE_FSNAME)
            .join(hsh)
            .join(Self::filename())
    }

    fn init(space_abspath: &Path) -> Result<Arc<CookieStoreMutex>> {
        let cookie_filepath = Self::filepath(space_abspath);
        if !cookie_filepath.exists() {
            let default_cookies = Arc::new(CookieStoreMutex::new(CookieStore::default()));
            Self::fswrite(space_abspath, &default_cookies)?;

            return Ok(default_cookies);
        }

        let file = fs::File::open(&cookie_filepath).map(BufReader::new)?;

        match cookie_json::load(file) {
            Ok(cookie_store) => Ok(Arc::new(CookieStoreMutex::new(cookie_store))),
            Err(_) => {
                // corrupt JSON, use default
                let default_cookies = Arc::new(CookieStoreMutex::new(CookieStore::default()));
                Self::fswrite(space_abspath, &default_cookies)?;

                Ok(default_cookies)
            }
        }
    }

    fn fswrite(space_abspath: &Path, cookies: &Arc<CookieStoreMutex>) -> Result<()> {
        let cookie_filepath = Self::filepath(space_abspath);

        if let Some(parent) = cookie_filepath.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut writer = fs::File::create(&cookie_filepath).map(BufWriter::new)?;
        let locked = cookies
            .lock()
            .map_err(|_| Error::LockError("Failed to lock cookie store".into()))?;

        cookie_json::save(&locked, &mut writer)?;

        Ok(())
    }

    pub fn get(space_abspath: &Path) -> Result<Arc<CookieStoreMutex>> {
        Self::init(space_abspath)
    }

    pub fn update<F>(space_abspath: &Path, mutator: F) -> Result<Arc<CookieStoreMutex>>
    where
        F: FnOnce(&Arc<CookieStoreMutex>),
    {
        let space_cookies = Self::get(space_abspath)?;
        mutator(&space_cookies);
        Self::fswrite(space_abspath, &space_cookies)?;

        Ok(space_cookies)
    }
}
