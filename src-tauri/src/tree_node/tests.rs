use std::{fs, path::Path};
use tempfile;

use crate::{
    collection::{
        self,
        models::{Collection, CreateCollectionDto},
    },
    error::Error,
    request::{self, models::CreateRequestDto},
    space::{self, models::CreateSpaceDto},
    state::SharedState,
    tree_node::{self, MoveTreeNodeDto, NodeType},
};

const SPACE_NAME: &str = "Tree Node";
const SPACE_FSNAME: &str = "tree-node";

fn tmp_sharedstate(tmp_dir: &Path) -> SharedState {
    let dto = CreateSpaceDto {
        name: SPACE_NAME.to_string(),
        location: tmp_dir.to_string_lossy().to_string(),
    };

    let mut sharedstate = SharedState::default();
    space::create_space(dto, &mut sharedstate).expect("Failed to create test space");

    sharedstate
}

#[test]
fn find_collection_returns_root_for_empty_path() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let sharedstate = tmp_sharedstate(tmp_dir.path());
    let space = sharedstate.space.unwrap();

    let result = tree_node::find_collection(&space.root_collection, Path::new(""));
    assert!(result.is_ok());
    assert_eq!(result.unwrap().meta.fsname, SPACE_FSNAME.to_string());
}

#[test]
fn find_collection_finds_direct_child() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_sharedstate(tmp_dir.path());

    let dto = CreateCollectionDto {
        parent_relpath: "".to_string(),
        relpath: "Parent Collection 1".to_string(),
    };
    collection::create_collection(&dto, &mut sharedstate).expect("Failed to create collection");

    let space = sharedstate.space.unwrap();
    let result =
        tree_node::find_collection(&space.root_collection, Path::new("parent-collection-1"));
    assert!(result.is_ok());
    let collection = result.unwrap();
    assert_eq!(
        collection.meta.name,
        Some("Parent Collection 1".to_string())
    );
    assert_eq!(collection.meta.fsname, "parent-collection-1");
}

// temp
pub fn log_collection_tree(collection: &Collection, depth: usize) {
    let indent = "  ".repeat(depth);
    eprintln!("{indent}Collection:");
    eprintln!("{}  fsname: '{}'", indent, collection.meta.fsname);
    eprintln!("{}  name: {:?}", indent, collection.meta.name);
    eprintln!("{}  requests: {}", indent, collection.requests.len());

    for (i, req) in collection.requests.iter().enumerate() {
        eprintln!("{}    req[{}]: {}", indent, i, req.meta.name);
    }

    eprintln!(
        "{}  sub-collections: {}",
        indent,
        collection.collections.len()
    );
    for sub_collection in &collection.collections {
        log_collection_tree(sub_collection, depth + 1);
    }
}

#[test]
fn find_collection_finds_nested_child() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_sharedstate(tmp_dir.path());

    let dto = CreateCollectionDto {
        parent_relpath: "".to_string(),
        relpath: "Parent Collection 1/Child Collection 1".to_string(),
    };
    collection::create_collection(&dto, &mut sharedstate)
        .expect("Failed to create nested collection");

    let space = sharedstate.space.unwrap();
    // log_collection_tree(&space.root_collection, 0); // temp
    let result = tree_node::find_collection(
        &space.root_collection,
        Path::new("parent-collection-1/child-collection-1"),
    );
    assert!(result.is_ok());
    let collection = result.unwrap();
    assert_eq!(collection.meta.name, Some("Child Collection 1".to_string()));
    assert_eq!(collection.meta.fsname, "child-collection-1");
}

#[test]
fn find_collection_fails_for_nonexistent_path() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let sharedstate = tmp_sharedstate(tmp_dir.path());
    let space = sharedstate.space.unwrap();

    let result =
        tree_node::find_collection(&space.root_collection, Path::new("nonexistent-collection"));
    assert!(result.is_err());
    match result.unwrap_err() {
        Error::InvalidPath(msg) => {
            assert!(msg.contains("Collection not found: nonexistent-collection"))
        }
        _ => panic!("Expected InvalidPath error"),
    }
}

#[test]
fn find_collection_fails_for_partially_invalid_path() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_sharedstate(tmp_dir.path());

    let dto = CreateCollectionDto {
        parent_relpath: "".to_string(),
        relpath: "Parent Collection 1".to_string(),
    };
    collection::create_collection(&dto, &mut sharedstate)
        .expect("Failed to create parent collection");

    let space = sharedstate.space.unwrap();
    let result = tree_node::find_collection(
        &space.root_collection,
        Path::new("parent-collection-1/missing-child"),
    );
    assert!(result.is_err());
    match result.unwrap_err() {
        Error::InvalidPath(msg) => assert!(msg.contains("Collection not found: missing-child")),
        _ => panic!("Expected InvalidPath error"),
    }
}

#[test]
fn move_tree_node_fails_with_no_space() {
    let mut sharedstate = SharedState::default();

    let dto = MoveTreeNodeDto {
        node_type: NodeType::Collection,
        src_relpath: "parent-collection-1".to_string(),
        dest_relpath: "parent-collection-2/parent-collection-1".to_string(),
    };

    let result = tree_node::move_tree_node(&dto, &mut sharedstate);
    assert!(result.is_err());
    match result.unwrap_err() {
        Error::InvalidPath(msg) => assert_eq!(msg, "No space found"),
        _ => panic!("Expected InvalidPath error"),
    }
}

#[test]
fn move_tree_node_fails_with_invalid_source_path() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_sharedstate(tmp_dir.path());

    let dto = MoveTreeNodeDto {
        node_type: NodeType::Collection,
        src_relpath: "".to_string(),
        dest_relpath: "parent-collection-1/child-collection-1".to_string(),
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
    let mut sharedstate = tmp_sharedstate(tmp_dir.path());

    let dto = CreateCollectionDto {
        parent_relpath: "".to_string(),
        relpath: "Parent Collection 1".to_string(),
    };
    collection::create_collection(&dto, &mut sharedstate).expect("Failed to create collection");

    let dto = MoveTreeNodeDto {
        node_type: NodeType::Collection,
        src_relpath: "parent-collection-1".to_string(),
        dest_relpath: "parent-collection-2".to_string(),
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
    let mut sharedstate = tmp_sharedstate(tmp_dir.path());

    let dto = CreateCollectionDto {
        parent_relpath: "".to_string(),
        relpath: "Parent Collection 1".to_string(),
    };
    collection::create_collection(&dto, &mut sharedstate)
        .expect("Failed to create parent collection");

    let dto = MoveTreeNodeDto {
        node_type: NodeType::Collection,
        src_relpath: "parent-collection-1".to_string(),
        dest_relpath: "parent-collection-1/parent-collection-1".to_string(),
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
    let mut sharedstate = tmp_sharedstate(tmp_dir.path());

    let dto = CreateCollectionDto {
        parent_relpath: "".to_string(),
        relpath: "Parent Collection 1".to_string(),
    };
    collection::create_collection(&dto, &mut sharedstate)
        .expect("Failed to create source collection");

    let dto = CreateCollectionDto {
        parent_relpath: "".to_string(),
        relpath: "Parent Collection 2/Child Collection 1".to_string(),
    };
    collection::create_collection(&dto, &mut sharedstate)
        .expect("Failed to create existing collection");

    let create_dto = CreateCollectionDto {
        parent_relpath: "parent-collection-2".to_string(),
        relpath: "Parent Collection 1".to_string(),
    };
    collection::create_collection(&create_dto, &mut sharedstate)
        .expect("Failed to create conflicting collection");

    let dto = MoveTreeNodeDto {
        node_type: NodeType::Collection,
        src_relpath: "parent-collection-1".to_string(),
        dest_relpath: "parent-collection-2/parent-collection-1".to_string(),
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
    let mut sharedstate = tmp_sharedstate(tmp_dir.path());

    let dto = CreateCollectionDto {
        parent_relpath: "".to_string(),
        relpath: "Parent Collection 1".to_string(),
    };
    collection::create_collection(&dto, &mut sharedstate)
        .expect("Failed to create parent collection");

    let dto = MoveTreeNodeDto {
        node_type: NodeType::Collection,
        src_relpath: "nonexistent-collection".to_string(),
        dest_relpath: "parent-collection-1/nonexistent-collection".to_string(),
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
    let mut sharedstate = tmp_sharedstate(tmp_dir.path());

    let dto = CreateCollectionDto {
        parent_relpath: "".to_string(),
        relpath: "Parent Collection 1".to_string(),
    };
    collection::create_collection(&dto, &mut sharedstate)
        .expect("Failed to create source collection");

    let dto = CreateCollectionDto {
        parent_relpath: "".to_string(),
        relpath: "Parent Collection 2".to_string(),
    };
    collection::create_collection(&dto, &mut sharedstate)
        .expect("Failed to create parent collection");

    let dto = MoveTreeNodeDto {
        node_type: NodeType::Collection,
        src_relpath: "parent-collection-1".to_string(),
        dest_relpath: "parent-collection-2/parent-collection-1".to_string(),
    };

    let result = tree_node::move_tree_node(&dto, &mut sharedstate);
    assert!(result.is_ok());

    let space = sharedstate.space.unwrap();
    let parent_col =
        tree_node::find_collection(&space.root_collection, Path::new("parent-collection-2"))
            .unwrap();
    let moved_collection = parent_col
        .collections
        .iter()
        .find(|c| c.meta.fsname == "parent-collection-1")
        .unwrap();
    assert_eq!(
        moved_collection.meta.name,
        Some("Parent Collection 1".to_string())
    );
    assert_eq!(moved_collection.meta.fsname, "parent-collection-1");

    let space_path = Path::new(&space.abspath);
    assert!(!space_path.join("parent-collection-1").exists());
    assert!(space_path
        .join("parent-collection-2/parent-collection-1")
        .exists());
}

#[test]
fn move_tree_node_successfully_moves_request() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_sharedstate(tmp_dir.path());

    let dto = CreateRequestDto {
        parent_relpath: "".to_string(),
        relpath: "Parent Request 1".to_string(),
    };
    request::create_req(&dto, &mut sharedstate).expect("Failed to create request");

    let dto = CreateCollectionDto {
        parent_relpath: "".to_string(),
        relpath: "Parent Collection 1".to_string(),
    };
    collection::create_collection(&dto, &mut sharedstate)
        .expect("Failed to create parent collection");

    let dto = MoveTreeNodeDto {
        node_type: NodeType::Request,
        src_relpath: "parent-request-1.toml".to_string(),
        dest_relpath: "parent-collection-1/parent-request-1.toml".to_string(),
    };

    let result = tree_node::move_tree_node(&dto, &mut sharedstate);
    assert!(result.is_ok());

    let space = sharedstate.space.unwrap();
    let parent_col =
        tree_node::find_collection(&space.root_collection, Path::new("parent-collection-1"))
            .unwrap();
    let moved_request = parent_col
        .requests
        .iter()
        .find(|r| r.meta.fsname == "parent-request-1.toml")
        .unwrap();
    assert_eq!(moved_request.meta.name, "Parent Request 1");
    assert_eq!(moved_request.meta.fsname, "parent-request-1.toml");

    let space_path = Path::new(&space.abspath);
    assert!(!space_path.join("parent-request-1.toml").exists());
    assert!(space_path
        .join("parent-collection-1/parent-request-1.toml")
        .exists());
}

#[test]
fn move_tree_node_fails_with_missing_destination_parent_directory() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_sharedstate(tmp_dir.path());

    let dto = CreateCollectionDto {
        parent_relpath: "".to_string(),
        relpath: "Parent Collection 1".to_string(),
    };
    collection::create_collection(&dto, &mut sharedstate)
        .expect("Failed to create source collection");

    let dto = CreateCollectionDto {
        parent_relpath: "".to_string(),
        relpath: "Parent Collection 2".to_string(),
    };
    collection::create_collection(&dto, &mut sharedstate)
        .expect("Failed to create parent collection");

    let space = sharedstate.space.as_ref().unwrap();
    let space_path = Path::new(&space.abspath);
    fs::remove_dir_all(space_path.join("parent-collection-2"))
        .expect("Failed to remove parent directory");

    let dto = MoveTreeNodeDto {
        node_type: NodeType::Collection,
        src_relpath: "parent-collection-1".to_string(),
        dest_relpath: "parent-collection-2/parent-collection-1".to_string(),
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
    let mut sharedstate = tmp_sharedstate(tmp_dir.path());

    let dto = CreateCollectionDto {
        parent_relpath: "".to_string(),
        relpath: "Grand Parent Collection 1/Parent Collection 1/Child Collection 1".to_string(),
    };
    collection::create_collection(&dto, &mut sharedstate)
        .expect("Failed to create nested collection");

    let dto = MoveTreeNodeDto {
        node_type: NodeType::Collection,
        src_relpath: "grand-parent-collection-1/parent-collection-1/child-collection-1".to_string(),
        dest_relpath: "grand-parent-collection-1/child-collection-1".to_string(),
    };

    let result = tree_node::move_tree_node(&dto, &mut sharedstate);
    assert!(result.is_ok());

    let space = sharedstate.space.unwrap();
    let grandparent_col = tree_node::find_collection(
        &space.root_collection,
        Path::new("grand-parent-collection-1"),
    )
    .unwrap();
    let moved_collection = grandparent_col
        .collections
        .iter()
        .find(|c| c.meta.fsname == "child-collection-1")
        .unwrap();
    assert_eq!(
        moved_collection.meta.name,
        Some("Child Collection 1".to_string())
    );
    assert_eq!(moved_collection.meta.fsname, "child-collection-1");

    let parent_col = tree_node::find_collection(
        &space.root_collection,
        Path::new("grand-parent-collection-1/parent-collection-1"),
    )
    .unwrap();
    assert!(!parent_col
        .collections
        .iter()
        .any(|c| c.meta.fsname == "child-collection-1"));

    let space_path = Path::new(&space.abspath);
    assert!(!space_path
        .join("grand-parent-collection-1/parent-collection-1/child-collection-1")
        .exists());
    assert!(space_path
        .join("grand-parent-collection-1/child-collection-1")
        .exists());
}

#[test]
fn move_tree_node_successfully_moves_request_to_parent() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_sharedstate(tmp_dir.path());

    let dto = CreateCollectionDto {
        parent_relpath: "".to_string(),
        relpath: "Grand Parent Collection 1/Parent Collection 1".to_string(),
    };
    collection::create_collection(&dto, &mut sharedstate)
        .expect("Failed to create nested collection");

    let dto = CreateRequestDto {
        parent_relpath: "grand-parent-collection-1/parent-collection-1".to_string(),
        relpath: "Grand Child Request 1".to_string(),
    };
    request::create_req(&dto, &mut sharedstate).expect("Failed to create request");

    let dto = MoveTreeNodeDto {
        node_type: NodeType::Request,
        src_relpath: "grand-parent-collection-1/parent-collection-1/grand-child-request-1.toml"
            .to_string(),
        dest_relpath: "grand-parent-collection-1/grand-child-request-1.toml".to_string(),
    };

    let result = tree_node::move_tree_node(&dto, &mut sharedstate);
    assert!(result.is_ok());

    let space = sharedstate.space.unwrap();
    let grandparent_col = tree_node::find_collection(
        &space.root_collection,
        Path::new("grand-parent-collection-1"),
    )
    .unwrap();
    let moved_request = grandparent_col
        .requests
        .iter()
        .find(|r| r.meta.fsname == "grand-child-request-1.toml")
        .unwrap();
    assert_eq!(moved_request.meta.name, "Grand Child Request 1");
    assert_eq!(moved_request.meta.fsname, "grand-child-request-1.toml");

    let parent_col = tree_node::find_collection(
        &space.root_collection,
        Path::new("grand-parent-collection-1/parent-collection-1"),
    )
    .unwrap();
    assert!(!parent_col
        .requests
        .iter()
        .any(|r| r.meta.fsname == "grand-child-request-1.toml"));

    let space_path = Path::new(&space.abspath);
    assert!(!space_path
        .join("grand-parent-collection-1/parent-collection-1/grand-child-request-1.toml")
        .exists());
    assert!(space_path
        .join("grand-parent-collection-1/grand-child-request-1.toml")
        .exists());
}

#[test]
fn move_tree_node_successfully_moves_collection_to_grandparent() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_sharedstate(tmp_dir.path());

    let dto = CreateCollectionDto {
        parent_relpath: "".to_string(),
        relpath: "Great Grand Parent Collection 1/Grand Parent Collection 1/Parent Collection 1/Child Collection 1".to_string(),
    };
    collection::create_collection(&dto, &mut sharedstate)
        .expect("Failed to create deeply nested collection");

    let dto = MoveTreeNodeDto {
        node_type: NodeType::Collection,
        src_relpath: "great-grand-parent-collection-1/grand-parent-collection-1/parent-collection-1/child-collection-1".to_string(),
        dest_relpath: "great-grand-parent-collection-1/grand-parent-collection-1/child-collection-1".to_string(),
    };

    let result = tree_node::move_tree_node(&dto, &mut sharedstate);
    assert!(result.is_ok());

    let space = sharedstate.space.unwrap();
    let grandparent_col = tree_node::find_collection(
        &space.root_collection,
        Path::new("great-grand-parent-collection-1/grand-parent-collection-1"),
    )
    .unwrap();
    let moved_collection = grandparent_col
        .collections
        .iter()
        .find(|c| c.meta.fsname == "child-collection-1")
        .unwrap();
    assert_eq!(
        moved_collection.meta.name,
        Some("Child Collection 1".to_string())
    );
    assert_eq!(moved_collection.meta.fsname, "child-collection-1");

    let parent_col = tree_node::find_collection(
        &space.root_collection,
        Path::new("great-grand-parent-collection-1/grand-parent-collection-1/parent-collection-1"),
    )
    .unwrap();
    assert!(!parent_col
        .collections
        .iter()
        .any(|c| c.meta.fsname == "child-collection-1"));

    let space_path = Path::new(&space.abspath);
    assert!(!space_path.join("great-grand-parent-collection-1/grand-parent-collection-1/parent-collection-1/child-collection-1").exists());
    assert!(space_path
        .join("great-grand-parent-collection-1/grand-parent-collection-1/child-collection-1")
        .exists());
}

#[test]
fn move_tree_node_successfully_moves_request_to_grandparent() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let mut sharedstate = tmp_sharedstate(tmp_dir.path());

    let dto = CreateCollectionDto {
        parent_relpath: "".to_string(),
        relpath: "Great Grand Parent Collection 1/Grand Parent Collection 1/Parent Collection 1"
            .to_string(),
    };
    collection::create_collection(&dto, &mut sharedstate)
        .expect("Failed to create nested collection");

    let dto = CreateRequestDto {
        parent_relpath:
            "great-grand-parent-collection-1/grand-parent-collection-1/parent-collection-1"
                .to_string(),
        relpath: "Great Grand Child Request 1".to_string(),
    };
    request::create_req(&dto, &mut sharedstate).expect("Failed to create request");

    let dto = MoveTreeNodeDto {
        node_type: NodeType::Request,
        src_relpath: "great-grand-parent-collection-1/grand-parent-collection-1/parent-collection-1/great-grand-child-request-1.toml".to_string(),
        dest_relpath: "great-grand-parent-collection-1/great-grand-child-request-1.toml".to_string(),
    };

    let result = tree_node::move_tree_node(&dto, &mut sharedstate);
    assert!(result.is_ok());

    let space = sharedstate.space.unwrap();
    let great_grandparent_col = tree_node::find_collection(
        &space.root_collection,
        Path::new("great-grand-parent-collection-1"),
    )
    .unwrap();
    let moved_request = great_grandparent_col
        .requests
        .iter()
        .find(|r| r.meta.fsname == "great-grand-child-request-1.toml")
        .unwrap();
    assert_eq!(moved_request.meta.name, "Great Grand Child Request 1");
    assert_eq!(
        moved_request.meta.fsname,
        "great-grand-child-request-1.toml"
    );

    let parent_col = tree_node::find_collection(
        &space.root_collection,
        Path::new("great-grand-parent-collection-1/grand-parent-collection-1/parent-collection-1"),
    )
    .unwrap();
    assert!(!parent_col
        .requests
        .iter()
        .any(|r| r.meta.fsname == "great-grand-child-request-1.toml"));

    let space_path = Path::new(&space.abspath);
    assert!(!space_path.join("great-grand-parent-collection-1/grand-parent-collection-1/parent-collection-1/great-grand-child-request-1.toml").exists());
    assert!(space_path
        .join("great-grand-parent-collection-1/great-grand-child-request-1.toml")
        .exists());
}
