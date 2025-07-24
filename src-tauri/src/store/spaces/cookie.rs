use cookie_store::serde::json as cookie_json;
use reqwest_cookie_store::{CookieStore, CookieStoreMutex};
use std::{
    fs,
    io::{BufReader, BufWriter},
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::error::{Error, Result};

pub struct SpaceCookieStore {
    pub cookies: Arc<CookieStoreMutex>,

    abspath: PathBuf,
}

impl SpaceCookieStore {
    fn new(sck_store_abspath: PathBuf, cookies: Arc<CookieStoreMutex>) -> Self {
        Self {
            cookies,
            abspath: sck_store_abspath,
        }
    }

    fn init(sck_store_abspath: &Path) -> Result<SpaceCookieStore> {
        if !sck_store_abspath.exists() {
            let default_cookies = Arc::new(CookieStoreMutex::new(CookieStore::default()));
            let sck_store = Self::new(sck_store_abspath.to_path_buf(), default_cookies);
            sck_store.fswrite()?;

            return Ok(sck_store);
        }

        let file = fs::File::open(sck_store_abspath).map(BufReader::new)?;

        match cookie_json::load(file) {
            Ok(cookie_store) => {
                let cookies = Arc::new(CookieStoreMutex::new(cookie_store));

                Ok(Self::new(sck_store_abspath.to_path_buf(), cookies))
            }
            Err(_) => {
                let default_cookies = Arc::new(CookieStoreMutex::new(CookieStore::default()));
                let sck_store = Self::new(sck_store_abspath.to_path_buf(), default_cookies);
                sck_store.fswrite()?;

                Ok(sck_store)
            }
        }
    }

    fn fswrite(&self) -> Result<()> {
        if let Some(parent) = self.abspath.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut writer = fs::File::create(&self.abspath).map(BufWriter::new)?;
        let sck_store_mtx = self
            .cookies
            .lock()
            .map_err(|_| Error::LockError("Failed to lock cookie store".into()))?;

        cookie_json::save(&sck_store_mtx, &mut writer)?;

        Ok(())
    }

    pub fn get(sck_store_abspath: &Path) -> Result<SpaceCookieStore> {
        Self::init(sck_store_abspath)
    }

    pub fn update<F>(&mut self, mutator: F) -> Result<()>
    where
        F: FnOnce(&Arc<CookieStoreMutex>),
    {
        mutator(&self.cookies);
        self.fswrite()
    }
}
