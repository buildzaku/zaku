use super::*;
use tempfile;

use crate::space::models::CreateSpaceDto;

#[test]
fn displayname_by_relpath_reads_existing_data() {
    let tmp_dir = tempfile::tempdir().expect("Failed to create temporary directory");
    let space_abspath = tmp_dir.path();

    let displayname_path = space_abspath.join(".zaku/collections/display_name.toml");
    fs::create_dir_all(displayname_path.parent().unwrap()).unwrap();
    fs::write(&displayname_path, r#""demo/path" = "Demo Path""#).unwrap();

    let displayname_map = displayname_by_relpath(space_abspath).unwrap();
    assert_eq!(displayname_map.get("demo/path"), Some(&"Demo Path".into()));
}

#[test]
fn displayname_by_relpath_invalid_toml_should_fail() {
    let tmp_dir = tempfile::tempdir().expect("Failed to create temporary directory");
    let space_abspath = tmp_dir.path();

    let displayname_path = space_abspath.join(".zaku/collections/display_name.toml");
    fs::create_dir_all(displayname_path.parent().unwrap()).unwrap();
    fs::write(&displayname_path, "not = [valid").unwrap();

    let result = displayname_by_relpath(space_abspath);
    assert!(result.is_err());
}

#[test]
fn displayname_by_relpath_creates_file_if_missing() {
    let tmp_dir = tempfile::tempdir().expect("Failed to create temporary directory");
    let space_abspath = tmp_dir.path();

    let displayname_map = displayname_by_relpath(space_abspath).unwrap();
    assert!(displayname_map.is_empty());

    let file_path = space_abspath.join(".zaku/collections/display_name.toml");
    assert!(file_path.exists());

    let content = fs::read_to_string(file_path).unwrap();
    assert!(content.contains("{}") || content.trim().is_empty());
}

#[test]
fn save_displayname_if_missing_writes_new_entry() {
    let tmp_dir = tempfile::tempdir().expect("Failed to create temporary directory");
    let space_abspath = tmp_dir.path();

    save_displayname_if_missing(space_abspath, "config/settings", "Config Settings").unwrap();

    let displayname_map = displayname_by_relpath(space_abspath).unwrap();
    assert_eq!(
        displayname_map.get("config/settings"),
        Some(&"Config Settings".into())
    );
}

#[test]
fn save_displayname_if_missing_does_not_overwrite_existing() {
    let tmp_dir = tempfile::tempdir().expect("Failed to create temporary directory");
    let space_abspath = tmp_dir.path();

    save_displayname_if_missing(space_abspath, "a/b", "Alpha").unwrap();
    save_displayname_if_missing(space_abspath, "a/b", "Beta").unwrap();

    let map = displayname_by_relpath(space_abspath).unwrap();
    assert_eq!(map.get("a/b"), Some(&"Alpha".into()));
}

#[test]
fn create_collections_all_basic() {
    let tmp_dir = tempfile::tempdir().expect("Failed to create temporary directory");
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
    let tmp_dir = tempfile::tempdir().expect("Failed to create temporary directory");
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
    let tmp_dir = tempfile::tempdir().expect("Failed to create temporary directory");
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
    let tmp_dir = tempfile::tempdir().expect("Failed to create temporary directory");
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
    let tmp_dir = tempfile::tempdir().expect("Failed to create temporary directory");
    let space_abspath = tmp_dir.path();
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
    let tmp_dir = tempfile::tempdir().expect("Failed to create temporary directory");
    let space_abspath = tmp_dir.path();
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
    let tmp_dir = tempfile::tempdir().expect("Failed to create temporary directory");
    let space_abspath = tmp_dir.path();
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
    let tmp_dir = tempfile::tempdir().expect("Failed to create temporary directory");
    let space_abspath = tmp_dir.path();
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
    let tmp_dir = tempfile::tempdir().expect("Failed to create temporary directory");
    let space_abspath = tmp_dir.path();
    std::fs::create_dir_all(space_abspath.join("library")).unwrap();

    let dto = CreateCollectionDto {
        parent_relpath: "library".into(),
        relpath: "Config@Home/Naïve#Settings/🔥 Experimental".into(),
    };

    let result = create_collections_all(space_abspath, &dto).unwrap();

    assert_eq!(result, "config@home/naïve#settings/🔥-experimental");

    let expected_path = space_abspath.join("library/config@home/naïve#settings/🔥-experimental");

    assert!(expected_path.exists());
}

#[test]
fn create_collections_all_unicode_segments_should_be_handled() {
    let tmp_dir = tempfile::tempdir().expect("Failed to create temporary directory");
    let space_abspath = tmp_dir.path();
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
    let tmp_dir = tempfile::tempdir().expect("Failed to create temporary directory");
    let space_abspath = tmp_dir.path();
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
    let tmp_dir = tempfile::tempdir().expect("Failed to create temporary directory");
    let space_abspath = tmp_dir.path();
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
    let tmp_dir = tempfile::tempdir().expect("Failed to create temporary directory");
    let space_name = "Main Space";
    let space_dirname = "main-space";
    let space_abspath = tmp_dir.path().join(space_dirname);

    let dto = CreateSpaceDto {
        name: space_name.into(),
        location: tmp_dir.path().to_string_lossy().into(),
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

    assert_eq!(result.relpath, "admin/settings/notifications");
    assert!(space_abspath.join("admin/settings/notifications").exists());
}

#[test]
fn create_collection_empty_relpath_should_fail() {
    let tmp_dir = tempfile::tempdir().expect("Failed to create temporary directory");
    let space_name = "Empty Check";
    let space_dirname = "empty-check";
    let space_abspath = tmp_dir.path().join(space_dirname);

    let dto = CreateSpaceDto {
        name: space_name.into(),
        location: tmp_dir.path().to_string_lossy().into(),
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
    let tmp_dir = tempfile::tempdir().expect("Failed to create temporary directory");
    let space_name = "Global Space";
    let space_dirname = "global-space";
    let space_abspath = tmp_dir.path().join(space_dirname);

    let dto = CreateSpaceDto {
        name: space_name.into(),
        location: tmp_dir.path().to_string_lossy().into(),
    };

    let mut sharedstate = SharedState::default();
    let _ = space::create_space(dto, &mut sharedstate).unwrap();

    fs::create_dir_all(space_abspath.join("global")).unwrap();

    let collection_dto = CreateCollectionDto {
        parent_relpath: "global".into(),
        relpath: "ザク/設定".into(),
    };

    let result = create_collection(&collection_dto, &mut sharedstate).unwrap();
    assert_eq!(result.relpath, "global/ザク/設定");
    assert!(space_abspath.join("global/ザク/設定").exists());
}

#[test]
fn create_collection_should_save_display_name() {
    let tmp_dir = tempfile::tempdir().expect("Failed to create temporary directory");
    let space_name = "Prefs";
    let space_dirname = "prefs";
    let space_abspath = tmp_dir.path().join(space_dirname);

    let dto = CreateSpaceDto {
        name: space_name.into(),
        location: tmp_dir.path().to_string_lossy().into(),
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

    assert_eq!(result.relpath, "prefs/privacy-settings");
    assert!(space_abspath.join("prefs/privacy-settings").exists());
}

#[cfg(windows)]
mod windows {
    use super::*;

    #[test]
    fn create_collections_all_reserved_names_should_fail() {
        let tmp_dir = tempfile::tempdir().expect("Failed to create temporary directory");
        let space_abspath = tmp_dir.path();
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
