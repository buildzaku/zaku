use std::{collections::HashMap, fs, path::PathBuf};
use tempfile;

use crate::{
    collection::{
        self,
        models::{ColName, CreateCollectionDto},
    },
    error::Error,
    request::{self, models::CreateRequestDto},
    space::{self, models::CreateSpaceDto},
    state::SharedState,
    utils,
};

#[test]
fn parse_root_collection_should_match_created_structure() {
    let collections_dto = vec![
        CreateCollectionDto {
            parent_relpath: "".into(),
            relpath: "Auth".into(),
        },
        CreateCollectionDto {
            parent_relpath: "".into(),
            relpath: "Users".into(),
        },
        CreateCollectionDto {
            parent_relpath: "users".into(),
            relpath: "Settings/Notifications".into(),
        },
        CreateCollectionDto {
            parent_relpath: "".into(),
            relpath: "Trending/Posts".into(),
        },
        CreateCollectionDto {
            parent_relpath: "".into(),
            relpath: "Data ~~~ Stats/Charts\\Monthly  ".into(),
        },
        CreateCollectionDto {
            parent_relpath: "".into(),
            relpath: "⚠️ ザク/🔥/💬 Status?".into(),
        },
    ];

    let requests_dto = vec![
        CreateRequestDto {
            parent_relpath: "".into(),
            relpath: "Ping".into(),
        },
        CreateRequestDto {
            parent_relpath: "".into(),
            relpath: "Admin/Ban User by ID".into(),
        },
        CreateRequestDto {
            parent_relpath: "auth".into(),
            relpath: "Access Token".into(),
        },
        CreateRequestDto {
            parent_relpath: "users".into(),
            relpath: "Get user by ID".into(),
        },
        CreateRequestDto {
            parent_relpath: "users/settings".into(),
            relpath: "Update User Preferences".into(),
        },
        CreateRequestDto {
            parent_relpath: "users/settings/notifications".into(),
            relpath: "List notifications".into(),
        },
        CreateRequestDto {
            parent_relpath: "trending/posts".into(),
            relpath: "List Top 25".into(),
        },
        CreateRequestDto {
            parent_relpath: "data-~~~-stats/charts-monthly".into(),
            relpath: "Export/CSV*&Report".into(),
        },
        CreateRequestDto {
            parent_relpath: "⚠️-ザク/🔥/💬-status".into(),
            relpath: "💡Idea:/*>?Bank".into(),
        },
    ];

    let expected_colname_by_relpath = HashMap::from([
        ("auth", "Auth"),
        ("users", "Users"),
        ("users/settings/notifications", "Notifications"),
        ("trending/posts", "Posts"),
        ("data-~~~-stats/charts-monthly", "Charts\\Monthly"),
        ("⚠️-ザク/🔥/💬-status", "💬 Status?"),
    ]);

    let expected_reqname_by_relpath = HashMap::from([
        ("ping.toml", "Ping"),
        ("admin/ban-user-by-id.toml", "Ban User by ID"),
        ("auth/access-token.toml", "Access Token"),
        ("users/get-user-by-id.toml", "Get user by ID"),
        (
            "users/settings/update-user-preferences.toml",
            "Update User Preferences",
        ),
        (
            "users/settings/notifications/list-notifications.toml",
            "List notifications",
        ),
        ("trending/posts/list-top-25.toml", "List Top 25"),
        (
            "data-~~~-stats/charts-monthly/export/csv-&report.toml",
            "CSV*&Report",
        ),
        ("⚠️-ザク/🔥/💬-status/💡idea/bank.toml", "*>?Bank"),
    ]);

    let tmp_dir = tempfile::tempdir().unwrap();
    let dto = CreateSpaceDto {
        name: "Main Space".into(),
        location: tmp_dir.path().to_string_lossy().into(),
    };

    let mut sharedstate = SharedState::default();
    space::create_space(dto, &mut sharedstate).expect("Failed to create space");
    let space_abspath = PathBuf::from(&sharedstate.space.as_ref().unwrap().abspath);

    for col_dto in &collections_dto {
        collection::create_collections_all(&space_abspath, col_dto)
            .expect("Failed to create collection");
    }

    for req_dto in &requests_dto {
        request::create_req(req_dto, &mut sharedstate).expect("Failed to create request");
    }

    let root_collection =
        collection::parse_root_collection(&space_abspath).expect("Failed to parse root collection");

    let mut stack = vec![(&root_collection, String::new())];

    while let Some((collection, current_path)) = stack.pop() {
        if !current_path.is_empty() {
            if let Some(expected_name) = expected_colname_by_relpath.get(current_path.as_str()) {
                assert_eq!(
                    collection.meta.name.as_deref(),
                    Some(*expected_name),
                    "Collection name mismatch at '{}'",
                    current_path
                );
            }
        }

        for req in &collection.requests {
            let req_path = if current_path.is_empty() {
                req.meta.fsname.clone()
            } else {
                utils::join_str_paths(vec![&current_path, &req.meta.fsname])
            };

            let expected_name = expected_reqname_by_relpath
                .get(req_path.as_str())
                .expect(&format!("Unexpected request: {}", req_path));
            assert_eq!(
                req.meta.name, *expected_name,
                "Request name mismatch at '{}'",
                req_path
            );
        }

        for child in &collection.collections {
            let child_path = if current_path.is_empty() {
                child.meta.fsname.clone()
            } else {
                utils::join_str_paths(vec![&current_path, &child.meta.fsname])
            };
            stack.push((child, child_path));
        }
    }
}

#[test]
fn colname_by_relpath_reads_existing_data() {
    let tmp_dir = tempfile::tempdir().expect("Failed to create temporary directory");
    let space_abspath = tmp_dir.path();

    let colname_filepath = space_abspath.join(".zaku/collections/name.toml");
    fs::create_dir_all(
        colname_filepath
            .parent()
            .expect("Failed to get parent directory"),
    )
    .expect("Failed to create parent directories");

    let mut mappings = HashMap::new();
    mappings.insert("demo/path".to_string(), "Demo Path".to_string());

    let colname = ColName { mappings };
    let serialized = toml::to_string_pretty(&colname).expect("Failed to serialize ColName struct");

    fs::write(&colname_filepath, serialized).expect("Failed to write TOML file");

    let colname =
        collection::colname_by_relpath(space_abspath).expect("Failed to get collection names");
    assert_eq!(colname.mappings.get("demo/path"), Some(&"Demo Path".into()));
}

#[test]
fn colname_by_relpath_invalid_toml_should_fail() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();

    let colname_filepath = space_abspath.join(".zaku/collections/name.toml");
    fs::create_dir_all(colname_filepath.parent().unwrap()).unwrap();

    let invalid_toml = "[mappings\n\"demo/path\" = \"Demo Path\"";
    fs::write(&colname_filepath, invalid_toml).unwrap();

    let result = collection::colname_by_relpath(space_abspath);
    assert!(result.is_err());
}

#[test]
fn colname_by_relpath_creates_file_if_missing() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();

    let colname =
        collection::colname_by_relpath(space_abspath).expect("Failed to get collection names");
    assert!(colname.mappings.is_empty());

    let file_path = space_abspath.join(".zaku/collections/name.toml");
    assert!(file_path.exists());

    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content.trim(), "[mappings]");
}

#[test]
fn save_colname_if_missing_writes_new_entry() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();

    collection::save_colname_if_missing(space_abspath, "config/settings", "Config Settings")
        .expect("Failed to save collection name");

    let colname =
        collection::colname_by_relpath(space_abspath).expect("Failed to get collection names");
    assert_eq!(
        colname.mappings.get("config/settings"),
        Some(&"Config Settings".into())
    );
}

#[test]
fn save_colname_if_missing_does_not_overwrite_existing() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();

    collection::save_colname_if_missing(space_abspath, "a/b", "Alpha")
        .expect("Failed to save collection name");
    collection::save_colname_if_missing(space_abspath, "a/b", "Beta")
        .expect("Failed to save collection name");

    let colname =
        collection::colname_by_relpath(space_abspath).expect("Failed to get collection names");
    assert_eq!(colname.mappings.get("a/b"), Some(&"Alpha".into()));
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

    let col_relpath = collection::create_collections_all(space_abspath, &dto)
        .expect("Failed to create collection directory/directories");
    assert_eq!(col_relpath, "users/settings/notifications");

    let expect_path = col_abspath.join("users/settings/notifications");
    assert!(expect_path.exists());
}

#[test]
fn create_collections_all_empty_relpath() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();
    let dto = CreateCollectionDto {
        parent_relpath: "auth".into(),
        relpath: "   ".into(),
    };

    let result = collection::create_collections_all(space_abspath, &dto);
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

    let col_relpath = collection::create_collections_all(space_abspath, &dto)
        .expect("Failed to create collection directory/directories");
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

    let result = collection::create_collections_all(space_abspath, &dto);
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
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();
    std::fs::create_dir_all(space_abspath.join("admin")).unwrap();

    let dto = CreateCollectionDto {
        parent_relpath: "admin".into(),
        relpath: "  /Notifications       /   ".into(),
    };

    let col_relpath = collection::create_collections_all(space_abspath, &dto)
        .expect("Failed to create collection directory/directories");
    assert_eq!(col_relpath, "notifications");

    assert!(space_abspath.join("admin/notifications").exists());
}

#[test]
fn create_collections_all_relpath_with_multiple_slashes_should_be_handled() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();
    std::fs::create_dir_all(space_abspath.join("settings")).unwrap();

    let dto = CreateCollectionDto {
        parent_relpath: "settings".into(),
        relpath: "System///Display".into(),
    };

    let col_relpath = collection::create_collections_all(space_abspath, &dto)
        .expect("Failed to create collection directory/directories");
    assert_eq!(col_relpath, "system/display");

    assert!(space_abspath.join("settings/system/display").exists());
}

#[test]
fn create_collections_all_relpath_with_only_empty_segments_should_return_error() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();
    std::fs::create_dir_all(space_abspath.join("posts")).unwrap();

    let dto = CreateCollectionDto {
        parent_relpath: "posts".into(),
        relpath: "   /   /   ".into(),
    };

    let result = collection::create_collections_all(space_abspath, &dto);
    assert!(matches!(result, Ok(p) if p.is_empty()));
}

#[test]
fn create_collections_all_duplicate_create_collections_should_not_fail() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();
    std::fs::create_dir_all(space_abspath.join("workspace")).unwrap();

    let dto = CreateCollectionDto {
        parent_relpath: "workspace".into(),
        relpath: "Config/Options".into(),
    };

    let _ = collection::create_collections_all(space_abspath, &dto)
        .expect("Failed to create collection directory/directories");
    let col_relpath = collection::create_collections_all(space_abspath, &dto)
        .expect("Failed to create collection directory/directories");

    assert_eq!(col_relpath, "config/options");
}

#[test]
fn create_collections_all_special_characters_should_be_sanitized_or_preserved() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();
    std::fs::create_dir_all(space_abspath.join("library")).unwrap();

    let dto = CreateCollectionDto {
        parent_relpath: "library".into(),
        relpath: "Config@Home/Naïve#Settings/🔥 Experimental".into(),
    };

    let col_relpath = collection::create_collections_all(space_abspath, &dto)
        .expect("Failed to create collection directory/directories");

    assert_eq!(col_relpath, "config@home/naïve#settings/🔥-experimental");

    let expect_path = space_abspath.join("library/config@home/naïve#settings/🔥-experimental");
    assert!(expect_path.exists());

    let colname = collection::colname_by_relpath(space_abspath)
        .expect("Failed to read collection name mappings");

    assert_eq!(
        colname.mappings.get("library/config@home"),
        Some(&"Config@Home".to_string())
    );
    assert_eq!(
        colname.mappings.get("library/config@home/naïve#settings"),
        Some(&"Naïve#Settings".to_string())
    );
    assert_eq!(
        colname
            .mappings
            .get("library/config@home/naïve#settings/🔥-experimental"),
        Some(&"🔥 Experimental".to_string())
    );
}

#[test]
fn create_collections_all_unicode_segments_should_be_handled() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();
    std::fs::create_dir_all(space_abspath.join("global")).unwrap();

    let dto = CreateCollectionDto {
        parent_relpath: "global".into(),
        relpath: "ザク/設定".into(),
    };

    let col_relpath = collection::create_collections_all(space_abspath, &dto)
        .expect("Failed to create collection directory/directories");
    assert_eq!(col_relpath, "ザク/設定");
    assert!(space_abspath.join("global/ザク/設定").exists());
}

#[test]
fn create_collections_all_trailing_slash_should_be_ignored() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();
    std::fs::create_dir_all(space_abspath.join("root")).unwrap();

    let dto = CreateCollectionDto {
        parent_relpath: "root".into(),
        relpath: "Settings/Preferences/".into(),
    };

    let col_relpath = collection::create_collections_all(space_abspath, &dto)
        .expect("Failed to create collection directory/directories");
    assert_eq!(col_relpath, "settings/preferences");
    assert!(space_abspath.join("root/settings/preferences").exists());
}

#[test]
fn create_collections_all_invalid_characters_should_be_sanitized() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();
    std::fs::create_dir_all(space_abspath.join("logs")).unwrap();

    let dto = CreateCollectionDto {
        parent_relpath: "logs".into(),
        relpath: r#"Error|Logs/<Critical>?Events:2025*Backup\Archive"Today""#.into(),
    };

    let col_relpath = collection::create_collections_all(space_abspath, &dto)
        .expect("Failed to create collection directory/directories");

    let expect_relpath = "error-logs/critical-events-2025-backup-archive-today";
    assert_eq!(col_relpath, expect_relpath);

    let expect_path = space_abspath.join("logs").join(expect_relpath);
    assert!(expect_path.exists());
}

#[test]
fn create_collection_basic() {
    let tmp_dir = tempfile::tempdir().unwrap();
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

    let col = collection::create_collection(&collection_dto, &mut sharedstate)
        .expect("Failed to create collection");

    assert_eq!(col.relpath, "admin/settings/notifications");
    assert!(space_abspath.join("admin/settings/notifications").exists());
}

#[test]
fn create_collection_empty_relpath_should_fail() {
    let tmp_dir = tempfile::tempdir().unwrap();
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

    let result = collection::create_collection(&collection_dto, &mut sharedstate);
    assert!(matches!(result, Err(Error::FileNotFound(_))));
}

#[test]
fn create_collection_missing_space_should_fail() {
    let collection_dto = CreateCollectionDto {
        parent_relpath: "admin".into(),
        relpath: "Trending Posts".into(),
    };

    let mut sharedstate = SharedState::default();
    let result = collection::create_collection(&collection_dto, &mut sharedstate);
    assert!(matches!(result, Err(Error::FileNotFound(_))));
}

#[test]
fn create_collection_unicode_path_should_succeed() {
    let tmp_dir = tempfile::tempdir().unwrap();
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

    let result = collection::create_collection(&collection_dto, &mut sharedstate)
        .expect("Failed to create collection");
    assert_eq!(result.relpath, "global/ザク/設定");
    assert!(space_abspath.join("global/ザク/設定").exists());
}

#[test]
fn create_collection_should_save_colname() {
    let tmp_dir = tempfile::tempdir().unwrap();
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

    let result = collection::create_collection(&collection_dto, &mut sharedstate)
        .expect("Failed to create collection");

    let colname =
        collection::colname_by_relpath(&space_abspath).expect("Failed to get collection names");
    assert_eq!(
        colname.mappings.get("prefs/privacy-settings"),
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
        let tmp_dir = tempfile::tempdir().unwrap();
        let space_abspath = tmp_dir.path();
        std::fs::create_dir_all(space_abspath.join("system")).unwrap();

        let dto = CreateCollectionDto {
            parent_relpath: "system".into(),
            relpath: "NUL/Config".into(),
        };

        let result = collection::create_collections_all(space_abspath, &dto);

        assert!(
            result.is_err(),
            "Expected failure due to reserved name on Windows"
        );
    }
}
