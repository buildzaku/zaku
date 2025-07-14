use std::fs;
use std::path::Path;
use std::{collections::HashMap, path::PathBuf};
use toml;

pub mod models;

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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile;

    use crate::space::models::CreateSpaceDto;

    #[test]
    fn displayname_by_relpath_reads_existing_data() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let space_abspath = tmp_dir.path();

        let displayname_path = space_abspath.join(".zaku/collections/display_name.toml");
        fs::create_dir_all(displayname_path.parent().unwrap()).unwrap();
        fs::write(&displayname_path, r#""demo/path" = "Demo Path""#).unwrap();

        let map = displayname_by_relpath(space_abspath).unwrap();
        assert_eq!(map.get("demo/path"), Some(&"Demo Path".into()));
    }

    #[test]
    fn displayname_by_relpath_invalid_toml_should_fail() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let space_abspath = tmp_dir.path();

        let path = space_abspath.join(".zaku/collections/display_name.toml");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "not = [valid").unwrap();

        let result = displayname_by_relpath(space_abspath);
        assert!(result.is_err());
    }

    #[test]
    fn displayname_by_relpath_creates_file_if_missing() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let space_abspath = tmp_dir.path();

        let result = displayname_by_relpath(space_abspath).unwrap();
        assert!(result.is_empty());

        let file_path = space_abspath.join(".zaku/collections/display_name.toml");
        assert!(file_path.exists());

        let content = fs::read_to_string(file_path).unwrap();
        assert!(content.contains("{}") || content.trim().is_empty());
    }

    #[test]
    fn save_displayname_if_missing_writes_new_entry() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let space_abspath = tmp_dir.path();

        save_displayname_if_missing(space_abspath, "config/settings", "Config Settings").unwrap();

        let map = displayname_by_relpath(space_abspath).unwrap();
        assert_eq!(map.get("config/settings"), Some(&"Config Settings".into()));
    }

    #[test]
    fn save_displayname_if_missing_does_not_overwrite_existing() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let space_abspath = tmp_dir.path();

        save_displayname_if_missing(space_abspath, "a/b", "Alpha").unwrap();
        save_displayname_if_missing(space_abspath, "a/b", "Beta").unwrap();

        let map = displayname_by_relpath(space_abspath).unwrap();
        assert_eq!(map.get("a/b"), Some(&"Alpha".into()));
    }

    #[test]
    fn create_collections_all_basic() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let space_abspath = tmp_dir.path();
        let dto = CreateCollectionDto {
            parent_relpath: "admin".into(),
            relpath: "Users/Settings/Notifications".into(),
        };

        let col_abspath = space_abspath.join("admin");
        fs::create_dir_all(&col_abspath).unwrap();

        let result = create_collections_all(space_abspath, &dto).unwrap();
        assert_eq!(result, "users/settings/notifications");

        let expected_path = col_abspath.join("users/settings/notifications");
        assert!(expected_path.exists());
    }

    #[test]
    fn create_collections_all_empty_relpath() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let space_abspath = tmp_dir.path();
        let dto = CreateCollectionDto {
            parent_relpath: "auth".into(),
            relpath: "   ".into(),
        };

        let result = create_collections_all(space_abspath, &dto);
        assert!(matches!(result, Err(Error::FileNotFound(_))));
    }

    #[test]
    fn create_collections_all_sanitization() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let space_abspath = tmp_dir.path();
        let dto = CreateCollectionDto {
            parent_relpath: "users".into(),
            relpath: "Notification Settings/List notifications".into(),
        };

        fs::create_dir_all(space_abspath.join("users")).unwrap();

        let col_relpath = create_collections_all(space_abspath, &dto).unwrap();
        assert_eq!(col_relpath, "notification-settings/list-notifications");

        assert!(space_abspath
            .join("users/notification-settings/list-notifications")
            .exists());
    }

    #[test]
    fn create_collections_all_parent_folder_missing_should_fail() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let space_abspath = tmp_dir.path();

        let dto = CreateCollectionDto {
            parent_relpath: "admin/settings".into(),
            relpath: "Preferences/Privacy".into(),
        };

        let result = create_collections_all(space_abspath, &dto);
        assert!(
            result.is_err(),
            "Expected failure due to missing parent folder"
        );

        if let Err(Error::Io(err)) = result {
            assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
        } else {
            panic!("Expected Io::NotFound error, got: {result:?}");
        }
    }

    #[test]
    fn create_collections_all_relpath_with_whitespace_segments_should_skip() {
        let tmp = tempfile::tempdir().unwrap();
        let space_abspath = tmp.path();
        std::fs::create_dir_all(space_abspath.join("admin")).unwrap();

        let dto = CreateCollectionDto {
            parent_relpath: "admin".into(),
            relpath: "  /Notifications       /   ".into(),
        };

        let result = create_collections_all(space_abspath, &dto).unwrap();
        assert_eq!(result, "notifications");

        assert!(space_abspath.join("admin/notifications").exists());
    }

    #[test]
    fn create_collections_all_relpath_with_multiple_slashes_should_be_handled() {
        let tmp = tempfile::tempdir().unwrap();
        let space_abspath = tmp.path();
        std::fs::create_dir_all(space_abspath.join("settings")).unwrap();

        let dto = CreateCollectionDto {
            parent_relpath: "settings".into(),
            relpath: "System///Display".into(),
        };

        let result = create_collections_all(space_abspath, &dto).unwrap();
        assert_eq!(result, "system/display");

        assert!(space_abspath.join("settings/system/display").exists());
    }

    #[test]
    fn create_collections_all_relpath_with_only_empty_segments_should_return_error() {
        let tmp = tempfile::tempdir().unwrap();
        let space_abspath = tmp.path();
        std::fs::create_dir_all(space_abspath.join("posts")).unwrap();

        let dto = CreateCollectionDto {
            parent_relpath: "posts".into(),
            relpath: "   /   /   ".into(),
        };

        let result = create_collections_all(space_abspath, &dto);
        assert!(matches!(result, Ok(p) if p.is_empty()));
    }

    #[test]
    fn create_collections_all_duplicate_create_collections_should_not_fail() {
        let tmp = tempfile::tempdir().unwrap();
        let space_abspath = tmp.path();
        std::fs::create_dir_all(space_abspath.join("workspace")).unwrap();

        let dto = CreateCollectionDto {
            parent_relpath: "workspace".into(),
            relpath: "Config/Options".into(),
        };

        let _ = create_collections_all(space_abspath, &dto).unwrap();
        let result = create_collections_all(space_abspath, &dto).unwrap();

        assert_eq!(result, "config/options");
    }

    #[test]
    fn create_collections_all_special_characters_should_be_sanitized_or_preserved() {
        let tmp = tempfile::tempdir().unwrap();
        let space_abspath = tmp.path();
        std::fs::create_dir_all(space_abspath.join("library")).unwrap();

        let dto = CreateCollectionDto {
            parent_relpath: "library".into(),
            relpath: "Config@Home/Naïve#Settings/🔥 Experimental".into(),
        };

        let result = create_collections_all(space_abspath, &dto).unwrap();

        assert_eq!(result, "config@home/naïve#settings/🔥-experimental");

        let expected_path =
            space_abspath.join("library/config@home/naïve#settings/🔥-experimental");

        assert!(expected_path.exists());
    }

    #[test]
    fn create_collections_all_unicode_segments_should_be_handled() {
        let tmp = tempfile::tempdir().unwrap();
        let space_abspath = tmp.path();
        std::fs::create_dir_all(space_abspath.join("global")).unwrap();

        let dto = CreateCollectionDto {
            parent_relpath: "global".into(),
            relpath: "ザク/設定".into(),
        };

        let result = create_collections_all(space_abspath, &dto).unwrap();
        assert_eq!(result, "ザク/設定");
        assert!(space_abspath.join("global/ザク/設定").exists());
    }

    #[test]
    fn create_collections_all_trailing_slash_should_be_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let space_abspath = tmp.path();
        std::fs::create_dir_all(space_abspath.join("root")).unwrap();

        let dto = CreateCollectionDto {
            parent_relpath: "root".into(),
            relpath: "Settings/Preferences/".into(),
        };

        let result = create_collections_all(space_abspath, &dto).unwrap();
        assert_eq!(result, "settings/preferences");
        assert!(space_abspath.join("root/settings/preferences").exists());
    }

    #[test]
    fn create_collections_all_invalid_characters_should_be_sanitized() {
        let tmp = tempfile::tempdir().unwrap();
        let space_abspath = tmp.path();
        std::fs::create_dir_all(space_abspath.join("logs")).unwrap();

        let dto = CreateCollectionDto {
            parent_relpath: "logs".into(),
            relpath: r#"Error|Logs/<Critical>?Events:2025*Backup\Archive"Today""#.into(),
        };

        let result = create_collections_all(space_abspath, &dto);
        assert!(result.is_ok());

        let expected = "error-logs/critical--events-2025-backup-archive-today";
        assert_eq!(result.unwrap(), expected);

        let expected_path = space_abspath.join("logs").join(expected);
        assert!(expected_path.exists());
    }

    #[test]
    fn create_collection_basic() {
        let tmp = tempfile::tempdir().unwrap();
        let space_name = "Main Space";
        let space_dirname = "main-space";
        let space_abspath = tmp.path().join(space_dirname);
        let dto = CreateSpaceDto {
            name: space_name.into(),
            location: tmp.path().to_string_lossy().into(),
        };

        let mut sharedstate = SharedState::default();
        let spaceref = space::create_space(dto, &mut sharedstate).unwrap();
        assert_eq!(spaceref.name, space_name);

        fs::create_dir_all(space_abspath.join("admin")).unwrap();

        let collection_dto = CreateCollectionDto {
            parent_relpath: "admin".into(),
            relpath: "Settings/Notifications".into(),
        };

        let result = create_collection(&collection_dto, &mut sharedstate).unwrap();

        assert_eq!(result.relpath, "settings/notifications");
        assert!(space_abspath.join("admin/settings/notifications").exists());
    }

    #[test]
    fn create_collection_empty_relpath_should_fail() {
        let tmp = tempfile::tempdir().unwrap();
        let space_name = "Empty Check";
        let space_dirname = "empty-check";
        let space_abspath = tmp.path().join(space_dirname);

        let dto = CreateSpaceDto {
            name: space_name.into(),
            location: tmp.path().to_string_lossy().into(),
        };

        let mut sharedstate = SharedState::default();
        let _ = space::create_space(dto, &mut sharedstate).unwrap();

        fs::create_dir_all(space_abspath.join("admin")).unwrap();

        let collection_dto = CreateCollectionDto {
            parent_relpath: "admin".into(),
            relpath: "   ".into(),
        };

        let result = create_collection(&collection_dto, &mut sharedstate);
        assert!(matches!(result, Err(Error::FileNotFound(_))));
    }

    #[test]
    fn create_collection_missing_active_space_should_fail() {
        let collection_dto = CreateCollectionDto {
            parent_relpath: "admin".into(),
            relpath: "Trending Posts".into(),
        };

        let mut sharedstate = SharedState::default();
        let result = create_collection(&collection_dto, &mut sharedstate);
        assert!(matches!(result, Err(Error::FileNotFound(_))));
    }

    #[test]
    fn create_collection_unicode_path_should_succeed() {
        let tmp = tempfile::tempdir().unwrap();
        let space_name = "Global Space";
        let space_dirname = "global-space";
        let space_abspath = tmp.path().join(space_dirname);

        let dto = CreateSpaceDto {
            name: space_name.into(),
            location: tmp.path().to_string_lossy().into(),
        };

        let mut sharedstate = SharedState::default();
        let _ = space::create_space(dto, &mut sharedstate).unwrap();

        fs::create_dir_all(space_abspath.join("global")).unwrap();

        let collection_dto = CreateCollectionDto {
            parent_relpath: "global".into(),
            relpath: "ザク/設定".into(),
        };

        let result = create_collection(&collection_dto, &mut sharedstate).unwrap();
        assert_eq!(result.relpath, "ザク/設定");
        assert!(space_abspath.join("global/ザク/設定").exists());
    }

    #[test]
    fn create_collection_should_save_display_name() {
        let tmp = tempfile::tempdir().unwrap();
        let space_name = "Prefs";
        let space_dirname = "prefs";
        let space_abspath = tmp.path().join(space_dirname);

        let dto = CreateSpaceDto {
            name: space_name.into(),
            location: tmp.path().to_string_lossy().into(),
        };

        let mut sharedstate = SharedState::default();
        let _ = space::create_space(dto, &mut sharedstate).unwrap();

        fs::create_dir_all(space_abspath.join("prefs")).unwrap();

        let collection_dto = CreateCollectionDto {
            parent_relpath: "prefs".into(),
            relpath: "Privacy Settings".into(),
        };

        let result = create_collection(&collection_dto, &mut sharedstate).unwrap();

        let displayname_map = displayname_by_relpath(&space_abspath).unwrap();
        assert_eq!(
            displayname_map.get("prefs/privacy-settings"),
            Some(&"Privacy Settings".into())
        );

        assert_eq!(result.relpath, "privacy-settings");
        assert!(space_abspath.join("prefs/privacy-settings").exists());
    }

    #[cfg(windows)]
    mod windows {
        use super::*;

        #[test]
        fn create_collections_all_reserved_names_should_fail() {
            let tmp = tempfile::tempdir().unwrap();
            let space_abspath = tmp.path();
            std::fs::create_dir_all(space_abspath.join("system")).unwrap();

            let dto = CreateCollectionDto {
                parent_relpath: "system".into(),
                relpath: "NUL/Config".into(),
            };

            let result = create_collections_all(space_abspath, &dto);

            assert!(
                result.is_err(),
                "Expected failure due to reserved name on Windows"
            );
        }
    }
}
