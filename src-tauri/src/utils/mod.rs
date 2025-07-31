use indexmap::IndexMap;
use sha2::{Digest, Sha256};
use std::path::{self, Path, PathBuf};
use tauri::{AppHandle, Manager};

use crate::{
    error::{Error, Result},
    models::SanitizedSegment,
};

#[cfg(test)]
pub mod tests;

pub fn toggle_devtools(app_handle: &AppHandle) {
    let webview_window = app_handle.get_webview_window("main").unwrap();

    if webview_window.is_devtools_open() {
        webview_window.close_devtools();
    } else {
        webview_window.open_devtools();
    }
}

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

pub fn hashed_filename(abspath: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(abspath.as_os_str().as_encoded_bytes());

    format!("{:x}", hasher.finalize())
}

pub fn to_fsname(name: &str) -> Result<String> {
    const WINDOWS_RESERVED: [&str; 22] = [
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
        "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    ];

    let sanitized = name
        .to_lowercase()
        .chars()
        .map(|char| {
            if char.is_alphabetic() || char.is_ascii_digit() {
                char
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");

    if sanitized.is_empty() {
        return Err(Error::SanitizationError(
            "Empty name after sanitization".to_string(),
        ));
    }

    if WINDOWS_RESERVED.contains(&sanitized.as_str()) {
        return Err(Error::SanitizationError(format!(
            "Reserved name not allowed: {sanitized}"
        )));
    }

    Ok(sanitized)
}

pub fn to_sanitized_segments(relpath: &Path) -> Result<Vec<SanitizedSegment>> {
    let relpath_str = relpath.to_string_lossy();
    let relpath_no_bslashes = relpath_str.replace('\\', "-");
    let mut segments = Vec::new();

    for component in PathBuf::from(&relpath_no_bslashes).components() {
        if let path::Component::Normal(os_str) = component {
            let name = os_str.to_string_lossy().trim().to_string();
            if !name.is_empty() {
                let fsname = to_fsname(&name)?;
                segments.push(SanitizedSegment { name, fsname });
            }
        }
    }

    Ok(segments)
}
