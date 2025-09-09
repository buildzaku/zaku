use serde::{Deserialize, Serialize};
use specta::Type;
use std::{
    fmt, fs,
    path::{self, Path, PathBuf},
};

use crate::{
    collection::models::Collection,
    error::{Error, Result},
    space,
    store::collection::SpaceCollectionsMetadataStore,
};

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
pub struct MoveTreeNodeDto {
    pub node_type: NodeType,
    pub cur_relpath: PathBuf,
    pub nxt_relpath: PathBuf,
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
        if let path::Component::Normal(segment) = component {
            let segment_str = segment.to_string_lossy();
            cur_collection = cur_collection
                .collections
                .iter()
                .find(|col| col.meta.fsname == segment_str)
                .ok_or_else(|| {
                    Error::InvalidPath(format!("Collection not found: {segment_str}"))
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
/// - `cur_abspath`: Absolute path of the source file/directory
/// - `nxt_abspath`: Absolute path of the destination file/directory
///
/// Returns a `Result` indicating success or failure of the move operation
fn fsmove(cur_abspath: &Path, nxt_abspath: &Path) -> Result<()> {
    if !cur_abspath.exists() {
        return Err(Error::FileNotFound(format!(
            "Source does not exist: {}",
            cur_abspath.display()
        )));
    }

    if nxt_abspath.exists() {
        return Err(Error::InvalidPath(format!(
            "Destination already exists: {}",
            nxt_abspath.display()
        )));
    }

    if let Some(nxt_dir) = nxt_abspath.parent()
        && !nxt_dir.exists()
    {
        return Err(Error::InvalidPath(format!(
            "Destination parent directory does not exist: {}",
            nxt_dir.display()
        )));
    }

    fs::rename(cur_abspath, nxt_abspath)?;

    Ok(())
}

/// Checks if a node with the given name already exists in the destination collection
///
/// Searches through the appropriate collection (collections or requests) based
/// on the node type to determine if a node with the same filesystem name exists.
///
/// - `nxt_parent_col`: Collection to check for existing nodes
/// - `node_type`: Type of node to check for (Collection or Request)
/// - `fsname`: Filesystem name to search for
///
/// Returns `true` if a node with the same name exists, `false` otherwise
fn node_exists_at_dest(nxt_parent_col: &Collection, node_type: &NodeType, fsname: &str) -> bool {
    match node_type {
        NodeType::Collection => nxt_parent_col
            .collections
            .iter()
            .any(|c| c.meta.fsname == fsname),
        NodeType::Request => nxt_parent_col
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
/// - `cur_parent_col`: Collection to check for the source node
/// - `node_type`: Type of node to check for (Collection or Request)
/// - `fsname`: Filesystem name to search for
///
/// Returns `true` if the source node exists, `false` otherwise
fn cur_exists(cur_parent_col: &Collection, node_type: &NodeType, fsname: &str) -> bool {
    match node_type {
        NodeType::Collection => cur_parent_col
            .collections
            .iter()
            .any(|c| c.meta.fsname == fsname),
        NodeType::Request => cur_parent_col
            .requests
            .iter()
            .any(|r| r.meta.fsname == fsname),
    }
}

/// Moves a node from source to destination path, updating both the filesystem
/// and collections in shared state. Performs validation to ensure:
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
/// - `space_abspath`: Absolute path to the space directory
///
/// Returns a `Result` indicating success or failure of the drop operation
pub fn move_tree_node(dto: &MoveTreeNodeDto, space_abspath: &Path) -> Result<()> {
    let space = space::parse_space(space_abspath)?;

    let cur_fsname = dto
        .cur_relpath
        .file_name()
        .ok_or_else(|| Error::InvalidPath("Invalid source path".to_string()))?
        .to_string_lossy()
        .to_string();

    let cur_parent_relpath = dto.cur_relpath.parent().unwrap_or_else(|| Path::new(""));
    let nxt_parent_relpath = dto.nxt_relpath.parent().unwrap_or_else(|| Path::new(""));

    if cur_parent_relpath == nxt_parent_relpath {
        return Err(Error::InvalidPath("Cannot drop to same parent".into()));
    }

    if dto.node_type == NodeType::Collection {
        let collection_path = cur_parent_relpath.join(&cur_fsname);
        if nxt_parent_relpath.starts_with(&collection_path) {
            return Err(Error::InvalidPath(
                "Cannot move collection into itself".into(),
            ));
        }
    }

    let nxt_parent_col = find_collection(&space.root_collection, nxt_parent_relpath)?;
    if node_exists_at_dest(nxt_parent_col, &dto.node_type, &cur_fsname) {
        return Err(Error::InvalidPath(format!(
            "{} '{}' already exists",
            dto.node_type, cur_fsname
        )));
    }

    let cur_parent_col = find_collection(&space.root_collection, cur_parent_relpath)?;
    if !cur_exists(cur_parent_col, &dto.node_type, &cur_fsname) {
        return Err(Error::InvalidPath(format!(
            "{} '{}' not found",
            dto.node_type, cur_fsname
        )));
    }

    let cur_abspath = space_abspath.join(&dto.cur_relpath);
    let nxt_abspath = space_abspath.join(&dto.nxt_relpath);

    fsmove(&cur_abspath, &nxt_abspath)?;

    if dto.node_type == NodeType::Collection {
        let mut scmt_store = SpaceCollectionsMetadataStore::get(space_abspath)?;
        scmt_store.update(|metadata| {
            if let Some(name) = metadata.mappings.remove(&dto.cur_relpath) {
                metadata.mappings.insert(dto.nxt_relpath.clone(), name);
            }
        })?;
    }

    Ok(())
}
