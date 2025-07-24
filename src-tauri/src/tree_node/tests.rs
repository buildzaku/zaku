use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile;

use crate::{
    collection,
    error::Error,
    models::SanitizedSegment,
    request,
    state::SharedState,
    store::{self, UserSettingsStore},
    tree_node::{self, MoveTreeNodeDto, NodeType},
};
#[test]
fn find_collection_returns_root_for_empty_path() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (sharedstate, _store, _space_ref) = store::utils::tmp_space("Tree Space", tmp_dir.path());
    let space = sharedstate.space.unwrap();

    let result = tree_node::find_collection(&space.root_collection, Path::new(""));
    assert!(result.is_ok());
    assert_eq!(result.unwrap().meta.fsname, "tree-space".to_string());
}

#[test]
fn find_collection_finds_direct_child() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (mut sharedstate, _store, _space_ref) =
        store::utils::tmp_space("Tree Space", tmp_dir.path());

    let location_relpath = Path::new("");
    let relpath = "Parent Col 1";
    let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
        location_relpath,
        relpath,
        &mut sharedstate,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&parent_relpath, &col_segment, &mut sharedstate)
        .expect("Failed to create collection");

    let space = sharedstate.space.unwrap();
    let result = tree_node::find_collection(&space.root_collection, Path::new("parent-col-1"));
    assert!(result.is_ok());
    let collection = result.unwrap();
    assert_eq!(collection.meta.name, Some("Parent Col 1".to_string()));
    assert_eq!(collection.meta.fsname, "parent-col-1");
}

#[test]
fn find_collection_finds_nested_child() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (mut sharedstate, _store, _space_ref) =
        store::utils::tmp_space("Tree Space", tmp_dir.path());

    let location_relpath = Path::new("");
    let relpath = "Parent Col 1/Child Col 1";
    let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
        location_relpath,
        relpath,
        &mut sharedstate,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&parent_relpath, &col_segment, &mut sharedstate)
        .expect("Failed to create nested collection");

    let space = sharedstate.space.unwrap();
    let nested_path = PathBuf::from("parent-col-1").join("child-col-1");
    let result = tree_node::find_collection(&space.root_collection, &nested_path);
    assert!(result.is_ok());
    let collection = result.unwrap();
    assert_eq!(collection.meta.name, Some("Child Col 1".to_string()));
    assert_eq!(collection.meta.fsname, "child-col-1");
}

#[test]
fn find_collection_fails_for_nonexistent_path() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (sharedstate, _store, _space_ref) = store::utils::tmp_space("Tree Space", tmp_dir.path());
    let space = sharedstate.space.unwrap();

    let result = tree_node::find_collection(&space.root_collection, Path::new("nonexistent-col-1"));
    assert!(result.is_err());
    match result.unwrap_err() {
        Error::InvalidPath(msg) => {
            assert!(msg.contains("Collection not found: nonexistent-col-1"))
        }
        _ => panic!("Expected InvalidPath error"),
    }
}

#[test]
fn find_collection_fails_for_partially_invalid_path() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (mut sharedstate, _store, _space_ref) =
        store::utils::tmp_space("Tree Space", tmp_dir.path());

    let location_relpath = Path::new("");
    let relpath = "Parent Col 1";
    let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
        location_relpath,
        relpath,
        &mut sharedstate,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&parent_relpath, &col_segment, &mut sharedstate)
        .expect("Failed to create parent collection");

    let space = sharedstate.space.unwrap();
    let invalid_path = PathBuf::from("parent-col-1").join("missing-child-col-1");
    let result = tree_node::find_collection(&space.root_collection, &invalid_path);
    assert!(result.is_err());
    match result.unwrap_err() {
        Error::InvalidPath(msg) => {
            assert!(msg.contains("Collection not found: missing-child-col-1"))
        }
        _ => panic!("Expected InvalidPath error"),
    }
}

#[test]
fn move_tree_node_fails_with_no_space() {
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

    let dest_relpath = PathBuf::from("parent-col-2").join("parent-col-1");
    let dto = MoveTreeNodeDto {
        node_type: NodeType::Collection,
        src_relpath: "parent-col-1".to_string(),
        dest_relpath: dest_relpath.to_string_lossy().to_string(),
    };

    let result = tree_node::move_tree_node(&dto, &mut sharedstate);
    assert!(result.is_err());
    match result.unwrap_err() {
        Error::InvalidPath(msg) => assert_eq!(msg, "No space found"),
        _ => panic!("Expected InvalidPath error"),
    }
}

#[test]
fn move_tree_node_fails_with_invalid_src_relpath() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (mut sharedstate, _store, _space_ref) =
        store::utils::tmp_space("Tree Space", tmp_dir.path());

    let dest_relpath = PathBuf::from("parent-col-1").join("child-col-1");
    let dto = MoveTreeNodeDto {
        node_type: NodeType::Collection,
        src_relpath: "".to_string(),
        dest_relpath: dest_relpath.to_string_lossy().to_string(),
    };

    let result = tree_node::move_tree_node(&dto, &mut sharedstate);
    assert!(result.is_err());
    match result.unwrap_err() {
        Error::InvalidPath(msg) => assert_eq!(msg, "Invalid source path"),
        _ => panic!("Expected InvalidPath error"),
    }
}

#[test]
fn move_tree_node_fails_when_dropping_to_same_parent() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (mut sharedstate, _store, _space_ref) =
        store::utils::tmp_space("Tree Space", tmp_dir.path());

    let location_relpath = Path::new("");
    let relpath = "Parent Col 1";
    let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
        location_relpath,
        relpath,
        &mut sharedstate,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&parent_relpath, &col_segment, &mut sharedstate)
        .expect("Failed to create collection");

    let dto = MoveTreeNodeDto {
        node_type: NodeType::Collection,
        src_relpath: "parent-col-1".to_string(),
        dest_relpath: "parent-col-2".to_string(),
    };

    let result = tree_node::move_tree_node(&dto, &mut sharedstate);
    assert!(result.is_err());
    match result.unwrap_err() {
        Error::InvalidPath(msg) => assert_eq!(msg, "Cannot drop to same parent"),
        _ => panic!("Expected InvalidPath error"),
    }
}

#[test]
fn move_tree_node_fails_when_moving_collection_into_itself() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (mut sharedstate, _store, _space_ref) =
        store::utils::tmp_space("Tree Space", tmp_dir.path());

    let location_relpath = Path::new("");
    let relpath = "Parent Col 1";
    let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
        location_relpath,
        relpath,
        &mut sharedstate,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&parent_relpath, &col_segment, &mut sharedstate)
        .expect("Failed to create parent collection");

    let dest_relpath = PathBuf::from("parent-col-1").join("parent-col-1");
    let dto = MoveTreeNodeDto {
        node_type: NodeType::Collection,
        src_relpath: "parent-col-1".to_string(),
        dest_relpath: dest_relpath.to_string_lossy().to_string(),
    };

    let result = tree_node::move_tree_node(&dto, &mut sharedstate);
    assert!(result.is_err());
    match result.unwrap_err() {
        Error::InvalidPath(msg) => assert_eq!(msg, "Cannot move collection into itself"),
        _ => panic!("Expected InvalidPath error"),
    }
}

#[test]
fn move_tree_node_fails_when_destination_already_exists() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (mut sharedstate, _store, _space_ref) =
        store::utils::tmp_space("Tree Space", tmp_dir.path());

    let location_relpath = Path::new("");
    let relpath = "Parent Col 1";
    let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
        location_relpath,
        relpath,
        &mut sharedstate,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&parent_relpath, &col_segment, &mut sharedstate)
        .expect("Failed to create source collection");

    let location_relpath = Path::new("");
    let relpath = "Parent Col 2/Child Col 1";
    let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
        location_relpath,
        relpath,
        &mut sharedstate,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&parent_relpath, &col_segment, &mut sharedstate)
        .expect("Failed to create existing collection");

    let conflicting_segment = SanitizedSegment {
        name: "Parent Col 1".to_string(),
        fsname: "parent-col-1".to_string(),
    };

    collection::create_collection(
        Path::new("parent-col-2"),
        &conflicting_segment,
        &mut sharedstate,
    )
    .expect("Failed to create conflicting collection");

    let dest_relpath = PathBuf::from("parent-col-2").join("parent-col-1");
    let dto = MoveTreeNodeDto {
        node_type: NodeType::Collection,
        src_relpath: "parent-col-1".to_string(),
        dest_relpath: dest_relpath.to_string_lossy().to_string(),
    };

    let result = tree_node::move_tree_node(&dto, &mut sharedstate);
    assert!(result.is_err());
    match result.unwrap_err() {
        Error::InvalidPath(msg) => assert!(msg.contains("already exists")),
        _ => panic!("Expected InvalidPath error"),
    }
}

#[test]
fn move_tree_node_fails_when_source_not_found() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (mut sharedstate, _store, _space_ref) =
        store::utils::tmp_space("Tree Space", tmp_dir.path());

    let location_relpath = Path::new("");
    let relpath = "Parent Col 1";
    let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
        location_relpath,
        relpath,
        &mut sharedstate,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&parent_relpath, &col_segment, &mut sharedstate)
        .expect("Failed to create parent collection");

    let dest_relpath = PathBuf::from("parent-col-1").join("nonexistent-col-1");
    let dto = MoveTreeNodeDto {
        node_type: NodeType::Collection,
        src_relpath: "nonexistent-col-1".to_string(),
        dest_relpath: dest_relpath.to_string_lossy().to_string(),
    };

    let result = tree_node::move_tree_node(&dto, &mut sharedstate);
    assert!(result.is_err());
    match result.unwrap_err() {
        Error::InvalidPath(msg) => assert!(msg.contains("not found")),
        _ => panic!("Expected InvalidPath error"),
    }
}

#[test]
fn move_tree_node_successfully_moves_collection() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (mut sharedstate, _store, _space_ref) =
        store::utils::tmp_space("Tree Space", tmp_dir.path());

    let location_relpath = Path::new("");
    let relpath = "Parent Col 1";
    let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
        location_relpath,
        relpath,
        &mut sharedstate,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&parent_relpath, &col_segment, &mut sharedstate)
        .expect("Failed to create source collection");

    let location_relpath = Path::new("");
    let relpath = "Parent Col 2";
    let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
        location_relpath,
        relpath,
        &mut sharedstate,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&parent_relpath, &col_segment, &mut sharedstate)
        .expect("Failed to create parent collection");

    let dest_relpath = PathBuf::from("parent-col-2").join("parent-col-1");
    let dto = MoveTreeNodeDto {
        node_type: NodeType::Collection,
        src_relpath: "parent-col-1".to_string(),
        dest_relpath: dest_relpath.to_string_lossy().to_string(),
    };

    let result = tree_node::move_tree_node(&dto, &mut sharedstate);
    assert!(result.is_ok());

    let space = sharedstate.space.unwrap();
    let parent_col =
        tree_node::find_collection(&space.root_collection, Path::new("parent-col-2")).unwrap();
    let moved_collection = parent_col
        .collections
        .iter()
        .find(|c| c.meta.fsname == "parent-col-1")
        .unwrap();
    assert_eq!(moved_collection.meta.name, Some("Parent Col 1".to_string()));
    assert_eq!(moved_collection.meta.fsname, "parent-col-1");

    let space_path = Path::new(&space.abspath);
    assert!(!space_path.join("parent-col-1").exists());
    assert!(space_path
        .join("parent-col-2")
        .join("parent-col-1")
        .exists());
}

#[test]
fn move_tree_node_successfully_moves_request() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (mut sharedstate, _store, _space_ref) =
        store::utils::tmp_space("Tree Space", tmp_dir.path());

    let req_segment = SanitizedSegment {
        name: "Parent Req 1".to_string(),
        fsname: "parent-req-1".to_string(),
    };

    request::create_req(Path::new(""), &req_segment, &mut sharedstate)
        .expect("Failed to create request");

    let location_relpath = Path::new("");
    let relpath = "Parent Col 1";
    let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
        location_relpath,
        relpath,
        &mut sharedstate,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&parent_relpath, &col_segment, &mut sharedstate)
        .expect("Failed to create parent collection");

    let dest_relpath = PathBuf::from("parent-col-1").join("parent-req-1.toml");
    let dto = MoveTreeNodeDto {
        node_type: NodeType::Request,
        src_relpath: "parent-req-1.toml".to_string(),
        dest_relpath: dest_relpath.to_string_lossy().to_string(),
    };

    let result = tree_node::move_tree_node(&dto, &mut sharedstate);
    assert!(result.is_ok());

    let space = sharedstate.space.unwrap();
    let parent_col =
        tree_node::find_collection(&space.root_collection, Path::new("parent-col-1")).unwrap();
    let moved_request = parent_col
        .requests
        .iter()
        .find(|r| r.meta.fsname == "parent-req-1.toml")
        .unwrap();
    assert_eq!(moved_request.meta.name, "Parent Req 1");
    assert_eq!(moved_request.meta.fsname, "parent-req-1.toml");

    let space_path = Path::new(&space.abspath);
    assert!(!space_path.join("parent-req-1.toml").exists());
    assert!(space_path
        .join("parent-col-1")
        .join("parent-req-1.toml")
        .exists());
}

#[test]
fn move_tree_node_fails_with_missing_destination_parent_directory() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (mut sharedstate, _store, _space_ref) =
        store::utils::tmp_space("Tree Space", tmp_dir.path());

    let location_relpath = Path::new("");
    let relpath = "Parent Col 1";
    let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
        location_relpath,
        relpath,
        &mut sharedstate,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&parent_relpath, &col_segment, &mut sharedstate)
        .expect("Failed to create source collection");

    let location_relpath = Path::new("");
    let relpath = "Parent Col 2";
    let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
        location_relpath,
        relpath,
        &mut sharedstate,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&parent_relpath, &col_segment, &mut sharedstate)
        .expect("Failed to create parent collection");

    let space = sharedstate.space.as_ref().unwrap();
    let space_path = Path::new(&space.abspath);
    fs::remove_dir_all(space_path.join("parent-col-2")).expect("Failed to remove parent directory");

    let dest_relpath = PathBuf::from("parent-col-2").join("parent-col-1");
    let dto = MoveTreeNodeDto {
        node_type: NodeType::Collection,
        src_relpath: "parent-col-1".to_string(),
        dest_relpath: dest_relpath.to_string_lossy().to_string(),
    };

    let result = tree_node::move_tree_node(&dto, &mut sharedstate);
    assert!(result.is_err());
    match result.unwrap_err() {
        Error::InvalidPath(msg) => {
            assert!(msg.contains("Destination parent directory does not exist"))
        }
        _ => panic!("Expected InvalidPath error about missing destination parent directory"),
    }
}

#[test]
fn move_tree_node_successfully_moves_collection_to_parent() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (mut sharedstate, _store, _space_ref) =
        store::utils::tmp_space("Tree Space", tmp_dir.path());

    let location_relpath = Path::new("");
    let relpath = "Grand Parent Col 1/Parent Col 1/Child Col 1";
    let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
        location_relpath,
        relpath,
        &mut sharedstate,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&parent_relpath, &col_segment, &mut sharedstate)
        .expect("Failed to create nested collection");

    let src_relpath = PathBuf::from("grand-parent-col-1")
        .join("parent-col-1")
        .join("child-col-1");
    let dest_relpath = PathBuf::from("grand-parent-col-1").join("child-col-1");
    let dto = MoveTreeNodeDto {
        node_type: NodeType::Collection,
        src_relpath: src_relpath.to_string_lossy().to_string(),
        dest_relpath: dest_relpath.to_string_lossy().to_string(),
    };

    let result = tree_node::move_tree_node(&dto, &mut sharedstate);
    assert!(result.is_ok());

    let space = sharedstate.space.unwrap();
    let grandparent_col =
        tree_node::find_collection(&space.root_collection, Path::new("grand-parent-col-1"))
            .unwrap();
    let moved_collection = grandparent_col
        .collections
        .iter()
        .find(|c| c.meta.fsname == "child-col-1")
        .unwrap();
    assert_eq!(moved_collection.meta.name, Some("Child Col 1".to_string()));
    assert_eq!(moved_collection.meta.fsname, "child-col-1");

    let parent_path = PathBuf::from("grand-parent-col-1").join("parent-col-1");
    let parent_col = tree_node::find_collection(&space.root_collection, &parent_path).unwrap();
    assert!(!parent_col
        .collections
        .iter()
        .any(|c| c.meta.fsname == "child-col-1"));

    let space_path = Path::new(&space.abspath);
    assert!(!space_path
        .join("grand-parent-col-1")
        .join("parent-col-1")
        .join("child-col-1")
        .exists());
    assert!(space_path
        .join("grand-parent-col-1")
        .join("child-col-1")
        .exists());
}

#[test]
fn move_tree_node_successfully_moves_request_to_parent() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (mut sharedstate, _store, _space_ref) =
        store::utils::tmp_space("Tree Space", tmp_dir.path());

    let location_relpath = Path::new("");
    let relpath = "Grand Parent Col 1/Parent Col 1";
    let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
        location_relpath,
        relpath,
        &mut sharedstate,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&parent_relpath, &col_segment, &mut sharedstate)
        .expect("Failed to create nested collection");

    let req_segment = SanitizedSegment {
        name: "Grand Child Req 1".to_string(),
        fsname: "grand-child-req-1".to_string(),
    };

    let req_parent_path = PathBuf::from("grand-parent-col-1").join("parent-col-1");
    request::create_req(&req_parent_path, &req_segment, &mut sharedstate)
        .expect("Failed to create request");

    let src_relpath = PathBuf::from("grand-parent-col-1")
        .join("parent-col-1")
        .join("grand-child-req-1.toml");
    let dest_relpath = PathBuf::from("grand-parent-col-1").join("grand-child-req-1.toml");
    let dto = MoveTreeNodeDto {
        node_type: NodeType::Request,
        src_relpath: src_relpath.to_string_lossy().to_string(),
        dest_relpath: dest_relpath.to_string_lossy().to_string(),
    };

    let result = tree_node::move_tree_node(&dto, &mut sharedstate);
    assert!(result.is_ok());

    let space = sharedstate.space.unwrap();
    let grandparent_col =
        tree_node::find_collection(&space.root_collection, Path::new("grand-parent-col-1"))
            .unwrap();
    let moved_request = grandparent_col
        .requests
        .iter()
        .find(|r| r.meta.fsname == "grand-child-req-1.toml")
        .unwrap();
    assert_eq!(moved_request.meta.name, "Grand Child Req 1");
    assert_eq!(moved_request.meta.fsname, "grand-child-req-1.toml");

    let parent_path = PathBuf::from("grand-parent-col-1").join("parent-col-1");
    let parent_col = tree_node::find_collection(&space.root_collection, &parent_path).unwrap();
    assert!(!parent_col
        .requests
        .iter()
        .any(|r| r.meta.fsname == "grand-child-req-1.toml"));

    let space_path = Path::new(&space.abspath);
    assert!(!space_path
        .join("grand-parent-col-1")
        .join("parent-col-1")
        .join("grand-child-req-1.toml")
        .exists());
    assert!(space_path
        .join("grand-parent-col-1")
        .join("grand-child-req-1.toml")
        .exists());
}

#[test]
fn move_tree_node_successfully_moves_collection_to_grandparent() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (mut sharedstate, _store, _space_ref) =
        store::utils::tmp_space("Tree Space", tmp_dir.path());

    let location_relpath = Path::new("");
    let relpath = "Great Grand Parent Col 1/Grand Parent Col 1/Parent Col 1/Child Col 1";
    let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
        location_relpath,
        relpath,
        &mut sharedstate,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&parent_relpath, &col_segment, &mut sharedstate)
        .expect("Failed to create deeply nested collection");

    let src_relpath = PathBuf::from("great-grand-parent-col-1")
        .join("grand-parent-col-1")
        .join("parent-col-1")
        .join("child-col-1");
    let dest_relpath = PathBuf::from("great-grand-parent-col-1")
        .join("grand-parent-col-1")
        .join("child-col-1");
    let dto = MoveTreeNodeDto {
        node_type: NodeType::Collection,
        src_relpath: src_relpath.to_string_lossy().to_string(),
        dest_relpath: dest_relpath.to_string_lossy().to_string(),
    };

    let result = tree_node::move_tree_node(&dto, &mut sharedstate);
    assert!(result.is_ok());

    let space = sharedstate.space.unwrap();
    let grandparent_path = PathBuf::from("great-grand-parent-col-1").join("grand-parent-col-1");
    let grandparent_col =
        tree_node::find_collection(&space.root_collection, &grandparent_path).unwrap();
    let moved_collection = grandparent_col
        .collections
        .iter()
        .find(|c| c.meta.fsname == "child-col-1")
        .unwrap();
    assert_eq!(moved_collection.meta.name, Some("Child Col 1".to_string()));
    assert_eq!(moved_collection.meta.fsname, "child-col-1");

    let parent_path = PathBuf::from("great-grand-parent-col-1")
        .join("grand-parent-col-1")
        .join("parent-col-1");
    let parent_col = tree_node::find_collection(&space.root_collection, &parent_path).unwrap();
    assert!(!parent_col
        .collections
        .iter()
        .any(|c| c.meta.fsname == "child-col-1"));

    let space_path = Path::new(&space.abspath);
    assert!(!space_path
        .join("great-grand-parent-col-1")
        .join("grand-parent-col-1")
        .join("parent-col-1")
        .join("child-col-1")
        .exists());
    assert!(space_path
        .join("great-grand-parent-col-1")
        .join("grand-parent-col-1")
        .join("child-col-1")
        .exists());
}

#[test]
fn move_tree_node_successfully_moves_request_to_grandparent() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (mut sharedstate, _store, _space_ref) =
        store::utils::tmp_space("Tree Space", tmp_dir.path());

    let location_relpath = Path::new("");
    let relpath = "Great Grand Parent Col 1/Grand Parent Col 1/Parent Col 1";
    let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
        location_relpath,
        relpath,
        &mut sharedstate,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&parent_relpath, &col_segment, &mut sharedstate)
        .expect("Failed to create nested collection");

    let req_segment = SanitizedSegment {
        name: "Great Grand Child Req 1".to_string(),
        fsname: "great-grand-child-req-1".to_string(),
    };

    let req_parent_path = PathBuf::from("great-grand-parent-col-1")
        .join("grand-parent-col-1")
        .join("parent-col-1");
    request::create_req(&req_parent_path, &req_segment, &mut sharedstate)
        .expect("Failed to create request");

    let src_relpath = PathBuf::from("great-grand-parent-col-1")
        .join("grand-parent-col-1")
        .join("parent-col-1")
        .join("great-grand-child-req-1.toml");
    let dest_relpath =
        PathBuf::from("great-grand-parent-col-1").join("great-grand-child-req-1.toml");
    let dto = MoveTreeNodeDto {
        node_type: NodeType::Request,
        src_relpath: src_relpath.to_string_lossy().to_string(),
        dest_relpath: dest_relpath.to_string_lossy().to_string(),
    };

    let result = tree_node::move_tree_node(&dto, &mut sharedstate);
    assert!(result.is_ok());

    let space = sharedstate.space.unwrap();
    let great_grandparent_col = tree_node::find_collection(
        &space.root_collection,
        Path::new("great-grand-parent-col-1"),
    )
    .unwrap();
    let moved_request = great_grandparent_col
        .requests
        .iter()
        .find(|r| r.meta.fsname == "great-grand-child-req-1.toml")
        .unwrap();
    assert_eq!(moved_request.meta.name, "Great Grand Child Req 1");
    assert_eq!(moved_request.meta.fsname, "great-grand-child-req-1.toml");

    let parent_path = PathBuf::from("great-grand-parent-col-1")
        .join("grand-parent-col-1")
        .join("parent-col-1");
    let parent_col = tree_node::find_collection(&space.root_collection, &parent_path).unwrap();
    assert!(!parent_col
        .requests
        .iter()
        .any(|r| r.meta.fsname == "great-grand-child-req-1.toml"));

    let space_path = Path::new(&space.abspath);
    assert!(!space_path
        .join("great-grand-parent-col-1")
        .join("grand-parent-col-1")
        .join("parent-col-1")
        .join("great-grand-child-req-1.toml")
        .exists());
    assert!(space_path
        .join("great-grand-parent-col-1")
        .join("great-grand-child-req-1.toml")
        .exists());
}
