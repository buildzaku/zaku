use std::path::PathBuf;

use indexmap::IndexMap;
use once_cell::sync::Lazy;
use tauri::{AppHandle, Manager};

pub fn toggle_devtools(app_handle: &AppHandle) {
    let webview_window = app_handle.get_webview_window("main").unwrap();

    if webview_window.is_devtools_open() {
        webview_window.close_devtools();
    } else {
        webview_window.open_devtools();
    }
}

pub static ZAKU_DATA_DIR: Lazy<PathBuf> = Lazy::new(|| {
    dirs::data_dir()
        .expect("Unable to get data directory")
        .join("Zaku")
});

pub fn from_indexmap(map: &IndexMap<String, String>) -> Vec<(bool, String, String)> {
    map.iter()
        .map(|(k, v)| {
            let included = !k.starts_with('!');
            let key = if included {
                k.clone()
            } else {
                k[1..].to_string()
            };
            (included, key, v.clone())
        })
        .collect()
}

pub fn to_indexmap(fields: &[(bool, String, String)]) -> Option<IndexMap<String, String>> {
    if fields.is_empty() {
        return None;
    }

    Some(
        fields
            .iter()
            .map(|(included, key, value)| {
                let key = if *included { key } else { &format!("!{}", key) };
                (key.clone(), value.clone())
            })
            .collect(),
    )
}
