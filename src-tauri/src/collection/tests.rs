use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
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
};

#[test]
fn parse_collection_should_match_created_structure() {
    struct TempCollection<'a> {
        relpath: &'a str,
        req_relpaths: Vec<&'a str>,
    }

    let structure = vec![
        TempCollection {
            relpath: "",
            req_relpaths: vec!["Ping", "Admin/Ban User by ID"],
        },
        TempCollection {
            relpath: "Auth",
            req_relpaths: vec!["Access Token"],
        },
        TempCollection {
            relpath: "Users",
            req_relpaths: vec![
                "Get user by ID",
                "Settings/Update User Preferences",
                "Settings/Notifications/List notifications",
            ],
        },
        TempCollection {
            relpath: "Trending/Posts",
            req_relpaths: vec!["List Top 25"],
        },
        TempCollection {
            relpath: "Data ~~~ Stats/Charts\\Monthly  ",
            req_relpaths: vec!["Export/CSV*&Report"],
        },
        TempCollection {
            relpath: "⚠️ ザク/🔥/💬 Status?",
            req_relpaths: vec!["💡Idea:/*>?Bank"],
        },
    ];

    let tmp_dir = tempfile::tempdir().unwrap();
    let space_name = "Structure Test";
    let space_dir = "structure-test";
    let dto = CreateSpaceDto {
        name: space_name.into(),
        location: tmp_dir.path().to_string_lossy().into(),
    };

    let mut sharedstate = SharedState::default();
    space::create_space(dto, &mut sharedstate).expect("Failed to create space");
    let space_abspath = PathBuf::from(&sharedstate.active_space.as_ref().unwrap().abspath);

    for col in &structure {
        let is_spaceroot = col.relpath.is_empty();
        let mut parent_relpath = "".to_string();

        if !is_spaceroot {
            let col_dto = CreateCollectionDto {
                parent_relpath: "".to_string(),
                relpath: col.relpath.to_string(),
            };
            parent_relpath = collection::create_collections_all(&space_abspath, &col_dto)
                .expect("Failed to create collection");
        }

        for relpath in col.req_relpaths.iter().cloned() {
            let req_dto = CreateRequestDto {
                parent_relpath: parent_relpath.clone(),
                relpath: relpath.to_string(),
            };
            request::create_req(&req_dto, &mut sharedstate).expect("Failed to create request");
        }
    }

    let parsed = collection::parse_collection(&space_abspath).expect("Failed to parse collection");
    assert_eq!(parsed.meta.dir_name, space_dir);

    let root_collections = &parsed.collections;

    let auth = root_collections
        .iter()
        .find(|c| c.meta.dir_name == "auth")
        .expect("Missing 'auth'");
    assert_eq!(auth.meta.name.as_deref(), Some("Auth"));
    let auth_path = space_abspath.join("auth");
    assert!(auth_path.is_dir());
    assert!(auth.requests.iter().any(|r| r.meta.name == "Access Token"));
    assert!(auth_path.join("access-token.toml").is_file());

    let users = root_collections
        .iter()
        .find(|c| c.meta.dir_name == "users")
        .expect("Missing 'users'");
    assert_eq!(users.meta.name.as_deref(), Some("Users"));
    let users_path = space_abspath.join("users");
    assert!(users_path.is_dir());
    assert!(users
        .requests
        .iter()
        .any(|r| r.meta.name == "Get user by ID"));
    assert!(users_path.join("get-user-by-id.toml").is_file());

    let settings = users
        .collections
        .iter()
        .find(|c| c.meta.dir_name == "settings")
        .expect("Missing 'settings'");
    assert_eq!(settings.meta.name.as_deref(), Some("Settings"));
    let settings_path = users_path.join("settings");
    assert!(settings_path.is_dir());
    assert!(settings
        .requests
        .iter()
        .any(|r| r.meta.name == "Update User Preferences"));
    assert!(settings_path.join("update-user-preferences.toml").is_file());

    let notifications = settings
        .collections
        .iter()
        .find(|c| c.meta.dir_name == "notifications")
        .expect("Missing 'notifications'");
    assert_eq!(notifications.meta.name.as_deref(), Some("Notifications"));
    let notifications_path = settings_path.join("notifications");
    assert!(notifications_path.is_dir());
    assert!(notifications
        .requests
        .iter()
        .any(|r| r.meta.name == "List notifications"));
    assert!(notifications_path.join("list-notifications.toml").is_file());

    assert!(parsed.requests.iter().any(|r| r.meta.name == "Ping"));
    assert!(space_abspath.join("ping.toml").is_file());

    let admin = root_collections
        .iter()
        .find(|c| c.meta.dir_name == "admin")
        .expect("Missing 'admin'");
    assert_eq!(admin.meta.name.as_deref(), Some("Admin"));
    let admin_path = space_abspath.join("admin");
    assert!(admin_path.is_dir());
    assert!(admin
        .requests
        .iter()
        .any(|r| r.meta.name == "Ban User by ID"));
    assert!(admin_path.join("ban-user-by-id.toml").is_file());

    let trending = root_collections
        .iter()
        .find(|c| c.meta.dir_name == "trending")
        .expect("Missing 'trending'");
    assert_eq!(trending.meta.name.as_deref(), Some("Trending"));
    let trending_path = space_abspath.join("trending");
    assert!(trending_path.is_dir());

    let posts = trending
        .collections
        .iter()
        .find(|c| c.meta.dir_name == "posts")
        .expect("Missing 'posts'");
    assert_eq!(posts.meta.name.as_deref(), Some("Posts"));
    let posts_path = trending_path.join("posts");
    assert!(posts_path.is_dir());
    assert!(posts.requests.iter().any(|r| r.meta.name == "List Top 25"));
    assert!(posts_path.join("list-top-25.toml").is_file());

    let data_stats = root_collections
        .iter()
        .find(|c| c.meta.dir_name == "data-~~~-stats")
        .expect("Missing 'data-~~~-stats'");
    assert_eq!(data_stats.meta.name.as_deref(), Some("Data ~~~ Stats"));
    let data_path = space_abspath.join("data-~~~-stats");
    assert!(data_path.is_dir());

    let charts = data_stats
        .collections
        .iter()
        .find(|c| c.meta.dir_name == "charts-monthly")
        .expect("Missing 'charts-monthly'");
    assert_eq!(charts.meta.name.as_deref(), Some("Charts\\Monthly"));
    let charts_path = data_path.join("charts-monthly");
    assert!(charts_path.is_dir());

    let export = charts
        .collections
        .iter()
        .find(|c| c.meta.dir_name == "export")
        .expect("Missing 'export'");
    assert_eq!(export.meta.name.as_deref(), Some("Export"));
    let export_path = charts_path.join("export");
    assert!(export_path.is_dir());

    assert!(export.requests.iter().any(|r| r.meta.name == "CSV*&Report"));
    assert!(export_path.join("csv-&report.toml").is_file());

    let zaku = root_collections
        .iter()
        .find(|c| c.meta.dir_name == "⚠️-ザク")
        .expect("Missing '⚠️-ザク'");
    assert_eq!(zaku.meta.name.as_deref(), Some("⚠️ ザク"));
    let zaku_path = space_abspath.join("⚠️-ザク");
    assert!(zaku_path.is_dir());

    let fire = zaku
        .collections
        .iter()
        .find(|c| c.meta.dir_name == "🔥")
        .expect("Missing '🔥'");
    assert_eq!(fire.meta.name.as_deref(), Some("🔥"));
    let fire_path = zaku_path.join("🔥");
    assert!(fire_path.is_dir());

    let status = fire
        .collections
        .iter()
        .find(|c| c.meta.dir_name == "💬-status")
        .expect("Missing '💬-status'");
    assert_eq!(status.meta.name.as_deref(), Some("💬 Status?"));
    let status_path = fire_path.join("💬-status");
    assert!(status_path.is_dir());

    let idea = status
        .collections
        .iter()
        .find(|c| c.meta.dir_name == "💡idea")
        .expect("Missing '💡idea'");
    assert_eq!(idea.meta.name.as_deref(), Some("💡Idea:"));
    let idea_path = status_path.join("💡idea");
    assert!(idea_path.is_dir());

    for r in &idea.requests {
        println!("Request meta.name: {:?}", r.meta.name);
    }
    assert!(idea.requests.iter().any(|r| r.meta.name == "*>?Bank"));
    assert!(idea_path.join("bank.toml").is_file());
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

    let expected_path = space_abspath.join("library/config@home/naïve#settings/🔥-experimental");
    assert!(expected_path.exists());

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

    let expected_relpath = "error-logs/critical-events-2025-backup-archive-today";
    assert_eq!(col_relpath, expected_relpath);

    let expected_path = space_abspath.join("logs").join(expected_relpath);
    assert!(expected_path.exists());
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
fn create_collection_missing_active_space_should_fail() {
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
