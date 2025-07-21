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
            relpath: "Parent Col 1".into(),
        },
        CreateCollectionDto {
            parent_relpath: "".into(),
            relpath: "Parent Col 2".into(),
        },
        CreateCollectionDto {
            parent_relpath: "parent-col-2".into(),
            relpath: "Child Col 1/Grand Child Col 1".into(),
        },
        CreateCollectionDto {
            parent_relpath: "".into(),
            relpath: "Parent Col 3/Child Col 2".into(),
        },
        CreateCollectionDto {
            parent_relpath: "".into(),
            relpath: "Tilde ~~~ Parent Col 1/Back\\Slash Child Col 1  ".into(),
        },
        CreateCollectionDto {
            parent_relpath: "".into(),
            relpath:
                "⚠️ ザク Unicode Parent Col 1/🔥 Emoji Child Col 1/💬 Emoji Grand Child Col 1?"
                    .into(),
        },
    ];

    let requests_dto = vec![
        CreateRequestDto {
            parent_relpath: "".into(),
            relpath: "Child Req 1".into(),
        },
        CreateRequestDto {
            parent_relpath: "".into(),
            relpath: "Parent Col 1/Child Req 2".into(),
        },
        CreateRequestDto {
            parent_relpath: "parent-col-1".into(),
            relpath: "Parent Req 1".into(),
        },
        CreateRequestDto {
            parent_relpath: "parent-col-2".into(),
            relpath: "Parent Req 2".into(),
        },
        CreateRequestDto {
            parent_relpath: "parent-col-2/child-col-1".into(),
            relpath: "Child Req 3".into(),
        },
        CreateRequestDto {
            parent_relpath: "parent-col-2/child-col-1/grand-child-col-1".into(),
            relpath: "Grand Child Req 1".into(),
        },
        CreateRequestDto {
            parent_relpath: "parent-col-3/child-col-2".into(),
            relpath: "Child Req 4".into(),
        },
        CreateRequestDto {
            parent_relpath: "tilde-~~~-parent-col-1/back-slash-child-col-1".into(),
            relpath: "Grand Child Col 1/Special*&Chars Req 1".into(),
        },
        CreateRequestDto {
            parent_relpath:
                "⚠️-ザク-unicode-parent-col-1/🔥-emoji-child-col-1/💬-emoji-grand-child-col-1"
                    .into(),
            relpath: "💡Emoji Special: Col 1/*>?Special Chars Req 1".into(),
        },
    ];

    let expected_colname_by_relpath_components = vec![
        (vec!["parent-col-1"], "Parent Col 1"),
        (vec!["parent-col-2"], "Parent Col 2"),
        (
            vec!["parent-col-2", "child-col-1", "grand-child-col-1"],
            "Grand Child Col 1",
        ),
        (vec!["parent-col-3", "child-col-2"], "Child Col 2"),
        (
            vec!["tilde-~~~-parent-col-1", "back-slash-child-col-1"],
            "Back-Slash Child Col 1",
        ),
        (
            vec![
                "⚠️-ザク-unicode-parent-col-1",
                "🔥-emoji-child-col-1",
                "💬-emoji-grand-child-col-1",
            ],
            "💬 Emoji Grand Child Col 1?",
        ),
    ];

    let expected_reqname_by_relpath_components = vec![
        (vec!["child-req-1.toml"], "Child Req 1"),
        (vec!["parent-col-1", "child-req-2.toml"], "Child Req 2"),
        (vec!["parent-col-1", "parent-req-1.toml"], "Parent Req 1"),
        (vec!["parent-col-2", "parent-req-2.toml"], "Parent Req 2"),
        (
            vec!["parent-col-2", "child-col-1", "child-req-3.toml"],
            "Child Req 3",
        ),
        (
            vec![
                "parent-col-2",
                "child-col-1",
                "grand-child-col-1",
                "grand-child-req-1.toml",
            ],
            "Grand Child Req 1",
        ),
        (
            vec!["parent-col-3", "child-col-2", "child-req-4.toml"],
            "Child Req 4",
        ),
        (
            vec![
                "tilde-~~~-parent-col-1",
                "back-slash-child-col-1",
                "grand-child-col-1",
                "special-&chars-req-1.toml",
            ],
            "Special*&Chars Req 1",
        ),
        (
            vec![
                "⚠️-ザク-unicode-parent-col-1",
                "🔥-emoji-child-col-1",
                "💬-emoji-grand-child-col-1",
                "💡emoji-special-col-1",
                "special-chars-req-1.toml",
            ],
            "*>?Special Chars Req 1",
        ),
    ];

    let mut expected_colname_by_relpath = HashMap::new();
    for (components, name) in expected_colname_by_relpath_components {
        let path = components
            .iter()
            .fold(PathBuf::new(), |acc, comp| acc.join(comp));
        expected_colname_by_relpath.insert(path.to_string_lossy().to_string(), name);
    }

    let mut expected_reqname_by_relpath = HashMap::new();
    for (components, name) in expected_reqname_by_relpath_components {
        let path = components
            .iter()
            .fold(PathBuf::new(), |acc, comp| acc.join(comp));
        expected_reqname_by_relpath.insert(path.to_string_lossy().to_string(), name);
    }

    let tmp_dir = tempfile::tempdir().unwrap();
    let dto = CreateSpaceDto {
        name: "Space".into(),
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
                    "Collection name mismatch at '{current_path}'"
                );
            }
        }

        for req in &collection.requests {
            let req_path = if current_path.is_empty() {
                req.meta.fsname.clone()
            } else {
                utils::join_strpaths(vec![&current_path, &req.meta.fsname])
            };

            let expected_name = expected_reqname_by_relpath
                .get(req_path.as_str())
                .unwrap_or_else(|| panic!("Unexpected request: {req_path}"));
            assert_eq!(
                req.meta.name, *expected_name,
                "Request name mismatch at '{req_path}'"
            );
        }

        for child in &collection.collections {
            let child_path = if current_path.is_empty() {
                child.meta.fsname.clone()
            } else {
                utils::join_strpaths(vec![&current_path, &child.meta.fsname])
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
    mappings.insert(
        "parent-col-1/child-col-1".to_string(),
        "Child Col 1".to_string(),
    );

    let colname = ColName { mappings };
    let serialized = toml::to_string_pretty(&colname).expect("Failed to serialize ColName struct");

    fs::write(&colname_filepath, serialized).expect("Failed to write TOML file");

    let colname =
        collection::colname_by_relpath(space_abspath).expect("Failed to get collection names");
    assert_eq!(
        colname.mappings.get("parent-col-1/child-col-1"),
        Some(&"Child Col 1".into())
    );
}

#[test]
fn colname_by_relpath_invalid_toml_should_fail() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();

    let colname_filepath = space_abspath.join(".zaku/collections/name.toml");
    fs::create_dir_all(colname_filepath.parent().unwrap()).unwrap();

    let invalid_toml = "[mappings\n\"parent-col-1/child-col-1\" = \"Child Col 1\"";
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

    collection::save_colname_if_missing(space_abspath, "parent-col-1/child-col-1", "Child Col 1")
        .expect("Failed to save collection name");

    let colname =
        collection::colname_by_relpath(space_abspath).expect("Failed to get collection names");
    assert_eq!(
        colname.mappings.get("parent-col-1/child-col-1"),
        Some(&"Child Col 1".into())
    );
}

#[test]
fn save_colname_if_missing_does_not_overwrite_existing() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();

    collection::save_colname_if_missing(
        space_abspath,
        "parent-col-1/child-col-1",
        "Child Col 1 - First Name",
    )
    .expect("Failed to save collection name");
    collection::save_colname_if_missing(
        space_abspath,
        "parent-col-1/child-col-1",
        "Child Col 1 - Second Name",
    )
    .expect("Failed to save collection name");

    let colname =
        collection::colname_by_relpath(space_abspath).expect("Failed to get collection names");
    assert_eq!(
        colname.mappings.get("parent-col-1/child-col-1"),
        Some(&"Child Col 1 - First Name".into())
    );
}

#[test]
fn create_collections_all_basic() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();
    let dto = CreateCollectionDto {
        parent_relpath: "parent-col-1".into(),
        relpath: "Child Col 1/Grand Child Col 1/Great Grand Child Col 1".into(),
    };

    let col_abspath = space_abspath.join("parent-col-1");
    fs::create_dir_all(&col_abspath).unwrap();

    let col_relpath = collection::create_collections_all(space_abspath, &dto)
        .expect("Failed to create collection directory/directories");

    let expected = PathBuf::from("child-col-1")
        .join("grand-child-col-1")
        .join("great-grand-child-col-1");
    assert_eq!(col_relpath, expected.to_string_lossy());

    let expect_path = col_abspath.join("child-col-1/grand-child-col-1/great-grand-child-col-1");
    assert!(expect_path.exists());
}

#[test]
fn create_collections_all_empty_relpath() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();
    let dto = CreateCollectionDto {
        parent_relpath: "parent-col-1".into(),
        relpath: "   ".into(),
    };

    let result = collection::create_collections_all(space_abspath, &dto);
    assert!(matches!(result, Err(Error::FileNotFound(_))));
}

#[test]
fn create_collections_all_sanitization_basic() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();
    let dto = CreateCollectionDto {
        parent_relpath: "grand-parent-col-1".into(),
        relpath: "Parent Col 1/Child Col 1".into(),
    };

    fs::create_dir_all(space_abspath.join("grand-parent-col-1")).unwrap();

    let col_relpath = collection::create_collections_all(space_abspath, &dto)
        .expect("Failed to create collection directory/directories");

    let expected = PathBuf::from("parent-col-1").join("child-col-1");
    assert_eq!(col_relpath, expected.to_string_lossy());

    assert!(space_abspath
        .join("grand-parent-col-1/parent-col-1/child-col-1")
        .exists());
}

#[test]
fn create_collections_all_parent_folder_missing_should_fail() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();

    let dto = CreateCollectionDto {
        parent_relpath: "parent-col-1/child-col-1".into(),
        relpath: "Grand Child Col 1/Great Grand Child Col 1".into(),
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
    std::fs::create_dir_all(space_abspath.join("parent-col-1")).unwrap();

    let dto = CreateCollectionDto {
        parent_relpath: "parent-col-1".into(),
        relpath: "  /Whitespace Child  Col 1       /   ".into(),
    };

    let col_relpath = collection::create_collections_all(space_abspath, &dto)
        .expect("Failed to create collection directory/directories");
    assert_eq!(col_relpath, "whitespace-child-col-1");

    assert!(space_abspath
        .join("parent-col-1/whitespace-child-col-1")
        .exists());
}

#[test]
fn create_collections_all_relpath_with_multiple_slashes_should_be_handled() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();
    std::fs::create_dir_all(space_abspath.join("parent-col-1")).unwrap();

    let dto = CreateCollectionDto {
        parent_relpath: "parent-col-1".into(),
        relpath: "Multiple Slash Col 1///Slash  Col 1".into(),
    };

    let col_relpath = collection::create_collections_all(space_abspath, &dto)
        .expect("Failed to create collection directory/directories");

    let expected = PathBuf::from("multiple-slash-col-1").join("slash-col-1");
    assert_eq!(col_relpath, expected.to_string_lossy());

    assert!(space_abspath
        .join("parent-col-1/multiple-slash-col-1/slash-col-1")
        .exists());
}

#[test]
fn create_collections_all_relpath_with_only_empty_segments_should_return_error() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();
    std::fs::create_dir_all(space_abspath.join("parent-col-1")).unwrap();

    let dto = CreateCollectionDto {
        parent_relpath: "parent-col-1".into(),
        relpath: "   /   /   ".into(),
    };

    let result = collection::create_collections_all(space_abspath, &dto);
    assert!(result.is_err());
}

#[test]
fn create_collections_all_duplicate_create_collections_should_not_fail() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();
    std::fs::create_dir_all(space_abspath.join("parent-col-1")).unwrap();

    let dto = CreateCollectionDto {
        parent_relpath: "parent-col-1".into(),
        relpath: "Duplicate Col 1/Duplicate Col 2".into(),
    };

    let _ = collection::create_collections_all(space_abspath, &dto)
        .expect("Failed to create collection directory/directories");
    let col_relpath = collection::create_collections_all(space_abspath, &dto)
        .expect("Failed to create collection directory/directories");

    let expected = PathBuf::from("duplicate-col-1").join("duplicate-col-2");
    assert_eq!(col_relpath, expected.to_string_lossy());
}

#[test]
fn create_collections_all_special_characters_should_be_sanitized_and_name_preserved() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();
    std::fs::create_dir_all(space_abspath.join("parent-col-1")).unwrap();

    let dto = CreateCollectionDto {
        parent_relpath: "parent-col-1".into(),
        relpath: "Special@Chars Col 1/Unicode# Col 2/🔥 Emoji Col 3".into(),
    };

    let col_relpath = collection::create_collections_all(space_abspath, &dto)
        .expect("Failed to create collection directory/directories");

    let expected = PathBuf::from("special@chars-col-1")
        .join("unicode#-col-2")
        .join("🔥-emoji-col-3");
    assert_eq!(col_relpath, expected.to_string_lossy());

    let expect_path =
        space_abspath.join("parent-col-1/special@chars-col-1/unicode#-col-2/🔥-emoji-col-3");
    assert!(expect_path.exists());

    let colname = collection::colname_by_relpath(space_abspath)
        .expect("Failed to read collection name mappings");

    let special_chars_key = PathBuf::from("parent-col-1").join("special@chars-col-1");
    assert_eq!(
        colname
            .mappings
            .get(&special_chars_key.to_string_lossy().to_string()),
        Some(&"Special@Chars Col 1".to_string())
    );

    let unicode_chars_key = PathBuf::from("parent-col-1")
        .join("special@chars-col-1")
        .join("unicode#-col-2");
    assert_eq!(
        colname
            .mappings
            .get(&unicode_chars_key.to_string_lossy().to_string()),
        Some(&"Unicode# Col 2".to_string())
    );

    let emoji_key = PathBuf::from("parent-col-1")
        .join("special@chars-col-1")
        .join("unicode#-col-2")
        .join("🔥-emoji-col-3");
    assert_eq!(
        colname
            .mappings
            .get(&emoji_key.to_string_lossy().to_string()),
        Some(&"🔥 Emoji Col 3".to_string())
    );
}

#[test]
fn create_collections_all_unicode_segments_should_be_handled() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();
    std::fs::create_dir_all(space_abspath.join("parent-col-1")).unwrap();

    let dto = CreateCollectionDto {
        parent_relpath: "parent-col-1".into(),
        relpath: "ザク Unicode Col 1/設定 Unicode Col 2".into(),
    };

    let col_relpath = collection::create_collections_all(space_abspath, &dto)
        .expect("Failed to create collection directory/directories");

    let expected = PathBuf::from("ザク-unicode-col-1").join("設定-unicode-col-2");
    assert_eq!(col_relpath, expected.to_string_lossy());
    assert!(space_abspath.join("parent-col-1").join(&expected).exists());
}

#[test]
fn create_collections_all_trailing_slash_should_be_ignored() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();
    std::fs::create_dir_all(space_abspath.join("grand-parent-col-1")).unwrap();

    let dto = CreateCollectionDto {
        parent_relpath: "grand-parent-col-1".into(),
        relpath: "Parent Col 1/Child Trailing Slash Col 2/".into(),
    };

    let col_relpath = collection::create_collections_all(space_abspath, &dto)
        .expect("Failed to create collection directory/directories");

    let expected = PathBuf::from("parent-col-1").join("child-trailing-slash-col-2");
    assert_eq!(col_relpath, expected.to_string_lossy());
    assert!(space_abspath
        .join("grand-parent-col-1")
        .join(&expected)
        .exists());
}

#[test]
fn create_collections_all_invalid_characters_should_be_sanitized() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();
    std::fs::create_dir_all(space_abspath.join("grand-parent-col-1")).unwrap();

    let dto = CreateCollectionDto {
        parent_relpath: "grand-parent-col-1".into(),
        relpath: r#"Parent|Invalid Chars Col 1/Child Col::2"/<Grand>?Child:Invalid*Chars::\Col""3"#
            .into(),
    };

    let col_relpath = collection::create_collections_all(space_abspath, &dto)
        .expect("Failed to create collection directory/directories");

    let expect_relpath = PathBuf::from("parent-invalid-chars-col-1")
        .join("child-col-2")
        .join("grand-child-invalid-chars-col-3");
    assert_eq!(col_relpath, expect_relpath.to_string_lossy());

    let expect_path = space_abspath
        .join("grand-parent-col-1")
        .join(&expect_relpath);
    assert!(expect_path.exists());
}

#[test]
fn create_collection_basic() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_name = "Basic Space";
    let space_dirname = "basic-space";
    let space_abspath = tmp_dir.path().join(space_dirname);

    let dto = CreateSpaceDto {
        name: space_name.into(),
        location: tmp_dir.path().to_string_lossy().into(),
    };

    let mut sharedstate = SharedState::default();
    let spaceref = space::create_space(dto, &mut sharedstate).unwrap();
    assert_eq!(spaceref.name, space_name);

    fs::create_dir_all(space_abspath.join("parent-col-1")).unwrap();

    let collection_dto = CreateCollectionDto {
        parent_relpath: "parent-col-1".into(),
        relpath: "Child Col 1/Grand Child Col 1".into(),
    };

    let col = collection::create_collection(&collection_dto, &mut sharedstate)
        .expect("Failed to create collection");

    let expected = PathBuf::from("parent-col-1")
        .join("child-col-1")
        .join("grand-child-col-1");
    assert_eq!(col.relpath, expected.to_string_lossy());
    assert!(space_abspath
        .join("parent-col-1/child-col-1/grand-child-col-1")
        .exists());
}

#[test]
fn create_collection_empty_relpath_should_fail() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_name = "Empty Space";
    let space_dirname = "empty-space";
    let space_abspath = tmp_dir.path().join(space_dirname);

    let dto = CreateSpaceDto {
        name: space_name.into(),
        location: tmp_dir.path().to_string_lossy().into(),
    };

    let mut sharedstate = SharedState::default();
    let _ = space::create_space(dto, &mut sharedstate).unwrap();

    fs::create_dir_all(space_abspath.join("parent-col-1")).unwrap();

    let collection_dto = CreateCollectionDto {
        parent_relpath: "parent-col-1".into(),
        relpath: "   ".into(),
    };

    let result = collection::create_collection(&collection_dto, &mut sharedstate);
    assert!(matches!(result, Err(Error::FileNotFound(_))));
}

#[test]
fn create_collection_missing_space_should_fail() {
    let collection_dto = CreateCollectionDto {
        parent_relpath: "parent-col-1".into(),
        relpath: "Child Col 1".into(),
    };

    let mut sharedstate = SharedState::default();
    let result = collection::create_collection(&collection_dto, &mut sharedstate);
    assert!(matches!(result, Err(Error::FileNotFound(_))));
}

#[test]
fn create_collection_unicode_path_should_succeed() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_name = "Unicode Space";
    let space_dirname = "unicode-space";
    let space_abspath = tmp_dir.path().join(space_dirname);

    let dto = CreateSpaceDto {
        name: space_name.into(),
        location: tmp_dir.path().to_string_lossy().into(),
    };

    let mut sharedstate = SharedState::default();
    let _ = space::create_space(dto, &mut sharedstate).unwrap();

    fs::create_dir_all(space_abspath.join("parent-col-1")).unwrap();

    let collection_dto = CreateCollectionDto {
        parent_relpath: "parent-col-1".into(),
        relpath: "ザク Unicode Col 1/設定 Unicode Col 2".into(),
    };

    let result = collection::create_collection(&collection_dto, &mut sharedstate)
        .expect("Failed to create collection");

    let expected = PathBuf::from("parent-col-1")
        .join("ザク-unicode-col-1")
        .join("設定-unicode-col-2");
    assert_eq!(result.relpath, expected.to_string_lossy());
    assert!(space_abspath
        .join("parent-col-1/ザク-unicode-col-1/設定-unicode-col-2")
        .exists());
}

#[test]
fn create_collection_should_save_colname() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_name = "Col Name Space";
    let space_dirname = "col-name-space";
    let space_abspath = tmp_dir.path().join(space_dirname);

    let dto = CreateSpaceDto {
        name: space_name.into(),
        location: tmp_dir.path().to_string_lossy().into(),
    };

    let mut sharedstate = SharedState::default();
    let _ = space::create_space(dto, &mut sharedstate).unwrap();

    fs::create_dir_all(space_abspath.join("parent-col-1")).unwrap();

    let collection_dto = CreateCollectionDto {
        parent_relpath: "parent-col-1".into(),
        relpath: "Child Col 1".into(),
    };

    let result = collection::create_collection(&collection_dto, &mut sharedstate)
        .expect("Failed to create collection");

    let colname =
        collection::colname_by_relpath(&space_abspath).expect("Failed to get collection names");

    let expected_key = PathBuf::from("parent-col-1").join("child-col-1");
    assert_eq!(
        colname
            .mappings
            .get(&expected_key.to_string_lossy().to_string()),
        Some(&"Child Col 1".into())
    );

    let expected_relpath = PathBuf::from("parent-col-1").join("child-col-1");
    assert_eq!(result.relpath, expected_relpath.to_string_lossy());
    assert!(space_abspath.join("parent-col-1/child-col-1").exists());
}

#[cfg(windows)]
mod windows {
    use super::*;

    #[test]
    fn create_collections_all_reserved_names_should_fail() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let space_abspath = tmp_dir.path();
        std::fs::create_dir_all(space_abspath.join("parent-col-1")).unwrap();

        let dto = CreateCollectionDto {
            parent_relpath: "parent-col-1".into(),
            relpath: "NUL/Child Col 1".into(),
        };

        let result = collection::create_collections_all(space_abspath, &dto);

        assert!(
            result.is_err(),
            "Expected failure due to reserved name on Windows"
        );
    }
}
