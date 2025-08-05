use std::{fs, path::PathBuf};

use crate::{
    collection,
    error::Error,
    models::SanitizedSegment,
    request,
    store::{self},
    tree_node::{self, MoveTreeNodeDto, NodeType},
};
#[test]
fn find_collection_returns_root_for_empty_path() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Tree Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();
    let space = collection::parse_root_collection(&tmp_space_abspath).unwrap();

    let result = tree_node::find_collection(&space, &PathBuf::from(""));
    assert!(result.is_ok());
    assert_eq!(result.unwrap().meta.fsname, "tree-space".to_string());
}

#[test]
fn find_collection_finds_direct_child() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Tree Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let (location_relpath, col_segment) = collection::create_parent_collections_if_missing(
        &PathBuf::from(""),
        &PathBuf::from("Parent Col 1"),
        &tmp_space_abspath,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&location_relpath, &col_segment, &tmp_space_abspath)
        .expect("Failed to create collection");

    let space = collection::parse_root_collection(&tmp_space_abspath).unwrap();
    let result = tree_node::find_collection(&space, &PathBuf::from("parent-col-1"));
    assert!(result.is_ok());
    let collection = result.unwrap();
    assert_eq!(collection.meta.name, Some("Parent Col 1".to_string()));
    assert_eq!(collection.meta.fsname, "parent-col-1");
}

#[test]
fn find_collection_finds_nested_child() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Tree Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let (location_relpath, col_segment) = collection::create_parent_collections_if_missing(
        &PathBuf::from(""),
        &PathBuf::from("Parent Col 1/Child Col 1"),
        &tmp_space_abspath,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&location_relpath, &col_segment, &tmp_space_abspath)
        .expect("Failed to create nested collection");

    let space = collection::parse_root_collection(&tmp_space_abspath).unwrap();
    let nested_path = PathBuf::from("parent-col-1/child-col-1");
    let result = tree_node::find_collection(&space, &nested_path);
    assert!(result.is_ok());
    let collection = result.unwrap();
    assert_eq!(collection.meta.name, Some("Child Col 1".to_string()));
    assert_eq!(collection.meta.fsname, "child-col-1");
}

#[test]
fn find_collection_fails_for_nonexistent_path() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Tree Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();
    let space = collection::parse_root_collection(&tmp_space_abspath).unwrap();

    let result = tree_node::find_collection(&space, &PathBuf::from("nonexistent-col-1"));
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
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Tree Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let (location_relpath, col_segment) = collection::create_parent_collections_if_missing(
        &PathBuf::from(""),
        &PathBuf::from("Parent Col 1"),
        &tmp_space_abspath,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&location_relpath, &col_segment, &tmp_space_abspath)
        .expect("Failed to create parent collection");

    let space = collection::parse_root_collection(&tmp_space_abspath).unwrap();
    let invalid_path = PathBuf::from("parent-col-1/missing-child-col-1");
    let result = tree_node::find_collection(&space, &invalid_path);
    assert!(result.is_err());
    match result.unwrap_err() {
        Error::InvalidPath(msg) => {
            assert!(msg.contains("Collection not found: missing-child-col-1"))
        }
        _ => panic!("Expected InvalidPath error"),
    }
}


#[test]
fn move_tree_node_fails_with_invalid_cur_relpath() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Tree Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let dto = MoveTreeNodeDto {
        node_type: NodeType::Collection,
        cur_relpath: PathBuf::from(""),
        nxt_relpath: PathBuf::from("parent-col-1/child-col-1"),
    };

    let result = tree_node::move_tree_node(&dto, &tmp_space_abspath);
    assert!(result.is_err());
    match result.unwrap_err() {
        Error::InvalidPath(msg) => assert_eq!(msg, "Invalid source path"),
        _ => panic!("Expected InvalidPath error"),
    }
}

#[test]
fn move_tree_node_fails_when_moving_to_self() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Tree Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let (location_relpath, col_segment) = collection::create_parent_collections_if_missing(
        &PathBuf::from(""),
        &PathBuf::from("Parent Col 1"),
        &tmp_space_abspath,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&location_relpath, &col_segment, &tmp_space_abspath)
        .expect("Failed to create collection");

    let dto = MoveTreeNodeDto {
        node_type: NodeType::Collection,
        cur_relpath: PathBuf::from("parent-col-1"),
        nxt_relpath: PathBuf::from("parent-col-1"),
    };

    let result = tree_node::move_tree_node(&dto, &tmp_space_abspath);
    assert!(result.is_err());
}

#[test]
fn move_tree_node_fails_when_moving_into_own_subtree() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Tree Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let (location_relpath, col_segment) = collection::create_parent_collections_if_missing(
        &PathBuf::from(""),
        &PathBuf::from("Parent Col 1"),
        &tmp_space_abspath,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&location_relpath, &col_segment, &tmp_space_abspath)
        .expect("Failed to create collection");

    let child_location = PathBuf::from("parent-col-1");
    let (child_location_relpath, child_col_segment) =
        collection::create_parent_collections_if_missing(
            &child_location,
            &PathBuf::from("Child Col"),
            &tmp_space_abspath,
        )
        .expect("Failed to create child collections");

    collection::create_collection(
        &child_location_relpath,
        &child_col_segment,
        &tmp_space_abspath,
    )
    .expect("Failed to create child collection");

    let dto = MoveTreeNodeDto {
        node_type: NodeType::Collection,
        cur_relpath: PathBuf::from("parent-col-1"),
        nxt_relpath: PathBuf::from("parent-col-1/child-col/parent-col-1"),
    };

    let result = tree_node::move_tree_node(&dto, &tmp_space_abspath);
    assert!(result.is_err());
}

#[test]
fn move_tree_node_fails_when_moving_collection_into_itself() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Tree Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let (location_relpath, col_segment) = collection::create_parent_collections_if_missing(
        &PathBuf::from(""),
        &PathBuf::from("Parent Col 1"),
        &tmp_space_abspath,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&location_relpath, &col_segment, &tmp_space_abspath)
        .expect("Failed to create parent collection");

    let dto = MoveTreeNodeDto {
        node_type: NodeType::Collection,
        cur_relpath: PathBuf::from("parent-col-1"),
        nxt_relpath: PathBuf::from("parent-col-1/parent-col-1"),
    };

    let result = tree_node::move_tree_node(&dto, &tmp_space_abspath);
    assert!(result.is_err());
    match result.unwrap_err() {
        Error::InvalidPath(msg) => assert_eq!(msg, "Cannot move collection into itself"),
        _ => panic!("Expected InvalidPath error"),
    }
}

#[test]
fn move_tree_node_fails_when_destination_already_exists() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Tree Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let relpath = "Parent Col 1";
    let (location_relpath, col_segment) = collection::create_parent_collections_if_missing(
        &PathBuf::from(""),
        &PathBuf::from(relpath),
        &tmp_space_abspath,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&location_relpath, &col_segment, &tmp_space_abspath)
        .expect("Failed to create source collection");

    let relpath = "Parent Col 2/Child Col 1";
    let (location_relpath, col_segment) = collection::create_parent_collections_if_missing(
        &PathBuf::from(""),
        &PathBuf::from(relpath),
        &tmp_space_abspath,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&location_relpath, &col_segment, &tmp_space_abspath)
        .expect("Failed to create existing collection");

    let conflicting_segment = SanitizedSegment {
        name: "Parent Col 1".to_string(),
        fsname: "parent-col-1".to_string(),
    };

    collection::create_collection(
        &PathBuf::from("parent-col-2"),
        &conflicting_segment,
        &tmp_space_abspath,
    )
    .expect("Failed to create conflicting collection");

    let dto = MoveTreeNodeDto {
        node_type: NodeType::Collection,
        cur_relpath: PathBuf::from("parent-col-1"),
        nxt_relpath: PathBuf::from("parent-col-2/parent-col-1"),
    };

    let result = tree_node::move_tree_node(&dto, &tmp_space_abspath);
    assert!(result.is_err());
    match result.unwrap_err() {
        Error::InvalidPath(msg) => assert!(msg.contains("already exists")),
        _ => panic!("Expected InvalidPath error"),
    }
}

#[test]
fn move_tree_node_fails_when_source_not_found() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Tree Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let relpath = "Parent Col 1";
    let (location_relpath, col_segment) = collection::create_parent_collections_if_missing(
        &PathBuf::from(""),
        &PathBuf::from(relpath),
        &tmp_space_abspath,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&location_relpath, &col_segment, &tmp_space_abspath)
        .expect("Failed to create parent collection");

    let dto = MoveTreeNodeDto {
        node_type: NodeType::Collection,
        cur_relpath: PathBuf::from("nonexistent-col-1"),
        nxt_relpath: PathBuf::from("parent-col-1/nonexistent-col-1"),
    };

    let result = tree_node::move_tree_node(&dto, &tmp_space_abspath);
    assert!(result.is_err());
    match result.unwrap_err() {
        Error::InvalidPath(msg) => assert!(msg.contains("not found")),
        _ => panic!("Expected InvalidPath error"),
    }
}

#[test]
fn move_tree_node_successfully_moves_collection() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Tree Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let relpath = "Parent Col 1";
    let (location_relpath, col_segment) = collection::create_parent_collections_if_missing(
        &PathBuf::from(""),
        &PathBuf::from(relpath),
        &tmp_space_abspath,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&location_relpath, &col_segment, &tmp_space_abspath)
        .expect("Failed to create source collection");

    let relpath = "Parent Col 2";
    let (location_relpath, col_segment) = collection::create_parent_collections_if_missing(
        &PathBuf::from(""),
        &PathBuf::from(relpath),
        &tmp_space_abspath,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&location_relpath, &col_segment, &tmp_space_abspath)
        .expect("Failed to create parent collection");

    let dto = MoveTreeNodeDto {
        node_type: NodeType::Collection,
        cur_relpath: PathBuf::from("parent-col-1"),
        nxt_relpath: PathBuf::from("parent-col-2/parent-col-1"),
    };

    let result = tree_node::move_tree_node(&dto, &tmp_space_abspath);
    assert!(result.is_ok());

    let space = collection::parse_root_collection(&tmp_space_abspath).unwrap();
    let parent_col = tree_node::find_collection(&space, &PathBuf::from("parent-col-2")).unwrap();
    let moved_collection = parent_col
        .collections
        .iter()
        .find(|c| c.meta.fsname == "parent-col-1")
        .unwrap();
    assert_eq!(moved_collection.meta.name, Some("Parent Col 1".to_string()));
    assert_eq!(moved_collection.meta.fsname, "parent-col-1");

    assert!(!&tmp_space_abspath.join("parent-col-1").exists());
    assert!(
        &tmp_space_abspath
            .join("parent-col-2")
            .join("parent-col-1")
            .exists()
    );
}

#[test]
fn move_tree_node_successfully_moves_request() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Tree Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let req_segment = SanitizedSegment {
        name: "Parent Req 1".to_string(),
        fsname: "parent-req-1".to_string(),
    };

    request::create_req(&PathBuf::from(""), &req_segment, &tmp_space_abspath)
        .expect("Failed to create request");

    let relpath = "Parent Col 1";
    let (location_relpath, col_segment) = collection::create_parent_collections_if_missing(
        &PathBuf::from(""),
        &PathBuf::from(relpath),
        &tmp_space_abspath,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&location_relpath, &col_segment, &tmp_space_abspath)
        .expect("Failed to create parent collection");

    let dto = MoveTreeNodeDto {
        node_type: NodeType::Request,
        cur_relpath: PathBuf::from("parent-req-1.toml"),
        nxt_relpath: PathBuf::from("parent-col-1/parent-req-1.toml"),
    };

    let result = tree_node::move_tree_node(&dto, &tmp_space_abspath);
    assert!(result.is_ok());

    let space = collection::parse_root_collection(&tmp_space_abspath).unwrap();
    let parent_col = tree_node::find_collection(&space, &PathBuf::from("parent-col-1")).unwrap();
    let moved_request = parent_col
        .requests
        .iter()
        .find(|r| r.meta.fsname == "parent-req-1.toml")
        .unwrap();
    assert_eq!(moved_request.meta.name, "Parent Req 1");
    assert_eq!(moved_request.meta.fsname, "parent-req-1.toml");

    assert!(!&tmp_space_abspath.join("parent-req-1.toml").exists());
    assert!(
        &tmp_space_abspath
            .join("parent-col-1")
            .join("parent-req-1.toml")
            .exists()
    );
}

#[test]
fn move_tree_node_fails_with_missing_destination_parent_directory() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Tree Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let relpath = "Parent Col 1";
    let (location_relpath, col_segment) = collection::create_parent_collections_if_missing(
        &PathBuf::from(""),
        &PathBuf::from(relpath),
        &tmp_space_abspath,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&location_relpath, &col_segment, &tmp_space_abspath)
        .expect("Failed to create source collection");

    let relpath = "Parent Col 2";
    let (location_relpath, col_segment) = collection::create_parent_collections_if_missing(
        &PathBuf::from(""),
        &PathBuf::from(relpath),
        &tmp_space_abspath,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&location_relpath, &col_segment, &tmp_space_abspath)
        .expect("Failed to create parent collection");

    let _space = collection::parse_root_collection(&tmp_space_abspath).unwrap();
    fs::remove_dir_all(tmp_space_abspath.join("parent-col-2"))
        .expect("Failed to remove parent directory");

    let dto = MoveTreeNodeDto {
        node_type: NodeType::Collection,
        cur_relpath: PathBuf::from("parent-col-1"),
        nxt_relpath: PathBuf::from("parent-col-2/parent-col-1"),
    };

    let result = tree_node::move_tree_node(&dto, &tmp_space_abspath);
    assert!(result.is_err());
    match result.unwrap_err() {
        Error::InvalidPath(msg) => {
            assert!(msg.contains("Collection not found"))
        }
        _ => panic!("Expected InvalidPath error about collection not found"),
    }
}

#[test]
fn move_tree_node_successfully_moves_collection_to_parent() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Tree Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let relpath = "Grand Parent Col 1/Parent Col 1/Child Col 1";
    let (location_relpath, col_segment) = collection::create_parent_collections_if_missing(
        &PathBuf::from(""),
        &PathBuf::from(relpath),
        &tmp_space_abspath,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&location_relpath, &col_segment, &tmp_space_abspath)
        .expect("Failed to create nested collection");

    let dto = MoveTreeNodeDto {
        node_type: NodeType::Collection,
        cur_relpath: PathBuf::from("grand-parent-col-1/parent-col-1/child-col-1"),
        nxt_relpath: PathBuf::from("grand-parent-col-1/child-col-1"),
    };

    let result = tree_node::move_tree_node(&dto, &tmp_space_abspath);
    assert!(result.is_ok());

    let space = collection::parse_root_collection(&tmp_space_abspath).unwrap();
    let grandparent_col =
        tree_node::find_collection(&space, &PathBuf::from("grand-parent-col-1")).unwrap();
    let moved_collection = grandparent_col
        .collections
        .iter()
        .find(|c| c.meta.fsname == "child-col-1")
        .unwrap();
    assert_eq!(moved_collection.meta.name, Some("Child Col 1".to_string()));
    assert_eq!(moved_collection.meta.fsname, "child-col-1");

    let parent_path = PathBuf::from("grand-parent-col-1/parent-col-1");
    let parent_col = tree_node::find_collection(&space, &parent_path).unwrap();
    assert!(
        !parent_col
            .collections
            .iter()
            .any(|c| c.meta.fsname == "child-col-1")
    );

    assert!(
        !&tmp_space_abspath
            .join("grand-parent-col-1")
            .join("parent-col-1")
            .join("child-col-1")
            .exists()
    );
    assert!(
        &tmp_space_abspath
            .join("grand-parent-col-1")
            .join("child-col-1")
            .exists()
    );
}

#[test]
fn move_tree_node_successfully_moves_request_to_parent() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Tree Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let relpath = "Grand Parent Col 1/Parent Col 1";
    let (location_relpath, col_segment) = collection::create_parent_collections_if_missing(
        &PathBuf::from(""),
        &PathBuf::from(relpath),
        &tmp_space_abspath,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&location_relpath, &col_segment, &tmp_space_abspath)
        .expect("Failed to create nested collection");

    let req_segment = SanitizedSegment {
        name: "Grand Child Req 1".to_string(),
        fsname: "grand-child-req-1".to_string(),
    };

    let req_parent_path = PathBuf::from("grand-parent-col-1/parent-col-1");
    request::create_req(&req_parent_path, &req_segment, &tmp_space_abspath)
        .expect("Failed to create request");

    let dto = MoveTreeNodeDto {
        node_type: NodeType::Request,
        cur_relpath: PathBuf::from("grand-parent-col-1/parent-col-1/grand-child-req-1.toml"),
        nxt_relpath: PathBuf::from("grand-parent-col-1/grand-child-req-1.toml"),
    };

    let result = tree_node::move_tree_node(&dto, &tmp_space_abspath);
    assert!(result.is_ok());

    let space = collection::parse_root_collection(&tmp_space_abspath).unwrap();
    let grandparent_col =
        tree_node::find_collection(&space, &PathBuf::from("grand-parent-col-1")).unwrap();
    let moved_request = grandparent_col
        .requests
        .iter()
        .find(|r| r.meta.fsname == "grand-child-req-1.toml")
        .unwrap();
    assert_eq!(moved_request.meta.name, "Grand Child Req 1");
    assert_eq!(moved_request.meta.fsname, "grand-child-req-1.toml");

    let parent_path = PathBuf::from("grand-parent-col-1/parent-col-1");
    let parent_col = tree_node::find_collection(&space, &parent_path).unwrap();
    assert!(
        !parent_col
            .requests
            .iter()
            .any(|r| r.meta.fsname == "grand-child-req-1.toml")
    );

    assert!(
        !&tmp_space_abspath
            .join("grand-parent-col-1")
            .join("parent-col-1")
            .join("grand-child-req-1.toml")
            .exists()
    );
    assert!(
        &tmp_space_abspath
            .join("grand-parent-col-1")
            .join("grand-child-req-1.toml")
            .exists()
    );
}

#[test]
fn move_tree_node_successfully_moves_collection_to_grandparent() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Tree Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let relpath = "Great Grand Parent Col 1/Grand Parent Col 1/Parent Col 1/Child Col 1";
    let (location_relpath, col_segment) = collection::create_parent_collections_if_missing(
        &PathBuf::from(""),
        &PathBuf::from(relpath),
        &tmp_space_abspath,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&location_relpath, &col_segment, &tmp_space_abspath)
        .expect("Failed to create deeply nested collection");

    let dto = MoveTreeNodeDto {
        node_type: NodeType::Collection,
        cur_relpath: PathBuf::from(
            "great-grand-parent-col-1/grand-parent-col-1/parent-col-1/child-col-1",
        ),
        nxt_relpath: PathBuf::from("great-grand-parent-col-1/grand-parent-col-1/child-col-1"),
    };

    let result = tree_node::move_tree_node(&dto, &tmp_space_abspath);
    assert!(result.is_ok());

    let space = collection::parse_root_collection(&tmp_space_abspath).unwrap();
    let grandparent_path = PathBuf::from("great-grand-parent-col-1/grand-parent-col-1");
    let grandparent_col = tree_node::find_collection(&space, &grandparent_path).unwrap();
    let moved_collection = grandparent_col
        .collections
        .iter()
        .find(|c| c.meta.fsname == "child-col-1")
        .unwrap();
    assert_eq!(moved_collection.meta.name, Some("Child Col 1".to_string()));
    assert_eq!(moved_collection.meta.fsname, "child-col-1");

    let parent_path = PathBuf::from("great-grand-parent-col-1/grand-parent-col-1/parent-col-1");
    let parent_col = tree_node::find_collection(&space, &parent_path).unwrap();
    assert!(
        !parent_col
            .collections
            .iter()
            .any(|c| c.meta.fsname == "child-col-1")
    );

    assert!(
        !&tmp_space_abspath
            .join("great-grand-parent-col-1")
            .join("grand-parent-col-1")
            .join("parent-col-1")
            .join("child-col-1")
            .exists()
    );
    assert!(
        &tmp_space_abspath
            .join("great-grand-parent-col-1")
            .join("grand-parent-col-1")
            .join("child-col-1")
            .exists()
    );
}

#[test]
fn move_tree_node_successfully_moves_request_to_grandparent() {
    let (_tmp_datadir, _tmp_spacedir, state_store) = store::utils::temp_space("Tree Space");
    let tmp_space_abspath = state_store.spaceref.as_ref().unwrap().abspath.clone();

    let (location_relpath, col_segment) = collection::create_parent_collections_if_missing(
        &PathBuf::from(""),
        &PathBuf::from("Great Grand Parent Col 1/Grand Parent Col 1/Parent Col 1"),
        &tmp_space_abspath,
    )
    .expect("Failed to create parent collections");

    collection::create_collection(&location_relpath, &col_segment, &tmp_space_abspath)
        .expect("Failed to create nested collection");

    let req_segment = SanitizedSegment {
        name: "Great Grand Child Req 1".to_string(),
        fsname: "great-grand-child-req-1".to_string(),
    };

    let req_parent_path = PathBuf::from("great-grand-parent-col-1/grand-parent-col-1/parent-col-1");
    request::create_req(&req_parent_path, &req_segment, &tmp_space_abspath)
        .expect("Failed to create request");

    let dto = MoveTreeNodeDto {
        node_type: NodeType::Request,
        cur_relpath: PathBuf::from(
            "great-grand-parent-col-1/grand-parent-col-1/parent-col-1/great-grand-child-req-1.toml",
        ),
        nxt_relpath: PathBuf::from("great-grand-parent-col-1/great-grand-child-req-1.toml"),
    };

    let result = tree_node::move_tree_node(&dto, &tmp_space_abspath);
    assert!(result.is_ok());

    let space = collection::parse_root_collection(&tmp_space_abspath).unwrap();
    let great_grandparent_col =
        tree_node::find_collection(&space, &PathBuf::from("great-grand-parent-col-1")).unwrap();
    let moved_request = great_grandparent_col
        .requests
        .iter()
        .find(|r| r.meta.fsname == "great-grand-child-req-1.toml")
        .unwrap();
    assert_eq!(moved_request.meta.name, "Great Grand Child Req 1");
    assert_eq!(moved_request.meta.fsname, "great-grand-child-req-1.toml");

    let parent_path = PathBuf::from("great-grand-parent-col-1/grand-parent-col-1/parent-col-1");
    let parent_col = tree_node::find_collection(&space, &parent_path).unwrap();
    assert!(
        !parent_col
            .requests
            .iter()
            .any(|r| r.meta.fsname == "great-grand-child-req-1.toml")
    );

    assert!(
        !&tmp_space_abspath
            .join("great-grand-parent-col-1")
            .join("grand-parent-col-1")
            .join("parent-col-1")
            .join("great-grand-child-req-1.toml")
            .exists()
    );
    assert!(
        &tmp_space_abspath
            .join("great-grand-parent-col-1")
            .join("great-grand-child-req-1.toml")
            .exists()
    );
}
