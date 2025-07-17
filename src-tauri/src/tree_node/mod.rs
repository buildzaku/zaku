use serde::{Deserialize, Serialize};
use specta::Type;
use std::{fmt, fs, path::Path};

use crate::collection::models::Collection;
use crate::error::{Error, Result};
use crate::state::SharedState;

#[cfg(test)]
pub mod tests;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Type)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    Collection,
    Request,
}

impl fmt::Display for NodeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeType::Collection => write!(f, "collection"),
            NodeType::Request => write!(f, "request"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct HandleTreeNodeDropDto {
    pub node_type: NodeType,
    pub src_relpath: String,
    pub dest_relpath: String,
}

/// Finds a collection within the collection tree by traversing the relative path
///
/// Walks through the collection hierarchy starting from the root collection,
/// following each path component to find the target collection.
///
/// - `root`: Root collection to start the search from
/// - `relpath`: Relative path to the target collection
///
/// Returns a `Result` containing a reference to the found collection
pub fn find_collection<'a>(root: &'a Collection, relpath: &Path) -> Result<&'a Collection> {
    let mut cur_collection = root;

    for component in relpath.components() {
        if let std::path::Component::Normal(segment) = component {
            let segment_str = segment.to_string_lossy();
            cur_collection = cur_collection
                .collections
                .iter()
                .find(|col| col.meta.fsname == segment_str)
                .ok_or_else(|| {
                    Error::InvalidPath(format!("Collection not found: {}", segment_str))
                })?;
        }
    }

    Ok(cur_collection)
}

/// Same as `find_collection` but returns a mutable reference to allow
/// modifications to the found collection.
///
/// - `root`: Root collection to start the search from
/// - `relpath`: Relative path to the target collection
///
/// Returns a `Result` containing a mutable reference to the found collection
fn find_collection_mut<'a>(root: &'a mut Collection, relpath: &Path) -> Result<&'a mut Collection> {
    let mut cur_collection = root;

    for component in relpath.components() {
        if let std::path::Component::Normal(segment) = component {
            let segment_str = segment.to_string_lossy();
            cur_collection = cur_collection
                .collections
                .iter_mut()
                .find(|col| col.meta.fsname == segment_str)
                .ok_or_else(|| {
                    Error::InvalidPath(format!("Collection not found: {}", segment_str))
                })?;
        }
    }

    Ok(cur_collection)
}

/// Moves a file or directory from source to destination path
///
/// Ensures the source exists, destination doesn't exist and all destination parent
/// directories exists before moving. Throws an error if any of these conditions
/// are not met.
///
/// - `src_abspath`: Absolute path of the source file/directory
/// - `dest_abspath`: Absolute path of the destination file/directory
///
/// Returns a `Result` indicating success or failure of the move operation
fn fsmove(src_abspath: &Path, dest_abspath: &Path) -> Result<()> {
    if !src_abspath.exists() {
        return Err(Error::FileNotFound(format!(
            "Source does not exist: {}",
            src_abspath.display()
        )));
    }

    if dest_abspath.exists() {
        return Err(Error::InvalidPath(format!(
            "Destination already exists: {}",
            dest_abspath.display()
        )));
    }

    if let Some(dest_dir) = dest_abspath.parent() {
        if !dest_dir.exists() {
            return Err(Error::InvalidPath(format!(
                "Destination parent directory does not exist: {}",
                dest_dir.display()
            )));
        }
    }

    fs::rename(src_abspath, dest_abspath)?;

    Ok(())
}

/// Checks if a node with the given name already exists in the destination collection
///
/// Searches through the appropriate collection (collections or requests) based
/// on the node type to determine if a node with the same filesystem name exists.
///
/// - `dest_parent_col`: Collection to check for existing nodes
/// - `node_type`: Type of node to check for (Collection or Request)
/// - `fsname`: Filesystem name to search for
///
/// Returns `true` if a node with the same name exists, `false` otherwise
fn node_exists_at_dest(dest_parent_col: &Collection, node_type: &NodeType, fsname: &str) -> bool {
    match node_type {
        NodeType::Collection => dest_parent_col
            .collections
            .iter()
            .any(|c| c.meta.fsname == fsname),
        NodeType::Request => dest_parent_col
            .requests
            .iter()
            .any(|r| r.meta.fsname == fsname),
    }
}

/// Checks if a source node exists in the source collection
///
/// Verifies that the node being moved actually exists in the source collection
/// before attempting the move operation.
///
/// - `src_parent_col`: Collection to check for the source node
/// - `node_type`: Type of node to check for (Collection or Request)
/// - `fsname`: Filesystem name to search for
///
/// Returns `true` if the source node exists, `false` otherwise
fn src_exists(src_parent_col: &Collection, node_type: &NodeType, fsname: &str) -> bool {
    match node_type {
        NodeType::Collection => src_parent_col
            .collections
            .iter()
            .any(|c| c.meta.fsname == fsname),
        NodeType::Request => src_parent_col
            .requests
            .iter()
            .any(|r| r.meta.fsname == fsname),
    }
}

/// Handles drag-and-drop operations for tree nodes (collections and requests)
///
/// Moves a node from source to destination path, updating both the filesystem
/// and the in-memory collection structure. Performs validation to ensure:
/// - Source and destination are different
/// - Collections cannot be moved into themselves
/// - No naming conflicts at destination
/// - Source node exists
///
/// After validation, removes the node from the source collection, adds it to
/// the destination collection, sorts the destination collection, and moves
/// the filesystem entry.
///
/// - `dto`: Contains node type and source/destination relative paths
/// - `sharedstate`: Shared state containing the space and collection tree
///
/// Returns a `Result` indicating success or failure of the drop operation
pub fn handle_tree_node_drop(
    dto: &HandleTreeNodeDropDto,
    sharedstate: &mut SharedState,
) -> Result<()> {
    let space = sharedstate
        .space
        .as_mut()
        .ok_or_else(|| Error::InvalidPath("No space found".to_string()))?;

    let src_fsname = Path::new(&dto.src_relpath)
        .file_name()
        .ok_or_else(|| Error::InvalidPath("Invalid source path".to_string()))?
        .to_string_lossy()
        .to_string();

    let src_parent_relpath = Path::new(&dto.src_relpath)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let dest_parent_relpath = Path::new(&dto.dest_relpath)
        .parent()
        .unwrap_or_else(|| Path::new(""));

    if src_parent_relpath == dest_parent_relpath {
        return Err(Error::InvalidPath("Cannot drop to same parent".into()));
    }

    if dto.node_type == NodeType::Collection {
        let collection_path = src_parent_relpath.join(&src_fsname);
        if dest_parent_relpath.starts_with(&collection_path) {
            return Err(Error::InvalidPath(
                "Cannot move collection into itself".into(),
            ));
        }
    }

    let dest_parent_col = find_collection(&space.root_collection, dest_parent_relpath)?;
    if node_exists_at_dest(&dest_parent_col, &dto.node_type, &src_fsname) {
        return Err(Error::InvalidPath(format!(
            "{} '{}' already exists",
            dto.node_type, src_fsname
        )));
    }

    let src_parent_col = find_collection(&space.root_collection, &src_parent_relpath)?;
    if !src_exists(&src_parent_col, &dto.node_type, &src_fsname) {
        return Err(Error::InvalidPath(format!(
            "{} '{}' not found",
            dto.node_type, src_fsname
        )));
    }

    match dto.node_type {
        NodeType::Collection => {
            let src_parent_col =
                find_collection_mut(&mut space.root_collection, &src_parent_relpath)?;
            let node_idx = src_parent_col
                .collections
                .iter()
                .position(|c| c.meta.fsname == src_fsname)
                .unwrap();
            let collection = src_parent_col.collections.remove(node_idx);

            let dest_parent_col =
                find_collection_mut(&mut space.root_collection, &dest_parent_relpath)?;
            dest_parent_col.collections.push(collection);
            dest_parent_col.collections.sort_by(|a, b| {
                a.meta
                    .fsname
                    .to_lowercase()
                    .cmp(&b.meta.fsname.to_lowercase())
            });
        }
        NodeType::Request => {
            let src_parent_col =
                find_collection_mut(&mut space.root_collection, &src_parent_relpath)?;
            let node_idx = src_parent_col
                .requests
                .iter()
                .position(|r| r.meta.fsname == src_fsname)
                .unwrap();
            let request = src_parent_col.requests.remove(node_idx);

            let dest_parent_col =
                find_collection_mut(&mut space.root_collection, &dest_parent_relpath)?;
            dest_parent_col.requests.push(request);
            dest_parent_col
                .requests
                .sort_by(|a, b| a.meta.fsname.cmp(&b.meta.fsname));
        }
    }

    let src_abspath = Path::new(&space.abspath).join(&dto.src_relpath);
    let dest_abspath = Path::new(&space.abspath).join(&dto.dest_relpath);
    fsmove(&src_abspath, &dest_abspath)?;

    Ok(())
}
