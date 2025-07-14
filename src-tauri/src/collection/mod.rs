use std::fs;
use std::path::Path;
use std::{collections::HashMap, path::PathBuf};
use toml;

pub mod models;

#[cfg(test)]
pub mod tests;

use crate::space;
use crate::{
    collection::models::{CreateCollectionDto, CreateNewCollection},
    error::{Error, Result},
    state::SharedState,
    utils,
};

/// Reads the collection display names from `.zaku/collections/display_name.toml`
///
/// If the file doesn't exist, it creates a new one and returns an empty map. Used to
/// map sanitized relpaths back to their original UI display names
///
/// - `space_abspath`: Absolute path of space
///
/// Returns a `Result` containing the collection's relpath-to-display-name map
pub fn colname_by_relpath(space_abspath: &Path) -> Result<HashMap<String, String>> {
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

/// Saves the collection's display name in `.zaku/collections/display_name.toml` if
/// it doesn't exist already
///
/// This helps preserve the original casing and formatting for UI, while allowing
/// sanitized versions to be used as actual directory names
///
/// - `space_abspath`: Absolute path of space
/// - `collection_relpath`: Path relative to space where the collection resides
/// - `collection_display_name`: Original name to display on UI
///
/// Returns a `Result` indicating success or failure
pub fn save_displayname_if_missing(
    space_abspath: &Path,
    collection_relpath: &str,
    collection_display_name: &str,
) -> Result<()> {
    let displayname_file_abspath = space_abspath.join(".zaku/collections/display_name.toml");

    let mut collection_name_by_relpath = colname_by_relpath(space_abspath)?;

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
/// Directories are created under `space_abspath/parent_relpath`
///
/// - `space_abspath`: Absolute path of space
/// - `create_collection_dto`: Contains `parent_relpath` and `relpath`
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
        let dir_sanitized_name = utils::sanitize_path_segment(dir_display_name);

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

/// Creates new collection directory/directories under the active space
///
/// If the collection path contains nested segments (e.g. `"Settings/Notifications"`),
/// it creates all parent directories as needed and stores each segment's original
/// name as display name.
///
/// - `dto`: Contains `parent_relpath` and `relpath` of the new collection from space root
/// - `sharedstate`: Shared state of the app
///
/// Returns a `Result` containing the newly created collection's metadata
pub fn create_collection(
    dto: &CreateCollectionDto,
    sharedstate: &mut SharedState,
) -> Result<CreateNewCollection> {
    if dto.relpath.trim().is_empty() {
        return Err(Error::FileNotFound(
            "Cannot create a collection without name".to_string(),
        ));
    }

    let active_space = sharedstate
        .active_space
        .clone()
        .ok_or_else(|| Error::FileNotFound("Active space not found".to_string()))?;

    let active_space_abspath = PathBuf::from(&active_space.abspath);

    let (parsed_parent_relpath, dir_display_name) = match dto.relpath.rfind('/') {
        Some(last_slash_index) => {
            let parsed_parent_relpath = &dto.relpath[..last_slash_index];
            let dir_display_name = &dto.relpath[last_slash_index + 1..];

            (
                Some(parsed_parent_relpath.to_string()),
                dir_display_name.to_string(),
            )
        }
        None => (None, dto.relpath.clone()),
    };

    let dir_display_name = dir_display_name.trim();
    let dir_sanitized_name = dir_display_name
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join("-");

    let (dir_parent_relpath, dir_sanitized_name) = match parsed_parent_relpath {
        Some(ref parsed_parent_relpath) => {
            let dto = CreateCollectionDto {
                parent_relpath: dto.parent_relpath.clone(),
                relpath: parsed_parent_relpath.to_string(),
            };

            let dirs_sanitized_relpath = create_collections_all(&active_space_abspath, &dto)?;

            let dir_parent_relpath = utils::join_str_paths(vec![
                dto.parent_relpath.as_str(),
                dirs_sanitized_relpath.as_str(),
            ]);

            (dir_parent_relpath, dir_sanitized_name)
        }
        None => (dto.parent_relpath.clone(), dir_sanitized_name),
    };

    let dir_abspath = active_space_abspath
        .join(&dir_parent_relpath)
        .join(&dir_sanitized_name);
    let dir_relpath = utils::join_str_paths(vec![
        dir_parent_relpath.as_str(),
        dir_sanitized_name.as_str(),
    ]);

    fs::create_dir(&dir_abspath)?;

    save_displayname_if_missing(&active_space_abspath, &dir_relpath, dir_display_name)?;

    let create_new_collection = CreateNewCollection {
        parent_relpath: dir_parent_relpath,
        relpath: dir_relpath,
    };

    sharedstate.active_space = Some(space::parse_space(&active_space_abspath)?);

    Ok(create_new_collection)
}
