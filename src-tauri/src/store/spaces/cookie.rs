use cookie_store::{serde::json as cookie_json, Cookie};
use once_cell::sync::Lazy;
use reqwest_cookie_store::{CookieStore, CookieStoreMutex};
use std::{
    collections::HashMap,
    fs,
    io::{BufReader, BufWriter},
    sync::{Arc, Mutex},
};

use crate::{
    error::{Error, Result},
    space::models::RemoveCookieDto,
    store::models::SpaceCookies,
    utils::{hashed_filename, ZAKU_DATA_DIR},
};

type CookiesCache = Mutex<HashMap<String, Arc<CookieStoreMutex>>>;

static COOKIES_CACHE: Lazy<CookiesCache> = Lazy::new(|| Mutex::new(HashMap::new()));

impl SpaceCookies {
    pub fn load(space_abspath: &str) -> Result<Arc<CookieStoreMutex>> {
        let hsh_space_abspath = hashed_filename(space_abspath);

        let mut cache = COOKIES_CACHE
            .lock()
            .map_err(|_| Error::FileReadError("Failed to lock cookie cache".into()))?;

        if let Some(space_cookies) = cache.get(space_abspath) {
            return Ok(Arc::clone(space_cookies));
        }

        let cookie_file = ZAKU_DATA_DIR
            .join(super::SPACES_STORE_DIR)
            .join(&hsh_space_abspath)
            .join("cookies.json");

        let space_cookiestore = if cookie_file.exists() {
            let file = fs::File::open(&cookie_file).map(BufReader::new)?;
            cookie_json::load(file).unwrap_or_else(|_| CookieStore::default())
        } else {
            CookieStore::default()
        };

        let space_cookies = Arc::new(CookieStoreMutex::new(space_cookiestore));
        cache.insert(space_abspath.to_string(), Arc::clone(&space_cookies));

        Ok(space_cookies)
    }

    pub fn persist(space_abspath: &str) -> Result<()> {
        let hsh_space_abspath = hashed_filename(space_abspath);

        let cache = COOKIES_CACHE
            .lock()
            .map_err(|_| Error::LockError("Failed to lock cookie cache".into()))?;

        if let Some(space_cookies) = cache.get(space_abspath) {
            let cookie_file = ZAKU_DATA_DIR
                .join(super::SPACES_STORE_DIR)
                .join(&hsh_space_abspath)
                .join("cookies.json");

            if let Some(parent) = cookie_file.parent() {
                fs::create_dir_all(parent)?;
            }

            let mut writer = fs::File::create(&cookie_file).map(BufWriter::new)?;
            let locked = space_cookies
                .lock()
                .map_err(|_| Error::LockError("Failed to lock cookie store".into()))?;

            cookie_json::save(&*locked, &mut writer)?;
        }

        Ok(())
    }

    pub fn clear(space_abspath: &str) -> Result<()> {
        let space_cookies = SpaceCookies::load(space_abspath)?;

        {
            let mut locked = space_cookies
                .lock()
                .map_err(|_| Error::LockError("Failed to lock cookie store".into()))?;
            locked.clear();
        }

        SpaceCookies::persist(space_abspath)?;

        Ok(())
    }

    pub fn remove(
        space_abspath: &str,
        rm_cookie_dto: RemoveCookieDto,
    ) -> Result<Option<Cookie<'static>>> {
        let RemoveCookieDto { domain, path, name } = rm_cookie_dto;
        let space_cookies = SpaceCookies::load(space_abspath)?;

        let removed = {
            let mut locked = space_cookies
                .lock()
                .map_err(|_| Error::LockError("Failed to lock cookie store".into()))?;
            locked.remove(&domain, &path, &name)
        };

        SpaceCookies::persist(space_abspath)?;

        Ok(removed)
    }
}
