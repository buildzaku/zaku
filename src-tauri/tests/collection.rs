use std::fs;
use tempfile::tempdir;

use zaku::{
    collection::{self, models::CreateCollectionDto},
    error::Error,
};

mod create_collections_all {
    use super::*;

    #[test]
    fn basic() {
        let tmp_dir = tempdir().unwrap();
        let space_abspath = tmp_dir.path();
        let dto = CreateCollectionDto {
            parent_relpath: "admin".to_string(),
            relpath: "Users/Settings/Notifications".to_string(),
        };

        let col_abspath = space_abspath.join("admin");
        fs::create_dir_all(&col_abspath).unwrap();

        let result = collection::create_collections_all(space_abspath, &dto).unwrap();
        assert_eq!(result, "users/settings/notifications");

        let expected_path = col_abspath.join("users/settings/notifications");
        assert!(expected_path.exists());
    }

    #[test]
    fn empty_relpath() {
        let tmp_dir = tempdir().unwrap();
        let space_abspath = tmp_dir.path();
        let dto = CreateCollectionDto {
            parent_relpath: "auth".to_string(),
            relpath: "   ".to_string(),
        };

        let result = collection::create_collections_all(space_abspath, &dto);
        assert!(matches!(result, Err(Error::FileNotFound(_))));
    }

    #[test]
    fn sanitization() {
        let tmp_dir = tempdir().unwrap();
        let space_abspath = tmp_dir.path();
        let dto = CreateCollectionDto {
            parent_relpath: "users".to_string(),
            relpath: "Notification Settings/List notifications".to_string(),
        };

        fs::create_dir_all(space_abspath.join("users")).unwrap();

        let col_relpath = collection::create_collections_all(space_abspath, &dto).unwrap();
        assert_eq!(col_relpath, "notification-settings/list-notifications");

        assert!(space_abspath
            .join("users/notification-settings/list-notifications")
            .exists());
    }

    #[test]
    fn parent_folder_missing_should_fail() {
        let tmp_dir = tempdir().unwrap();
        let space_abspath = tmp_dir.path();

        let dto = CreateCollectionDto {
            parent_relpath: "admin/settings".to_string(),
            relpath: "Preferences/Privacy".to_string(),
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
    fn relpath_with_whitespace_segments_should_skip() {
        let tmp = tempdir().unwrap();
        let space_abspath = tmp.path();
        std::fs::create_dir_all(space_abspath.join("admin")).unwrap();

        let dto = CreateCollectionDto {
            parent_relpath: "admin".to_string(),
            relpath: "  /Notifications       /   ".to_string(),
        };

        let result = collection::create_collections_all(space_abspath, &dto).unwrap();
        assert_eq!(result, "notifications");

        assert!(space_abspath.join("admin/notifications").exists());
    }

    #[test]
    fn relpath_with_multiple_slashes_should_be_handled() {
        let tmp = tempdir().unwrap();
        let space_abspath = tmp.path();
        std::fs::create_dir_all(space_abspath.join("settings")).unwrap();

        let dto = CreateCollectionDto {
            parent_relpath: "settings".to_string(),
            relpath: "System///Display".to_string(),
        };

        let result = collection::create_collections_all(space_abspath, &dto).unwrap();
        assert_eq!(result, "system/display");

        assert!(space_abspath.join("settings/system/display").exists());
    }

    #[test]
    fn relpath_with_only_empty_segments_should_return_error() {
        let tmp = tempdir().unwrap();
        let space_abspath = tmp.path();
        std::fs::create_dir_all(space_abspath.join("posts")).unwrap();

        let dto = CreateCollectionDto {
            parent_relpath: "posts".to_string(),
            relpath: "   /   /   ".to_string(),
        };

        let result = collection::create_collections_all(space_abspath, &dto);
        assert!(matches!(result, Ok(p) if p.is_empty()));
    }

    #[test]
    fn duplicate_create_collections_should_not_fail() {
        let tmp = tempdir().unwrap();
        let space_abspath = tmp.path();
        std::fs::create_dir_all(space_abspath.join("workspace")).unwrap();

        let dto = CreateCollectionDto {
            parent_relpath: "workspace".to_string(),
            relpath: "Config/Options".to_string(),
        };

        let _ = collection::create_collections_all(space_abspath, &dto).unwrap();
        let result = collection::create_collections_all(space_abspath, &dto).unwrap();

        assert_eq!(result, "config/options");
    }

    #[test]
    fn special_characters_should_be_sanitized_or_preserved() {
        let tmp = tempfile::tempdir().unwrap();
        let space_abspath = tmp.path();
        std::fs::create_dir_all(space_abspath.join("library")).unwrap();

        let dto = CreateCollectionDto {
            parent_relpath: "library".to_string(),
            relpath: "Config@Home/Naïve#Settings/🔥 Experimental".to_string(),
        };

        let result = collection::create_collections_all(space_abspath, &dto).unwrap();

        assert_eq!(result, "config@home/naïve#settings/🔥-experimental");

        let expected_path =
            space_abspath.join("library/config@home/naïve#settings/🔥-experimental");

        assert!(expected_path.exists());
    }

    #[test]
    fn unicode_segments_should_be_handled() {
        let tmp = tempfile::tempdir().unwrap();
        let space_abspath = tmp.path();
        std::fs::create_dir_all(space_abspath.join("global")).unwrap();

        let dto = CreateCollectionDto {
            parent_relpath: "global".to_string(),
            relpath: "ザク/設定".to_string(),
        };

        let result = collection::create_collections_all(space_abspath, &dto).unwrap();
        assert_eq!(result, "ザク/設定");
        assert!(space_abspath.join("global/ザク/設定").exists());
    }

    #[test]
    fn trailing_slash_should_be_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let space_abspath = tmp.path();
        std::fs::create_dir_all(space_abspath.join("root")).unwrap();

        let dto = CreateCollectionDto {
            parent_relpath: "root".to_string(),
            relpath: "Settings/Preferences/".to_string(),
        };

        let result = collection::create_collections_all(space_abspath, &dto).unwrap();
        assert_eq!(result, "settings/preferences");
        assert!(space_abspath.join("root/settings/preferences").exists());
    }

    #[test]
    fn invalid_characters_should_be_sanitized() {
        let tmp = tempfile::tempdir().unwrap();
        let space_abspath = tmp.path();
        std::fs::create_dir_all(space_abspath.join("logs")).unwrap();

        let dto = CreateCollectionDto {
            parent_relpath: "logs".to_string(),
            relpath: r#"Error|Logs/<Critical>?Events:2025*Backup\Archive"Today""#.to_string(),
        };

        let result = collection::create_collections_all(space_abspath, &dto);
        assert!(result.is_ok());

        let expected = "error-logs/critical--events-2025-backup-archive-today";
        assert_eq!(result.unwrap(), expected);

        let expected_path = space_abspath.join("logs").join(expected);
        assert!(expected_path.exists());
    }

    #[cfg(windows)]
    mod windows {
        use super::*;

        #[test]
        fn reserved_names_should_fail() {
            let tmp = tempfile::tempdir().unwrap();
            let space_abspath = tmp.path();
            std::fs::create_dir_all(space_abspath.join("system")).unwrap();

            let dto = CreateCollectionDto {
                parent_relpath: "system".to_string(),
                relpath: "NUL/Config".to_string(),
            };

            let result = collection::create_collections_all(space_abspath, &dto);

            assert!(
                result.is_err(),
                "Expected failure due to reserved name on Windows"
            );
        }
    }
}
