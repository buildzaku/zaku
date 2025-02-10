use std::path::PathBuf;

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
