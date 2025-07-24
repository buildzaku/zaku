use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use tempfile;

use crate::{
    collection::{self, models::CreateCollectionDto, ColName},
    error::Error,
    models::SanitizedSegment,
    request::{self, models::CreateRequestDto},
    state::SharedState,
    store::{self, UserSettingsStore},
    utils,
};

#[test]
fn parse_root_collection_should_match_created_structure() {
    let collections_dto = vec![
        CreateCollectionDto {
            location_relpath: PathBuf::from(""),
            relpath: "Parent Col 1".into(),
        },
        CreateCollectionDto {
            location_relpath: PathBuf::from(""),
            relpath: "Parent Col 2".into(),
        },
        CreateCollectionDto {
            location_relpath: PathBuf::from("parent-col-2"),
            relpath: "Child Col 1/Grand Child Col 1".into(),
        },
        CreateCollectionDto {
            location_relpath: PathBuf::from(""),
            relpath: "Parent Col 3/Child Col 2".into(),
        },
        CreateCollectionDto {
            location_relpath: PathBuf::from(""),
            relpath: "Tilde ~~~ Parent Col 1/Back\\Slash Child Col 1  ".into(),
        },
        CreateCollectionDto {
            location_relpath: PathBuf::from(""),
            relpath:
                "⚠️ ザク Unicode Parent Col 1/🔥 Emoji Child Col 1/💬 Emoji Grand Child Col 1?"
                    .into(),
        },
    ];

    let requests_dto = vec![
        CreateRequestDto {
            location_relpath: PathBuf::from(""),
            relpath: "Child Req 1".into(),
        },
        CreateRequestDto {
            location_relpath: PathBuf::from(""),
            relpath: "Parent Col 1/Child Req 2".into(),
        },
        CreateRequestDto {
            location_relpath: PathBuf::from("parent-col-1"),
            relpath: "Parent Req 1".into(),
        },
        CreateRequestDto {
            location_relpath: PathBuf::from("parent-col-2"),
            relpath: "Parent Req 2".into(),
        },
        CreateRequestDto {
            location_relpath: PathBuf::from("parent-col-2").join("child-col-1"),
            relpath: "Child Req 3".into(),
        },
        CreateRequestDto {
            location_relpath: PathBuf::from("parent-col-2")
                .join("child-col-1")
                .join("grand-child-col-1"),
            relpath: "Grand Child Req 1".into(),
        },
        CreateRequestDto {
            location_relpath: PathBuf::from("parent-col-3").join("child-col-2"),
            relpath: "Child Req 4".into(),
        },
        CreateRequestDto {
            location_relpath: PathBuf::from("tilde-parent-col-1").join("back-slash-child-col-1"),
            relpath: "Grand Child Col 1/Special*&Chars Req 1".into(),
        },
        CreateRequestDto {
            location_relpath: PathBuf::from("ザク-unicode-parent-col-1")
                .join("emoji-child-col-1")
                .join("emoji-grand-child-col-1"),
            relpath: "💡Emoji Special: Col 1/*>?Special Chars Req 1".into(),
        },
    ];

    let expected_colnames = HashMap::from([
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

    let expected_reqnames = HashMap::from([
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

    let tmp_dir = tempfile::tempdir().unwrap();
    let (mut sharedstate, _store, _space_ref) =
        store::utils::tmp_space("Col Space", tmp_dir.path());
    let space_abspath = PathBuf::from(&sharedstate.space.as_ref().unwrap().abspath);

    for col_dto in &collections_dto {
        let location_relpath = Path::new(&col_dto.location_relpath);
        let relpath = &col_dto.relpath;
        let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
            location_relpath,
            relpath,
            &mut sharedstate,
        )
        .expect("Failed to create parent collections");

        collection::create_collection(&parent_relpath, &col_segment, &mut sharedstate)
            .expect("Failed to create collection");
    }

    for req_dto in &requests_dto {
        let location_relpath = Path::new(&req_dto.location_relpath);
        let relpath = &req_dto.relpath;
        let (parent_relpath, req_segment) = collection::create_parent_collections_if_missing(
            location_relpath,
            relpath,
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
fn colname_reads_existing_data() {
    let tmp_dir = tempfile::tempdir().expect("Failed to create temporary directory");
    let space_abspath = tmp_dir.path();

    let collection_relpath = PathBuf::from("parent-col-1").join("child-col-1");

    let mut colnames = ColName::load(space_abspath).unwrap();
    colnames.set(&collection_relpath, "Child Col 1").unwrap();

    assert_eq!(
        colnames.get(&collection_relpath),
        Some("Child Col 1".to_string())
    );
}

#[test]
fn colname_invalid_toml_should_fail() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();

    let colname_filepath = space_abspath
        .join(".zaku")
        .join("collections")
        .join("name.toml");
    fs::create_dir_all(colname_filepath.parent().unwrap()).unwrap();

    let invalid_toml = "[mappings\n\"parent-col-1/child-col-1\" = \"Child Col 1\"";
    fs::write(&colname_filepath, invalid_toml).unwrap();

    let result = ColName::load(space_abspath);
    assert!(result.is_err());
}

#[test]
fn colname_creates_file_if_missing() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();

    let colnames = ColName::load(space_abspath).unwrap();
    let name = colnames.get(&PathBuf::from("any-path"));
    assert_eq!(name, None);

    let file_path = space_abspath
        .join(".zaku")
        .join("collections")
        .join("name.toml");
    assert!(file_path.exists());

    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content.trim(), "[mappings]");
}

#[test]
fn colname_set_if_missing_writes_new_entry() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();

    let collection_relpath = PathBuf::from("parent-col-1").join("child-col-1");

    let mut colnames = ColName::load(space_abspath).unwrap();
    colnames.set(&collection_relpath, "Child Col 1").unwrap();

    let name = colnames.get(&collection_relpath);
    assert_eq!(name, Some("Child Col 1".to_string()));
}

#[test]
fn colname_set_if_missing_does_not_overwrite_existing() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let space_abspath = tmp_dir.path();

    let collection_relpath = PathBuf::from("parent-col-1").join("child-col-1");

    let mut colnames = ColName::load(space_abspath).unwrap();
    colnames
        .set(&collection_relpath, "Child Col 1 - First Name")
        .unwrap();
    colnames
        .set(&collection_relpath, "Child Col 1 - Second Name")
        .unwrap();

    let name = colnames.get(&collection_relpath);
    assert_eq!(name, Some("Child Col 1 - First Name".to_string()));
}

#[test]
fn create_parent_collections_if_missing_basic() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (mut sharedstate, _store, _space_ref) =
        store::utils::tmp_space("Col Space", tmp_dir.path());
    let space_abspath = PathBuf::from(&sharedstate.space.as_ref().unwrap().abspath);

    let location_relpath = Path::new("");
    let relpath = "Parent Col 1/Child Col 1/Grand Child Col 1";
    let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
        location_relpath,
        relpath,
        &mut sharedstate,
    )
    .expect("Failed to create parent collections");

    let expected_parent = PathBuf::from("parent-col-1").join("child-col-1");
    assert_eq!(parent_relpath, expected_parent);
    assert_eq!(col_segment.name, "Grand Child Col 1");
    assert_eq!(col_segment.fsname, "grand-child-col-1");

    assert!(space_abspath.join("parent-col-1").exists());
    assert!(space_abspath
        .join("parent-col-1")
        .join("child-col-1")
        .exists());
    assert!(!space_abspath
        .join("parent-col-1")
        .join("child-col-1")
        .join("grand-child-col-1")
        .exists());
}

#[test]
fn create_parent_collections_if_missing_single_segment() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (mut sharedstate, _store, _space_ref) =
        store::utils::tmp_space("Col Space", tmp_dir.path());

    let location_relpath = Path::new("");
    let relpath = "Single Col 1";
    let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
        location_relpath,
        relpath,
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
    let (mut sharedstate, _store, _space_ref) =
        store::utils::tmp_space("Col Space", tmp_dir.path());

    let location_relpath = Path::new("");
    let relpath = "Duplicate Col 1/Duplicate Col 2";
    let (parent_relpath1, col_segment1) = collection::create_parent_collections_if_missing(
        location_relpath,
        relpath,
        &mut sharedstate,
    )
    .expect("Failed to create parent collections first time");

    let location_relpath = Path::new("");
    let relpath = "Duplicate Col 1/Duplicate Col 2";
    let (parent_relpath2, col_segment2) = collection::create_parent_collections_if_missing(
        location_relpath,
        relpath,
        &mut sharedstate,
    )
    .expect("Failed to create parent collections second time");

    assert_eq!(parent_relpath1, parent_relpath2);
    assert_eq!(col_segment1.name, col_segment2.name);
    assert_eq!(col_segment1.fsname, col_segment2.fsname);
}

#[test]
fn create_parent_collections_if_missing_special_chars_only_should_fail() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (mut sharedstate, _store, _space_ref) =
        store::utils::tmp_space("Col Space", tmp_dir.path());

    let location_relpath = Path::new("");
    let relpath = "Parent Col 1/!@#$%/Grand Child Col 1";
    let result = collection::create_parent_collections_if_missing(
        location_relpath,
        relpath,
        &mut sharedstate,
    );

    assert!(matches!(result, Err(Error::SanitizationError(_))));
}

#[test]
fn create_collection_basic() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (mut sharedstate, _store, _space_ref) =
        store::utils::tmp_space("Col Space", tmp_dir.path());
    let space_abspath = PathBuf::from(&sharedstate.space.as_ref().unwrap().abspath);

    fs::create_dir_all(space_abspath.join("parent-col-1")).unwrap();

    let col_segment = SanitizedSegment {
        name: "Child Col 1".to_string(),
        fsname: "child-col-1".to_string(),
    };

    let result =
        collection::create_collection(Path::new("parent-col-1"), &col_segment, &mut sharedstate)
            .expect("Failed to create collection");

    let expected_relpath = PathBuf::from("parent-col-1").join("child-col-1");
    assert_eq!(result.relpath, expected_relpath.to_string_lossy());
    assert!(space_abspath
        .join("parent-col-1")
        .join("child-col-1")
        .exists());
}

#[test]
fn create_collection_empty_fsname_should_fail() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (mut sharedstate, _store, _space_ref) =
        store::utils::tmp_space("Col Space", tmp_dir.path());

    let col_segment = SanitizedSegment {
        name: "Empty Col 1".to_string(),
        fsname: "   ".to_string(),
    };

    let result =
        collection::create_collection(Path::new("parent-col-1"), &col_segment, &mut sharedstate);
    assert!(matches!(result, Err(Error::FileNotFound(_))));
}

#[test]
fn create_collection_missing_space_should_fail() {
    let col_segment = SanitizedSegment {
        name: "Child Col 1".to_string(),
        fsname: "child-col-1".to_string(),
    };

    let tmp_dir = tempfile::tempdir().unwrap();
    let ust_store_abspath = store::utils::ust_store_abspath(tmp_dir.path());
    let user_settings = UserSettingsStore::get(&ust_store_abspath)
        .expect("Failed to init user settings")
        .into_inner();

    let mut sharedstate = SharedState {
        space: None,
        spacerefs: Vec::new(),
        user_settings,
    };
    let result =
        collection::create_collection(Path::new("parent-col-1"), &col_segment, &mut sharedstate);
    assert!(matches!(result, Err(Error::FileNotFound(_))));
}

#[test]
fn create_collection_unicode_path_should_succeed() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (mut sharedstate, _store, _space_ref) =
        store::utils::tmp_space("Col Space", tmp_dir.path());
    let space_abspath = PathBuf::from(&sharedstate.space.as_ref().unwrap().abspath);

    fs::create_dir_all(space_abspath.join("parent-col-1")).unwrap();

    let col_segment = SanitizedSegment {
        name: "ザク Unicode Col 1".to_string(),
        fsname: "ザク-unicode-col-1".to_string(),
    };

    let result =
        collection::create_collection(Path::new("parent-col-1"), &col_segment, &mut sharedstate)
            .expect("Failed to create collection");

    let expected_relpath = PathBuf::from("parent-col-1").join("ザク-unicode-col-1");
    assert_eq!(result.relpath, expected_relpath.to_string_lossy());
    assert!(space_abspath
        .join("parent-col-1")
        .join("ザク-unicode-col-1")
        .exists());
}

#[test]
fn create_collection_should_save_colname() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (mut sharedstate, _store, _space_ref) =
        store::utils::tmp_space("Col Space", tmp_dir.path());
    let space_abspath = PathBuf::from(&sharedstate.space.as_ref().unwrap().abspath);

    fs::create_dir_all(space_abspath.join("parent-col-1")).unwrap();

    let col_segment = SanitizedSegment {
        name: "Child Col 1".to_string(),
        fsname: "child-col-1".to_string(),
    };

    let result =
        collection::create_collection(Path::new("parent-col-1"), &col_segment, &mut sharedstate)
            .expect("Failed to create collection");

    let colnames = ColName::load(&space_abspath).unwrap();
    let expected_relpath = PathBuf::from("parent-col-1").join("child-col-1");

    assert_eq!(
        colnames.get(&expected_relpath),
        Some("Child Col 1".to_string())
    );
    assert_eq!(result.relpath, expected_relpath.to_string_lossy());
    assert!(space_abspath
        .join("parent-col-1")
        .join("child-col-1")
        .exists());
}

#[test]
fn create_collection_integrated_flow() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (mut sharedstate, _store, _space_ref) =
        store::utils::tmp_space("Col Space", tmp_dir.path());
    let space_abspath = PathBuf::from(&sharedstate.space.as_ref().unwrap().abspath);

    let location_relpath = Path::new("");
    let relpath = "Parent Col 1/Child Col 1/Grand Child Col 1";
    let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
        location_relpath,
        relpath,
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
    assert!(space_abspath
        .join("parent-col-1")
        .join("child-col-1")
        .exists());
    assert!(space_abspath
        .join("parent-col-1")
        .join("child-col-1")
        .join("grand-child-col-1")
        .exists());

    let colnames = ColName::load(&space_abspath).unwrap();

    let parent_relpath = PathBuf::from("parent-col-1");
    let child_relpath = PathBuf::from("parent-col-1").join("child-col-1");
    let grandchild_relpath = PathBuf::from("parent-col-1")
        .join("child-col-1")
        .join("grand-child-col-1");

    assert_eq!(
        colnames.get(&parent_relpath),
        Some("Parent Col 1".to_string())
    );
    assert_eq!(
        colnames.get(&child_relpath),
        Some("Child Col 1".to_string())
    );
    assert_eq!(
        colnames.get(&grandchild_relpath),
        Some("Grand Child Col 1".to_string())
    );
}
