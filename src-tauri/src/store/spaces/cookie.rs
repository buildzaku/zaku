use cookie_store::{
    serde::json::{load as read_cookie_json, save as write_cookie_json},
    Cookie,
};
use once_cell::sync::Lazy;
use reqwest_cookie_store::{CookieStore, CookieStoreMutex};
use std::{
    collections::HashMap,
    fs,
    io::{BufReader, BufWriter},
    sync::{Arc, Mutex},
};

use crate::{
    utils::{hashed_filename, ZAKU_DATA_DIR},
    space::models::RemoveCookieDto,
    store::models::SpaceCookies,
};

type CookiesCache = Mutex<HashMap<String, Arc<CookieStoreMutex>>>;

static COOKIES_CACHE: Lazy<CookiesCache> = Lazy::new(|| Mutex::new(HashMap::new()));

impl SpaceCookies {
    pub fn load(space_abspath: &str) -> Arc<CookieStoreMutex> {
        let hsh_space_abspath = hashed_filename(space_abspath);

        let mut cache = COOKIES_CACHE.lock().expect("Failed to lock cookie cache");
        if let Some(space_cookies) = cache.get(space_abspath) {
            return Arc::clone(space_cookies);
        }

        let cookie_file = ZAKU_DATA_DIR
            .join(super::SPACES_STORE_DIR)
            .join(&hsh_space_abspath)
            .join("cookies.json");
        let space_cookiestore = if cookie_file.exists() {
            let file = fs::File::open(&cookie_file)
                .map(BufReader::new)
                .expect("Failed to open cookie file");
            read_cookie_json(file).unwrap_or_else(|_| CookieStore::default())
        } else {
            CookieStore::default()
        };

        let space_cookies = Arc::new(CookieStoreMutex::new(space_cookiestore));
        cache.insert(space_abspath.to_string(), Arc::clone(&space_cookies));

        return space_cookies;
    }

    pub fn persist(space_abspath: &str) {
        let hsh_space_abspath = hashed_filename(space_abspath);

        let cache = COOKIES_CACHE.lock().expect("Failed to lock cookie cache");
        if let Some(space_cookies) = cache.get(space_abspath) {
            let cookie_file = ZAKU_DATA_DIR
                .join(super::SPACES_STORE_DIR)
                .join(&hsh_space_abspath)
                .join("cookies.json");

            if let Some(parent) = cookie_file.parent() {
                fs::create_dir_all(parent).expect("Failed to create cookie directory");
            }

            let mut writer = fs::File::create(&cookie_file)
                .map(BufWriter::new)
                .expect("Failed to create cookie file");

            let locked = space_cookies.lock().unwrap();
            write_cookie_json(&*locked, &mut writer).expect("Failed to persist cookie store");
        }
    }

    pub fn clear(space_abspath: &str) {
        let space_cookies = SpaceCookies::load(space_abspath);
        {
            let mut locked = space_cookies.lock().unwrap();
            locked.clear();
        }
        SpaceCookies::persist(space_abspath);
    }

    pub fn remove(space_abspath: &str, rm_cookie_dto: RemoveCookieDto) -> Option<Cookie<'static>> {
        let RemoveCookieDto { domain, path, name } = rm_cookie_dto;
        let space_cookies = SpaceCookies::load(space_abspath);
        let removed = {
            let mut locked = space_cookies.lock().unwrap();

            locked.remove(&domain, &path, &name)
        };
        SpaceCookies::persist(space_abspath);

        return removed;
    }
}
