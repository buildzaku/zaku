use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::PathBuf,
};

use crate::{
    collection::{
        self,
        models::{CreateCollectionDto, SpaceCollectionsMetadata, SpaceCollectionsMetadataStore},
    },
    error::Error,
    models::SanitizedSegment,
    request::{self, models::CreateRequestDto},
    store::{self},
    utils,
};

#[test]
fn parse_root_collection_should_match_created_structure() {
    let collections_dto = vec![
        CreateCollectionDto {
            location_relpath: PathBuf::from(""),
            relpath: PathBuf::from("Parent Col 1"),
        },
        CreateCollectionDto {
            location_relpath: PathBuf::from(""),
            relpath: PathBuf::from("Parent Col 2"),
        },
        CreateCollectionDto {
            location_relpath: PathBuf::from("parent-col-2"),
            relpath: PathBuf::from("Child Col 1/Grand Child Col 1"),
        },
        CreateCollectionDto {
            location_relpath: PathBuf::from(""),
            relpath: PathBuf::from("Parent Col 3/Child Col 2"),
        },
        CreateCollectionDto {
            location_relpath: PathBuf::from(""),
            relpath: PathBuf::from("Tilde ~~~ Parent Col 1/Back\\Slash Child Col 1  "),
        },
        CreateCollectionDto {
            location_relpath: PathBuf::from(""),
            relpath: PathBuf::from(
                "⚠️ ザク Unicode Parent Col 1/🔥 Emoji Child Col 1/💬 Emoji Grand Child Col 1?",
            ),
        },
    ];

    let requests_dto = vec![
        CreateRequestDto {
            location_relpath: PathBuf::from(""),
            relpath: PathBuf::from("Child Req 1"),
        },
        CreateRequestDto {
            location_relpath: PathBuf::from(""),
            relpath: PathBuf::from("Parent Col 1/Child Req 2"),
        },
        CreateRequestDto {
            location_relpath: PathBuf::from("parent-col-1"),
            relpath: PathBuf::from("Parent Req 1"),
        },
        CreateRequestDto {
            location_relpath: PathBuf::from("parent-col-2"),
            relpath: PathBuf::from("Parent Req 2"),
        },
        CreateRequestDto {
            location_relpath: PathBuf::from("parent-col-2/child-col-1"),
            relpath: PathBuf::from("Child Req 3"),
        },
        CreateRequestDto {
            location_relpath: PathBuf::from("parent-col-2/child-col-1/grand-child-col-1"),
            relpath: PathBuf::from("Grand Child Req 1"),
        },
        CreateRequestDto {
            location_relpath: PathBuf::from("parent-col-3/child-col-2"),
            relpath: PathBuf::from("Child Req 4"),
        },
        CreateRequestDto {
            location_relpath: PathBuf::from("tilde-parent-col-1/back-slash-child-col-1"),
            relpath: PathBuf::from("Grand Child Col 1/Special*&Chars Req 1"),
        },
        CreateRequestDto {
            location_relpath: PathBuf::from(
                "ザク-unicode-parent-col-1/emoji-child-col-1/emoji-grand-child-col-1",
            ),
            relpath: PathBuf::from("💡Emoji Special: Col 1/*>?Special Chars Req 1"),
        },
    ];

    let expected_colnames: HashMap<PathBuf, &'static str> = HashMap::from([
        (PathBuf::from("parent-col-1"), "Parent Col 1"),
        (PathBuf::from("parent-col-2"), "Parent Col 2"),
        (
            PathBuf::from("parent-col-2")
                .join("child-col-1")
                .join("grand-child-col-1"),
            "Grand Child Col 1",
        ),
        (
            PathBuf::from("parent-col-3").join("child-col-2"),
            "Child Col 2",
        ),
        (
            PathBuf::from("tilde-parent-col-1").join("back-slash-child-col-1"),
            "Back-Slash Child Col 1",
        ),
        (
            PathBuf::from("ザク-unicode-parent-col-1")
                .join("emoji-child-col-1")
                .join("emoji-grand-child-col-1"),
            "💬 Emoji Grand Child Col 1?",
        ),
    ]);

    let expected_reqnames: HashMap<PathBuf, &'static str> = HashMap::from([
        (PathBuf::from("child-req-1.toml"), "Child Req 1"),
        (
            PathBuf::from("parent-col-1").join("child-req-2.toml"),
            "Child Req 2",
        ),
        (
            PathBuf::from("parent-col-1").join("parent-req-1.toml"),
            "Parent Req 1",
        ),
        (
            PathBuf::from("parent-col-2").join("parent-req-2.toml"),
            "Parent Req 2",
        ),
        (
            PathBuf::from("parent-col-2")
                .join("child-col-1")
                .join("child-req-3.toml"),
            "Child Req 3",
        ),
        (
            PathBuf::from("parent-col-2")
                .join("child-col-1")
                .join("grand-child-col-1")
                .join("grand-child-req-1.toml"),
            "Grand Child Req 1",
        ),
        (
            PathBuf::from("parent-col-3")
                .join("child-col-2")
                .join("child-req-4.toml"),
            "Child Req 4",
        ),
        (
            PathBuf::from("tilde-parent-col-1")
                .join("back-slash-child-col-1")
                .join("grand-child-col-1")
                .join("special-chars-req-1.toml"),
            "Special*&Chars Req 1",
        ),
        (
            PathBuf::from("ザク-unicode-parent-col-1")
                .join("emoji-child-col-1")
                .join("emoji-grand-child-col-1")
                .join("emoji-special-col-1")
                .join("special-chars-req-1.toml"),
            "*>?Special Chars Req 1",
        ),
    ]);

    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Col Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    for col_dto in &collections_dto {
        let location_relpath = PathBuf::from(&col_dto.location_relpath);
        let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
            &location_relpath,
            &col_dto.relpath,
            &tmp_space_abspath,
        )
        .expect("Failed to create parent collections");

        collection::create_collection(&parent_relpath, &col_segment, &tmp_space_abspath)
            .expect("Failed to create collection");
    }

    for req_dto in &requests_dto {
        let location_relpath = PathBuf::from(&req_dto.location_relpath);
        let (parent_relpath, req_segment) = collection::create_parent_collections_if_missing(
            &location_relpath,
            &req_dto.relpath,
            &tmp_space_abspath,
        )
        .expect("Failed to create parent collections");

        request::create_req(&parent_relpath, &req_segment, &tmp_space_abspath)
            .expect("Failed to create request");
    }

    let root_collection = collection::parse_root_collection(&tmp_space_abspath, &state_store)
        .expect("Failed to parse root collection");

    let mut stack = vec![(&root_collection, String::new())];

    while let Some((collection, current_path)) = stack.pop() {
        if !current_path.is_empty() {
            let current_pathbuf = PathBuf::from(&current_path);
            if let Some(expected_name) = expected_colnames.get(&current_pathbuf) {
                assert_eq!(
                    collection.meta.name.as_deref(),
                    Some(*expected_name),
                    "Collection name mismatch at '{current_path}'"
                );
            }
        }

        for req in &collection.requests {
            let req_pathbuf = if current_path.is_empty() {
                PathBuf::from(&req.meta.fsname)
            } else {
                PathBuf::from(&current_path).join(&req.meta.fsname)
            };

            let expected_name = expected_reqnames
                .get(&req_pathbuf)
                .unwrap_or_else(|| panic!("Unexpected request: {}", req_pathbuf.display()));
            assert_eq!(
                req.meta.name,
                *expected_name,
                "Request name mismatch at '{}'",
                req_pathbuf.display()
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
fn collections_metadata_reads_existing_data() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Col Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let collection_relpath = PathBuf::from("parent-col-1/child-col-1");
    let toml_path = store::utils::scmt_store_abspath(&tmp_space_abspath);

    std::fs::create_dir_all(toml_path.parent().unwrap()).unwrap();

    let mut mappings = BTreeMap::new();
    mappings.insert(collection_relpath.clone(), "Child Col 1".to_string());

    let metadata = SpaceCollectionsMetadata { mappings };
    let toml_content = toml::to_string_pretty(&metadata).unwrap();
    std::fs::write(&toml_path, toml_content).unwrap();

    let scmt_store = SpaceCollectionsMetadataStore::get(&tmp_space_abspath).unwrap();

    assert_eq!(
        scmt_store.mappings.get(&collection_relpath),
        Some(&"Child Col 1".to_string())
    );
}

#[test]
fn collections_metadata_invalid_toml_creates_default_store() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Col Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let scmt_abspath = store::utils::scmt_store_abspath(&tmp_space_abspath);
    fs::create_dir_all(scmt_abspath.parent().unwrap()).unwrap();

    let invalid_toml = "[mappings\n\"parent-col-1/child-col-1\" = \"Child Col 1\"";
    fs::write(&scmt_abspath, invalid_toml).unwrap();

    let result = SpaceCollectionsMetadataStore::get(&tmp_space_abspath);
    assert!(result.is_ok());
    // Should create default store when invalid TOML is encountered
    let store = result.unwrap();
    assert!(store.mappings.is_empty());
}

#[test]
fn collections_metadata_creates_file_if_missing() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Col Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let scmt_store = SpaceCollectionsMetadataStore::get(&tmp_space_abspath).unwrap();
    let name = scmt_store.mappings.get(&PathBuf::from("any-path"));
    assert_eq!(name, None);

    let scmt_abspath = store::utils::scmt_store_abspath(&tmp_space_abspath);
    assert!(scmt_abspath.exists());

    let content = fs::read_to_string(scmt_abspath).unwrap();
    assert_eq!(content.trim(), "[mappings]");
}

#[test]
fn create_parent_collections_if_missing_basic() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Col Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let location_relpath = PathBuf::from("");
    let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
        &location_relpath,
        &PathBuf::from("Parent Col 1/Child Col 1/Grand Child Col 1"),
        &tmp_space_abspath,
    )
    .expect("Failed to create parent collections");

    assert_eq!(
        parent_relpath,
        PathBuf::from("parent-col-1").join("child-col-1")
    );
    assert_eq!(col_segment.name, "Grand Child Col 1");
    assert_eq!(col_segment.fsname, "grand-child-col-1");

    assert!(tmp_space_abspath.join("parent-col-1").exists());
    assert!(
        tmp_space_abspath
            .join("parent-col-1")
            .join("child-col-1")
            .exists()
    );
    assert!(
        !tmp_space_abspath
            .join("parent-col-1")
            .join("child-col-1")
            .join("grand-child-col-1")
            .exists()
    );
}

#[test]
fn create_parent_collections_if_missing_with_nested_backslash() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Col Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
        &PathBuf::from(""),
        &PathBuf::from("Parent Col 1/Child Col 1\\Grand Child Col 1"),
        &tmp_space_abspath,
    )
    .expect("Failed to create parent collections");

    assert_eq!(parent_relpath, PathBuf::from("parent-col-1"));
    assert_eq!(col_segment.name, "Child Col 1-Grand Child Col 1");
    assert_eq!(col_segment.fsname, "child-col-1-grand-child-col-1");

    assert!(tmp_space_abspath.join("parent-col-1").exists());
}

#[test]
fn create_parent_collections_if_missing_single_segment() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Col Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let location_relpath = PathBuf::from("");
    let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
        &location_relpath,
        &PathBuf::from("Single Col 1"),
        &tmp_space_abspath,
    )
    .expect("Failed to create parent collections");

    assert_eq!(parent_relpath, PathBuf::from(""));
    assert_eq!(col_segment.name, "Single Col 1");
    assert_eq!(col_segment.fsname, "single-col-1");
}

#[test]
fn create_parent_collections_if_missing_duplicate_should_not_fail() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Col Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let location_relpath = PathBuf::from("");
    let (parent_relpath1, col_segment1) = collection::create_parent_collections_if_missing(
        &location_relpath,
        &PathBuf::from("Duplicate Col 1/Duplicate Col 2"),
        &tmp_space_abspath,
    )
    .expect("Failed to create parent collections first time");

    let location_relpath = PathBuf::from("");
    let (parent_relpath2, col_segment2) = collection::create_parent_collections_if_missing(
        &location_relpath,
        &PathBuf::from("Duplicate Col 1/Duplicate Col 2"),
        &tmp_space_abspath,
    )
    .expect("Failed to create parent collections second time");

    assert_eq!(parent_relpath1, parent_relpath2);
    assert_eq!(col_segment1.name, col_segment2.name);
    assert_eq!(col_segment1.fsname, col_segment2.fsname);
}

#[test]
fn create_parent_collections_if_missing_special_chars_only_should_fail() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Col Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let location_relpath = PathBuf::from("");
    let result = collection::create_parent_collections_if_missing(
        &location_relpath,
        &PathBuf::from("Parent Col 1/!@#$%/Grand Child Col 1"),
        &tmp_space_abspath,
    );

    assert!(matches!(result, Err(Error::SanitizationError(_))));
}

#[test]
fn create_collection_basic() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Col Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    fs::create_dir_all(tmp_space_abspath.join("parent-col-1")).unwrap();

    let col_segment = SanitizedSegment {
        name: "Child Col 1".to_string(),
        fsname: "child-col-1".to_string(),
    };

    let result = collection::create_collection(
        &PathBuf::from("parent-col-1"),
        &col_segment,
        &tmp_space_abspath,
    )
    .expect("Failed to create collection");

    assert_eq!(
        result.relpath,
        PathBuf::from("parent-col-1").join("child-col-1")
    );
    assert!(
        tmp_space_abspath
            .join("parent-col-1")
            .join("child-col-1")
            .exists()
    );
}

#[test]
fn create_collection_empty_fsname_should_fail() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Col Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let col_segment = SanitizedSegment {
        name: "Empty Col 1".to_string(),
        fsname: "   ".to_string(),
    };

    let result = collection::create_collection(
        &PathBuf::from("parent-col-1"),
        &col_segment,
        &tmp_space_abspath,
    );
    assert!(matches!(result, Err(Error::InvalidName(_))));
}

#[test]
fn create_collection_missing_space_should_fail() {
    let col_segment = SanitizedSegment {
        name: "Child Col 1".to_string(),
        fsname: "child-col-1".to_string(),
    };

    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();
    let result =
        collection::create_collection(&PathBuf::from("parent-col-1"), &col_segment, space_abspath);
    assert!(matches!(result, Err(Error::Io(_))));
}

#[test]
fn create_collection_unicode_path_should_succeed() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Col Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    fs::create_dir_all(tmp_space_abspath.join("parent-col-1")).unwrap();

    let col_segment = SanitizedSegment {
        name: "ザク Unicode Col 1".to_string(),
        fsname: "ザク-unicode-col-1".to_string(),
    };

    let result = collection::create_collection(
        &PathBuf::from("parent-col-1"),
        &col_segment,
        &tmp_space_abspath,
    )
    .expect("Failed to create collection");

    assert_eq!(
        result.relpath,
        PathBuf::from("parent-col-1").join("ザク-unicode-col-1")
    );
    assert!(
        tmp_space_abspath
            .join("parent-col-1")
            .join("ザク-unicode-col-1")
            .exists()
    );
}

#[test]
fn create_collection_should_save_collections_metadata() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Col Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    fs::create_dir_all(tmp_space_abspath.join("parent-col-1")).unwrap();

    let col_segment = SanitizedSegment {
        name: "Child Col 1".to_string(),
        fsname: "child-col-1".to_string(),
    };

    let result = collection::create_collection(
        &PathBuf::from("parent-col-1"),
        &col_segment,
        &tmp_space_abspath,
    )
    .expect("Failed to create collection");

    let scmt_store = SpaceCollectionsMetadataStore::get(&tmp_space_abspath).unwrap();
    let expected_relpath = PathBuf::from("parent-col-1").join("child-col-1");

    assert_eq!(
        scmt_store.mappings.get(&expected_relpath),
        Some(&"Child Col 1".to_string())
    );
    assert_eq!(result.relpath, expected_relpath);
    assert!(
        tmp_space_abspath
            .join("parent-col-1")
            .join("child-col-1")
            .exists()
    );
}

#[test]
fn create_collection_integrated_flow() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Col Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let location_relpath = PathBuf::from("");
    let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
        &location_relpath,
        &PathBuf::from("Parent Col 1/Child Col 1/Grand Child Col 1"),
        &tmp_space_abspath,
    )
    .expect("Failed to create parent collections");

    let result = collection::create_collection(&parent_relpath, &col_segment, &tmp_space_abspath)
        .expect("Failed to create collection");

    assert_eq!(
        result.relpath,
        PathBuf::from("parent-col-1")
            .join("child-col-1")
            .join("grand-child-col-1")
    );

    assert!(tmp_space_abspath.join("parent-col-1").exists());
    assert!(
        tmp_space_abspath
            .join("parent-col-1")
            .join("child-col-1")
            .exists()
    );
    assert!(
        tmp_space_abspath
            .join("parent-col-1")
            .join("child-col-1")
            .join("grand-child-col-1")
            .exists()
    );

    let scmt_store = SpaceCollectionsMetadataStore::get(&tmp_space_abspath).unwrap();

    let parent_relpath = PathBuf::from("parent-col-1");
    let child_relpath = PathBuf::from("parent-col-1/child-col-1");
    let grandchild_relpath = PathBuf::from("parent-col-1/child-col-1/grand-child-col-1");

    assert_eq!(
        scmt_store.mappings.get(&parent_relpath),
        Some(&"Parent Col 1".to_string())
    );
    assert_eq!(
        scmt_store.mappings.get(&child_relpath),
        Some(&"Child Col 1".to_string())
    );
    assert_eq!(
        scmt_store.mappings.get(&grandchild_relpath),
        Some(&"Grand Child Col 1".to_string())
    );
}
