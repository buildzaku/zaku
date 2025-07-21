use std::{collections::HashMap, fs, path::PathBuf};
use tempfile;

use crate::{
    collection::{
        self,
        models::{ColName, CreateCollectionDto},
    },
    error::Error,
    models::SanitizedSegment,
    request::{self, models::CreateRequestDto},
    space::{self, models::CreateSpaceDto},
    state::SharedState,
    utils,
};

fn tmp_space_sharedstate(tmp_path: &std::path::Path) -> SharedState {
    let dto = CreateSpaceDto {
        name: "Col Space".to_string(),
        location: tmp_path.to_string_lossy().to_string(),
    };

    let mut sharedstate = SharedState::default();
    space::create_space(dto, &mut sharedstate).expect("Failed to create test space");

    sharedstate
}

#[test]
fn parse_root_collection_should_match_created_structure() {
    let collections_dto = vec![
        CreateCollectionDto {
            location_relpath: "".into(),
            relpath: "Parent Col 1".into(),
        },
        CreateCollectionDto {
            location_relpath: "".into(),
            relpath: "Parent Col 2".into(),
        },
        CreateCollectionDto {
            location_relpath: "parent-col-2".into(),
            relpath: "Child Col 1/Grand Child Col 1".into(),
        },
        CreateCollectionDto {
            location_relpath: "".into(),
            relpath: "Parent Col 3/Child Col 2".into(),
        },
        CreateCollectionDto {
            location_relpath: "".into(),
            relpath: "Tilde ~~~ Parent Col 1/Back\\Slash Child Col 1  ".into(),
        },
        CreateCollectionDto {
            location_relpath: "".into(),
            relpath:
                "⚠️ ザク Unicode Parent Col 1/🔥 Emoji Child Col 1/💬 Emoji Grand Child Col 1?"
                    .into(),
        },
    ];

    let requests_dto = vec![
        CreateRequestDto {
            location_relpath: "".into(),
            relpath: "Child Req 1".into(),
        },
        CreateRequestDto {
            location_relpath: "".into(),
            relpath: "Parent Col 1/Child Req 2".into(),
        },
        CreateRequestDto {
            location_relpath: "parent-col-1".into(),
            relpath: "Parent Req 1".into(),
        },
        CreateRequestDto {
            location_relpath: "parent-col-2".into(),
            relpath: "Parent Req 2".into(),
        },
        CreateRequestDto {
            location_relpath: "parent-col-2/child-col-1".into(),
            relpath: "Child Req 3".into(),
        },
        CreateRequestDto {
            location_relpath: "parent-col-2/child-col-1/grand-child-col-1".into(),
            relpath: "Grand Child Req 1".into(),
        },
        CreateRequestDto {
            location_relpath: "parent-col-3/child-col-2".into(),
            relpath: "Child Req 4".into(),
        },
        CreateRequestDto {
            location_relpath: "tilde-~~~-parent-col-1/back-slash-child-col-1".into(),
            relpath: "Grand Child Col 1/Special*&Chars Req 1".into(),
        },
        CreateRequestDto {
            location_relpath:
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
    let mut sharedstate = tmp_space_sharedstate(tmp_dir.path());
    let space_abspath = PathBuf::from(&sharedstate.space.as_ref().unwrap().abspath);

    for col_dto in &collections_dto {
        let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
            std::path::Path::new(&col_dto.location_relpath),
            &col_dto.relpath,
            &mut sharedstate,
        )
        .expect("Failed to create parent collections");

        collection::create_collection(&parent_relpath, &col_segment, &mut sharedstate)
            .expect("Failed to create collection");
    }

    for req_dto in &requests_dto {
        let (parent_relpath, req_segment) = collection::create_parent_collections_if_missing(
            std::path::Path::new(&req_dto.location_relpath),
            &req_dto.relpath,
            &mut sharedstate,
        )
        .expect("Failed to create parent collections");

        request::create_req(&parent_relpath, &req_segment, &mut sharedstate)
            .expect("Failed to create request");
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
fn to_sanitized_segments_basic() {
    let segments = utils::to_sanitized_segments("Parent Col 1/Child Col 1/Grand Child Col 1");

    assert_eq!(segments.len(), 3);
    assert_eq!(segments[0].name, "Parent Col 1");
    assert_eq!(segments[0].fsname, "parent-col-1");
    assert_eq!(segments[1].name, "Child Col 1");
    assert_eq!(segments[1].fsname, "child-col-1");
    assert_eq!(segments[2].name, "Grand Child Col 1");
    assert_eq!(segments[2].fsname, "grand-child-col-1");
}

#[test]
fn to_sanitized_segments_empty_relpath() {
    let segments = utils::to_sanitized_segments("   ");
    assert!(segments.is_empty());
}

#[test]
fn to_sanitized_segments_with_whitespace_segments() {
    let segments = utils::to_sanitized_segments("  /Whitespace Child  Col 1       /   ");

    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0].name, "Whitespace Child  Col 1");
    assert_eq!(segments[0].fsname, "whitespace-child-col-1");
}

#[test]
fn to_sanitized_segments_with_multiple_slashes() {
    let segments = utils::to_sanitized_segments("Multiple Slash Col 1///Slash  Col 1");

    assert_eq!(segments.len(), 2);
    assert_eq!(segments[0].name, "Multiple Slash Col 1");
    assert_eq!(segments[0].fsname, "multiple-slash-col-1");
    assert_eq!(segments[1].name, "Slash  Col 1");
    assert_eq!(segments[1].fsname, "slash-col-1");
}

#[test]
fn to_sanitized_segments_with_only_empty_segments() {
    let segments = utils::to_sanitized_segments("   /   /   ");
    assert!(segments.is_empty());
}

#[test]
fn to_sanitized_segments_special_characters() {
    let segments =
        utils::to_sanitized_segments("Special@Chars Col 1/Unicode# Col 2/🔥 Emoji Col 3");

    assert_eq!(segments.len(), 3);
    assert_eq!(segments[0].name, "Special@Chars Col 1");
    assert_eq!(segments[0].fsname, "special@chars-col-1");
    assert_eq!(segments[1].name, "Unicode# Col 2");
    assert_eq!(segments[1].fsname, "unicode#-col-2");
    assert_eq!(segments[2].name, "🔥 Emoji Col 3");
    assert_eq!(segments[2].fsname, "🔥-emoji-col-3");
}

#[test]
fn to_sanitized_segments_unicode() {
    let segments = utils::to_sanitized_segments("ザク Unicode Col 1/設定 Unicode Col 2");

    assert_eq!(segments.len(), 2);
    assert_eq!(segments[0].name, "ザク Unicode Col 1");
    assert_eq!(segments[0].fsname, "ザク-unicode-col-1");
    assert_eq!(segments[1].name, "設定 Unicode Col 2");
    assert_eq!(segments[1].fsname, "設定-unicode-col-2");
}

#[test]
fn to_sanitized_segments_trailing_slash() {
    let segments = utils::to_sanitized_segments("Parent Col 1/Child Trailing Slash Col 2/");

    assert_eq!(segments.len(), 2);
    assert_eq!(segments[0].name, "Parent Col 1");
    assert_eq!(segments[0].fsname, "parent-col-1");
    assert_eq!(segments[1].name, "Child Trailing Slash Col 2");
    assert_eq!(segments[1].fsname, "child-trailing-slash-col-2");
}

#[test]
fn to_sanitized_segments_invalid_characters() {
    let segments = utils::to_sanitized_segments(
        r#"Parent|Invalid Chars Col 1/Child Col::2"/<Grand>?Child:Invalid*Chars::\Col""3"#,
    );

    assert_eq!(segments.len(), 3);
    assert_eq!(segments[0].name, "Parent|Invalid Chars Col 1");
    assert_eq!(segments[0].fsname, "parent-invalid-chars-col-1");
    assert_eq!(segments[1].name, r#"Child Col::2""#);
    assert_eq!(segments[1].fsname, "child-col-2");
    assert_eq!(segments[2].name, r#"<Grand>?Child:Invalid*Chars::-Col""3"#);
    assert_eq!(segments[2].fsname, "grand-child-invalid-chars-col-3");
}

#[test]
fn create_parent_collections_if_missing_basic() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_space_sharedstate(tmp_dir.path());
    let space_abspath = PathBuf::from(&sharedstate.space.as_ref().unwrap().abspath);

    let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
        std::path::Path::new(""),
        "Parent Col 1/Child Col 1/Grand Child Col 1",
        &mut sharedstate,
    )
    .expect("Failed to create parent collections");

    let expected_parent = PathBuf::from("parent-col-1").join("child-col-1");
    assert_eq!(parent_relpath, expected_parent);
    assert_eq!(col_segment.name, "Grand Child Col 1");
    assert_eq!(col_segment.fsname, "grand-child-col-1");

    assert!(space_abspath.join("parent-col-1").exists());
    assert!(space_abspath.join("parent-col-1/child-col-1").exists());
    assert!(!space_abspath
        .join("parent-col-1/child-col-1/grand-child-col-1")
        .exists());
}

#[test]
fn create_parent_collections_if_missing_single_segment() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_space_sharedstate(tmp_dir.path());

    let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
        std::path::Path::new(""),
        "Single Col 1",
        &mut sharedstate,
    )
    .expect("Failed to create parent collections");

    assert_eq!(parent_relpath, PathBuf::from(""));
    assert_eq!(col_segment.name, "Single Col 1");
    assert_eq!(col_segment.fsname, "single-col-1");
}

#[test]
fn create_parent_collections_if_missing_duplicate_should_not_fail() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_space_sharedstate(tmp_dir.path());

    let (parent_relpath1, col_segment1) = collection::create_parent_collections_if_missing(
        std::path::Path::new(""),
        "Duplicate Col 1/Duplicate Col 2",
        &mut sharedstate,
    )
    .expect("Failed to create parent collections first time");

    let (parent_relpath2, col_segment2) = collection::create_parent_collections_if_missing(
        std::path::Path::new(""),
        "Duplicate Col 1/Duplicate Col 2",
        &mut sharedstate,
    )
    .expect("Failed to create parent collections second time");

    assert_eq!(parent_relpath1, parent_relpath2);
    assert_eq!(col_segment1.name, col_segment2.name);
    assert_eq!(col_segment1.fsname, col_segment2.fsname);
}

#[test]
fn create_collection_basic() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_space_sharedstate(tmp_dir.path());
    let space_abspath = PathBuf::from(&sharedstate.space.as_ref().unwrap().abspath);

    fs::create_dir_all(space_abspath.join("parent-col-1")).unwrap();

    let col_segment = SanitizedSegment {
        name: "Child Col 1".to_string(),
        fsname: "child-col-1".to_string(),
    };

    let result = collection::create_collection(
        std::path::Path::new("parent-col-1"),
        &col_segment,
        &mut sharedstate,
    )
    .expect("Failed to create collection");

    let expected_relpath = PathBuf::from("parent-col-1").join("child-col-1");
    assert_eq!(result.relpath, expected_relpath.to_string_lossy());
    assert!(space_abspath.join("parent-col-1/child-col-1").exists());
}

#[test]
fn create_collection_empty_fsname_should_fail() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_space_sharedstate(tmp_dir.path());

    let col_segment = SanitizedSegment {
        name: "Empty Col 1".to_string(),
        fsname: "   ".to_string(),
    };

    let result = collection::create_collection(
        std::path::Path::new("parent-col-1"),
        &col_segment,
        &mut sharedstate,
    );
    assert!(matches!(result, Err(Error::FileNotFound(_))));
}

#[test]
fn create_collection_missing_space_should_fail() {
    let col_segment = SanitizedSegment {
        name: "Child Col 1".to_string(),
        fsname: "child-col-1".to_string(),
    };

    let mut sharedstate = SharedState::default();
    let result = collection::create_collection(
        std::path::Path::new("parent-col-1"),
        &col_segment,
        &mut sharedstate,
    );
    assert!(matches!(result, Err(Error::FileNotFound(_))));
}

#[test]
fn create_collection_unicode_path_should_succeed() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_space_sharedstate(tmp_dir.path());
    let space_abspath = PathBuf::from(&sharedstate.space.as_ref().unwrap().abspath);

    fs::create_dir_all(space_abspath.join("parent-col-1")).unwrap();

    let col_segment = SanitizedSegment {
        name: "ザク Unicode Col 1".to_string(),
        fsname: "ザク-unicode-col-1".to_string(),
    };

    let result = collection::create_collection(
        std::path::Path::new("parent-col-1"),
        &col_segment,
        &mut sharedstate,
    )
    .expect("Failed to create collection");

    let expected_relpath = PathBuf::from("parent-col-1").join("ザク-unicode-col-1");
    assert_eq!(result.relpath, expected_relpath.to_string_lossy());
    assert!(space_abspath
        .join("parent-col-1/ザク-unicode-col-1")
        .exists());
}

#[test]
fn create_collection_should_save_colname() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_space_sharedstate(tmp_dir.path());
    let space_abspath = PathBuf::from(&sharedstate.space.as_ref().unwrap().abspath);

    fs::create_dir_all(space_abspath.join("parent-col-1")).unwrap();

    let col_segment = SanitizedSegment {
        name: "Child Col 1".to_string(),
        fsname: "child-col-1".to_string(),
    };

    let result = collection::create_collection(
        std::path::Path::new("parent-col-1"),
        &col_segment,
        &mut sharedstate,
    )
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

#[test]
fn create_collection_integrated_flow() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_space_sharedstate(tmp_dir.path());
    let space_abspath = PathBuf::from(&sharedstate.space.as_ref().unwrap().abspath);

    let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
        std::path::Path::new(""),
        "Parent Col 1/Child Col 1/Grand Child Col 1",
        &mut sharedstate,
    )
    .expect("Failed to create parent collections");

    let result = collection::create_collection(&parent_relpath, &col_segment, &mut sharedstate)
        .expect("Failed to create collection");

    let expected_relpath = PathBuf::from("parent-col-1")
        .join("child-col-1")
        .join("grand-child-col-1");
    assert_eq!(result.relpath, expected_relpath.to_string_lossy());

    assert!(space_abspath.join("parent-col-1").exists());
    assert!(space_abspath.join("parent-col-1/child-col-1").exists());
    assert!(space_abspath
        .join("parent-col-1/child-col-1/grand-child-col-1")
        .exists());

    let colname = collection::colname_by_relpath(&space_abspath)
        .expect("Failed to read collection name mappings");

    assert_eq!(
        colname.mappings.get("parent-col-1"),
        Some(&"Parent Col 1".to_string())
    );
    assert_eq!(
        colname.mappings.get("parent-col-1/child-col-1"),
        Some(&"Child Col 1".to_string())
    );
    assert_eq!(
        colname
            .mappings
            .get("parent-col-1/child-col-1/grand-child-col-1"),
        Some(&"Grand Child Col 1".to_string())
    );
}

#[cfg(windows)]
mod windows {
    use super::*;

    #[test]
    fn to_sanitized_segments_reserved_names_should_be_handled() {
        let segments = utils::to_sanitized_segments("NUL/Child Col 1");

        if segments.is_empty() || segments[0].fsname != "nul" {
            // Test passes if segments are handled appropriately
        } else {
            panic!("Reserved names should be handled on Windows");
        }
    }
}
