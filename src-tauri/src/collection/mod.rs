use std::collections::HashMap;
use std::fs;
use std::path::Path;
use toml;

pub mod models;

use crate::{
    collection::models::CreateCollectionDto,
    error::{Error, Result},
    utils,
};

pub fn displayname_by_relpath(space_abspath: &Path) -> Result<HashMap<String, String>> {
    let displayname_file_abspath = space_abspath.join(".zaku/collections/display_name.toml");

    let content = match fs::read_to_string(&displayname_file_abspath) {
        Ok(content) => content,
        Err(_) => {
            let empty_map: HashMap<String, String> = HashMap::new();

            if let Some(parent) = displayname_file_abspath.parent() {
                fs::create_dir_all(parent)?;
            }

            let serialized = toml::to_string_pretty(&empty_map)?;
            fs::write(&displayname_file_abspath, &serialized)?;
            serialized
        }
    };

    Ok(toml::from_str(&content)?)
}

pub fn save_displayname_if_missing(
    space_abspath: &Path,
    collection_relpath: &str,
    collection_display_name: &str,
) -> Result<()> {
    let displayname_file_abspath = space_abspath.join(".zaku/collections/display_name.toml");

    let mut collection_name_by_relpath = displayname_by_relpath(space_abspath)?;

    collection_name_by_relpath
        .entry(collection_relpath.to_string())
        .or_insert_with(|| collection_display_name.to_string());

    let toml_content = toml::to_string_pretty(&collection_name_by_relpath)?;

    fs::write(&displayname_file_abspath, toml_content)?;

    Ok(())
}

/// Creates a collection directory (nested if needed) based on `relpath`
/// under the specified `parent_relpath`. Each segment is sanitized for
/// the filesystem and the original segment is saved as display name
///
/// Example, if `relpath` is `"Settings/Notifications"`, it creates:
/// - Directories: `settings/notifications`
/// - Display names saved:
///   - `settings` -> `"Settings"`
///   - `notifications` -> `"Notifications"`
///
/// Directories are created under `space_abspath/parent_relpath`.
///
/// - `space_abspath`: Absolute path of space.
/// - `create_collection_dto`: Contains `parent_relpath` and `relpath`.
///
/// Returns a `Result`  containing the created collection's relative path
pub fn create_collections_all(
    space_abspath: &Path,
    create_collection_dto: &CreateCollectionDto,
) -> Result<String> {
    if create_collection_dto.relpath.trim().is_empty() {
        return Err(Error::FileNotFound("Collection name is missing".into()));
    }

    let mut dirs = Vec::new();
    for dir_display_name in create_collection_dto.relpath.split('/') {
        let dir_display_name = dir_display_name.trim();
        let dir_sanitized_name = dir_display_name
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join("-");

        if dir_display_name.is_empty() || dir_sanitized_name.is_empty() {
            continue;
        }

        dirs.push((dir_sanitized_name, dir_display_name.to_string()));
    }

    let collection_parent_abspath = space_abspath.join(&create_collection_dto.parent_relpath);
    let mut collections_relpath = String::new();

    for (dir_sanitized_name, dir_display_name) in &dirs {
        let mut cur_collection_relpath = collections_relpath.clone();

        if !cur_collection_relpath.is_empty() {
            cur_collection_relpath.push('/');
        }
        cur_collection_relpath.push_str(dir_sanitized_name);

        let target_dir = collection_parent_abspath.join(&cur_collection_relpath);
        let dir_exists = fs::metadata(&target_dir).is_ok();
        if !dir_exists {
            fs::create_dir(&target_dir)?;
        };

        let cur_collection_relpath = utils::join_str_paths(vec![
            &create_collection_dto.parent_relpath,
            &cur_collection_relpath,
        ]);

        save_displayname_if_missing(space_abspath, &cur_collection_relpath, dir_display_name)
            .map_err(|e| Error::FileReadError(format!("{cur_collection_relpath}: {e}")))?;

        if !collections_relpath.is_empty() {
            collections_relpath.push('/');
        }
        collections_relpath.push_str(dir_sanitized_name);
    }

    Ok(collections_relpath)
}
