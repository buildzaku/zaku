use indexmap::IndexMap;
use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};
use tauri::{AppHandle, Manager};

use crate::models::SanitizedSegment;

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

pub fn sanitize_name(name: &str) -> String {
    const INVALID_CHARS: [char; 8] = ['<', '>', ':', '"', '\\', '|', '?', '*'];

    name.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_whitespace() || INVALID_CHARS.contains(&c) {
                '-'
            } else {
                c
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

pub fn rm_backslash(name: &str) -> String {
    name.replace('\\', "-")
}

pub fn to_sanitized_segments(relpath: &str) -> Vec<SanitizedSegment> {
    let relpath_no_bslashes = rm_backslash(relpath);
    let mut segments = Vec::new();

    for component in Path::new(&relpath_no_bslashes).components() {
        if let Component::Normal(os_str) = component {
            let name = os_str.to_string_lossy().trim().to_string();
            if !name.is_empty() {
                let fsname = sanitize_name(&name);

                if !fsname.is_empty() {
                    segments.push(SanitizedSegment { name, fsname });
                }
            }
        }
    }

    segments
}
