use indexmap::IndexMap;
use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

pub fn toggle_devtools(app_handle: &AppHandle) {
    let webview_window = app_handle.get_webview_window("main").unwrap();

    if webview_window.is_devtools_open() {
        webview_window.close_devtools();
    } else {
        webview_window.open_devtools();
    }
}

pub static APP_DATA_DIR: Lazy<PathBuf> = Lazy::new(|| {
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
                let key = if *included { key } else { &format!("!{key}") };

                (key.clone(), value.clone())
            })
            .collect(),
    )
}

pub fn join_strpaths(paths: Vec<&str>) -> String {
    paths
        .into_iter()
        .filter(|path| !path.is_empty())
        .collect::<PathBuf>()
        .to_string_lossy()
        .to_string()
}

pub fn hashed_filename(abspath: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(abspath.as_bytes());

    format!("{:x}", hasher.finalize())
}

/// Sanitizes a segment (directory/file) name to be safe across platforms
///
/// - Converts to lowercase
/// - Replaces invalid characters with `-`
/// - Replaces whitespace with `-`
/// - Trims leading/trailing hyphens
pub fn sanitize_path_segment(segment: &str) -> String {
    const INVALID_CHARS: [char; 8] = ['<', '>', ':', '"', '\\', '|', '?', '*'];

    let mut sanitized = String::new();

    for char in segment.to_lowercase().chars() {
        if char.is_whitespace() || INVALID_CHARS.contains(&char) {
            if !sanitized.ends_with('-') {
                sanitized.push('-');
            }
        } else {
            sanitized.push(char);
        }
    }

    sanitized.trim_matches('-').to_string()
}
