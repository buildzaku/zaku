use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    collection::models::Collection,
    error::{Error, Result},
    request::models::HttpReq,
    space::models::Space,
};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(tag = "type")]
pub enum DragPayload {
    Collection {
        parent_relative_path: String,
        node: Collection,
    },
    Request {
        parent_relative_path: String,
        node: HttpReq,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(tag = "type")]
pub enum DragOverTarget {
    Collection { relative_path: String },
    Request { parent_relative_path: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(tag = "type")]
pub enum RemoveTreeNodeTarget {
    Collection { dir_name: String },
    Request { file_name: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(tag = "type")]
pub enum FocusedTreeNode {
    Collection {
        parent_relative_path: String,
        relative_path: String,
    },
    Request {
        parent_relative_path: String,
        relative_path: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct ActiveRequest {
    pub parent_relative_path: String,
    pub request: HttpReq,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct MoveTreeNodeCommand {
    pub src_relpath: String,
    pub dest_relpath: String,
}

/// Splits a path string into non-empty segments by removing empty parts
///
/// Filters out empty segments that can occur from leading/trailing slashes
/// or consecutive slashes in the path.
///
/// - `path`: The path string to split (e.g., "api/v1/users" or "/api//v1/")
///
/// Returns a `Vec<&str>` containing the path segments
pub fn path_segments(path: &str) -> Vec<&str> {
    path.split('/').filter(|s| !s.is_empty()).collect()
}

/// Joins multiple path segments into a single path string
///
/// Filters out empty segments and joins the remaining segments with forward slashes.
/// Useful for reconstructing paths from segment arrays while avoiding double slashes.
///
/// - `segments`: Array of string references representing path segments
///
/// Returns a `String` with joined path segments (e.g., "api/v1/users")
pub fn join_paths(segments: &[&str]) -> String {
    segments
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join("/")
}

/// Builds a complete path by joining a current path with a tree node name
///
/// Handles the root case where current_path is empty by returning just the node name.
/// Otherwise combines the current path and node name with a forward slash.
///
/// - `current_path`: The base path (empty string for root, or path like "api/v1")
/// - `node_name`: The name of the tree node to append (e.g., "users", "auth.json")
///
/// Returns a `String` containing the complete path
pub fn build_path(current_path: &str, node_name: &str) -> String {
    if current_path.is_empty() {
        node_name.to_string()
    } else {
        format!("{}/{}", current_path, node_name)
    }
}

/// Checks if the current path is a subpath of the target path
///
/// Determines whether current_path represents a directory that contains or equals
/// the target_path. Used to prevent moving collections into their own subdirectories.
///
/// - `current_path`: The potential parent path to check
/// - `target_path`: The target path to validate against
///
/// Returns `true` if current_path is a subpath of target_path
pub fn is_subpath(current_path: &str, target_path: &str) -> bool {
    let current_segments = path_segments(current_path);
    let target_segments = path_segments(target_path);

    current_segments.len() <= target_segments.len()
        && current_segments
            .iter()
            .zip(target_segments.iter())
            .all(|(a, b)| a == b)
}

/// Validates whether a drag-and-drop operation is allowed
///
/// Performs validation checks to prevent invalid drop operations such as:
/// - Dropping on the same parent collection
/// - Moving collections into themselves or their children
/// - Invalid target paths for the current drag operation
///
/// - `drag_payload`: The node being dragged with its current location
/// - `drop_target_path`: The intended destination path for the drop
/// - `path`: The current path context for validation
///
/// Returns `true` if the drop operation is valid and allowed
pub fn is_drop_allowed(drag_payload: &DragPayload, drop_target_path: &str, path: &str) -> bool {
    let parent_path = match drag_payload {
        DragPayload::Collection {
            parent_relative_path,
            ..
        } => parent_relative_path,
        DragPayload::Request {
            parent_relative_path,
            ..
        } => parent_relative_path,
    };

    // Cannot drop on same parent
    if drop_target_path == parent_path {
        return false;
    }

    match drag_payload {
        DragPayload::Collection { node, .. } => {
            let node_path = build_path(parent_path, &node.meta.dir_name);

            // Cannot drop collection into itself or its children
            if is_subpath(&node_path, drop_target_path) {
                return false;
            }

            drop_target_path == path && drop_target_path != node_path
        }
        DragPayload::Request { .. } => drop_target_path == path && drop_target_path != parent_path,
    }
}

/// Checks if a focused tree node is within the specified collection path
///
/// Determines whether the currently focused tree node (collection or request)
/// is located within or is the specified collection. Used for UI state management
/// to highlight active collections and their contents.
///
/// - `collection_path`: The path of the collection to check against
/// - `focused_node`: The currently focused tree node (collection or request)
///
/// Returns `true` if the focused node is within the specified collection
pub fn is_focused_node_in_collection(
    collection_path: &str,
    focused_node: &FocusedTreeNode,
) -> bool {
    match focused_node {
        FocusedTreeNode::Collection { relative_path, .. } => relative_path == collection_path,
        FocusedTreeNode::Request {
            parent_relative_path,
            ..
        } => parent_relative_path == collection_path,
    }
}

/// Navigates to a specific collection within a space using a relative path
///
/// Traverses the collection hierarchy starting from the space root, following
/// the path segments to find the target collection. Each segment represents
/// a collection directory name in the nested structure.
///
/// - `space`: Mutable reference to the space containing collections
/// - `path`: Relative path to the target collection (e.g., "api/v1/auth")
///
/// Returns a `Result` containing a mutable reference to the target collection
pub fn navigate_to_collection<'a>(space: &'a mut Space, path: &str) -> Result<&'a mut Collection> {
    let segments = path_segments(path);

    // Handle empty path (root) - but Space is not a Collection!
    if segments.is_empty() {
        return Err(Error::InvalidPath(
            "Cannot navigate to root as it's not a collection".to_string(),
        ));
    }

    // Start with the first segment from space.collections
    let mut current = space
        .collections
        .iter_mut()
        .find(|col| col.meta.dir_name == segments[0])
        .ok_or_else(|| Error::FileNotFound(format!("Collection '{}' not found", segments[0])))?;

    // Navigate through the remaining segments
    for segment in &segments[1..] {
        current = current
            .collections
            .iter_mut()
            .find(|col| col.meta.dir_name == *segment)
            .ok_or_else(|| Error::FileNotFound(format!("Collection '{}' not found", segment)))?;
    }

    Ok(current)
}

/// Adds a collection to an existing parent collection with validation
///
/// Inserts a collection into the target collection's children, checking for
/// duplicate directory names and maintaining alphabetical sort order.
/// Prevents naming conflicts within the same collection.
///
/// - `collection`: The collection to add to the parent
/// - `target_collection`: The parent collection to add to
///
/// Returns a `Result` indicating success or validation error
pub fn add_collection_to_collection(
    collection: Collection,
    target_collection: &mut Collection,
) -> Result<()> {
    // Check for duplicate directory name
    if target_collection
        .collections
        .iter()
        .any(|col| col.meta.dir_name == collection.meta.dir_name)
    {
        return Err(Error::InvalidPath(format!(
            "Collection '{}' already exists in '{}'",
            collection.meta.dir_name, target_collection.meta.dir_name
        )));
    }

    target_collection.collections.push(collection);
    target_collection.collections.sort_by(|a, b| {
        a.meta
            .dir_name
            .to_lowercase()
            .cmp(&b.meta.dir_name.to_lowercase())
    });

    Ok(())
}

/// Adds a request to an existing collection with validation
///
/// Inserts a request into the target collection's requests list, checking for
/// duplicate file names and maintaining alphabetical sort order.
/// Prevents naming conflicts within the same collection.
///
/// - `request`: The HTTP request to add to the collection
/// - `target_collection`: The collection to add the request to
///
/// Returns a `Result` indicating success or validation error
pub fn add_request_to_collection(
    request: HttpReq,
    target_collection: &mut Collection,
) -> Result<()> {
    // Check for duplicate file name
    if target_collection
        .requests
        .iter()
        .any(|req| req.meta.file_name == request.meta.file_name)
    {
        return Err(Error::InvalidPath(format!(
            "Request '{}' already exists in '{}'",
            request.meta.file_name, target_collection.meta.dir_name
        )));
    }

    target_collection.requests.push(request);
    target_collection
        .requests
        .sort_by(|a, b| a.meta.file_name.cmp(&b.meta.file_name));

    Ok(())
}

/// Removes a tree node (collection or request) from a collection
///
/// Removes the specified tree node from the collection's children based on
/// the target type. Validates that the node exists before attempting removal.
///
/// - `target`: Specifies which collection or request to remove by name
/// - `collection`: The parent collection to remove the node from
///
/// Returns a `Result` indicating success or if the node was not found
pub fn remove_node_from_collection(
    target: &RemoveTreeNodeTarget,
    collection: &mut Collection,
) -> Result<()> {
    match target {
        RemoveTreeNodeTarget::Collection { dir_name } => {
            let initial_len = collection.collections.len();
            collection
                .collections
                .retain(|col| col.meta.dir_name != *dir_name);

            if collection.collections.len() == initial_len {
                return Err(Error::FileNotFound(format!(
                    "Collection '{}' not found",
                    dir_name
                )));
            }
        }
        RemoveTreeNodeTarget::Request { file_name } => {
            let initial_len = collection.requests.len();
            collection
                .requests
                .retain(|req| req.meta.file_name != *file_name);

            if collection.requests.len() == initial_len {
                return Err(Error::FileNotFound(format!(
                    "Request '{}' not found",
                    file_name
                )));
            }
        }
    }

    Ok(())
}

/// Creates a filesystem move command from drag payload and target path
///
/// Translates a logical drag-and-drop operation into filesystem paths for
/// moving files/directories. Extracts the current location and destination
/// to create the appropriate move command for the underlying storage.
///
/// - `drag_payload`: The tree node being moved with its current location
/// - `target_path`: The destination path where the node should be moved
///
/// Returns a `MoveTreeNodeCommand` with source and destination relative paths
pub fn create_move_command(drag_payload: &DragPayload, target_path: &str) -> MoveTreeNodeCommand {
    let (parent_relative_path, node_name) = match drag_payload {
        DragPayload::Collection {
            parent_relative_path,
            node,
        } => (parent_relative_path, &node.meta.dir_name),
        DragPayload::Request {
            parent_relative_path,
            node,
        } => (parent_relative_path, &node.meta.file_name),
    };

    MoveTreeNodeCommand {
        src_relpath: build_path(parent_relative_path, node_name),
        dest_relpath: build_path(target_path, node_name),
    }
}

/// Processes a complete drag-and-drop operation within the space tree structure
///
/// Handles the full drag-and-drop workflow by validating the operation,
/// adding the tree node to its new location, removing it from the old location,
/// and creating the corresponding filesystem move command.
///
/// - `drag_payload`: The tree node being moved with its current location
/// - `drop_target_path`: The destination path for the drop operation
/// - `space`: Mutable reference to the space containing the tree structure
///
/// Returns a `Result` containing the filesystem move command to execute
pub fn process_drag_drop(
    drag_payload: &DragPayload,
    drop_target_path: &str,
    space: &mut Space,
) -> Result<MoveTreeNodeCommand> {
    let parent_path = match drag_payload {
        DragPayload::Collection {
            parent_relative_path,
            ..
        } => parent_relative_path,
        DragPayload::Request {
            parent_relative_path,
            ..
        } => parent_relative_path,
    };

    // Validate the operation
    if drop_target_path == parent_path {
        return Err(Error::InvalidPath(
            "Cannot drop node to the same parent".to_string(),
        ));
    }

    // Additional validation for collections
    if let DragPayload::Collection { node, .. } = drag_payload {
        let node_path = build_path(parent_path, &node.meta.dir_name);
        if is_subpath(&node_path, drop_target_path) {
            return Err(Error::InvalidPath(
                "Cannot move collection into itself or its children".to_string(),
            ));
        }
    }

    // Navigate to target collection and add node
    let target_collection = navigate_to_collection(space, drop_target_path)?;
    match drag_payload {
        DragPayload::Collection { node, .. } => {
            add_collection_to_collection(node.clone(), target_collection)?;
        }
        DragPayload::Request { node, .. } => {
            add_request_to_collection(node.clone(), target_collection)?;
        }
    }

    // Navigate to parent collection and remove node
    let parent_collection = navigate_to_collection(space, parent_path)?;
    let remove_target = match drag_payload {
        DragPayload::Collection { node, .. } => RemoveTreeNodeTarget::Collection {
            dir_name: node.meta.dir_name.clone(),
        },
        DragPayload::Request { node, .. } => RemoveTreeNodeTarget::Request {
            file_name: node.meta.file_name.clone(),
        },
    };
    remove_node_from_collection(&remove_target, parent_collection)?;

    Ok(create_move_command(drag_payload, drop_target_path))
}

/// Moves a file or directory on the filesystem based on the move command
///
/// Executes the actual filesystem operation to move a tree node from its source
/// location to its destination. Creates parent directories as needed and handles
/// both file and directory moves within the space's absolute path.
///
/// - `move_command`: Contains source and destination relative paths for the move
/// - `space_abspath`: The absolute path of the space root directory
///
/// Returns a `Result` indicating success or filesystem error
pub fn move_filesystem_node(move_command: &MoveTreeNodeCommand, space_abspath: &str) -> Result<()> {
    let src_path = std::path::Path::new(space_abspath).join(&move_command.src_relpath);
    let dest_path = std::path::Path::new(space_abspath).join(&move_command.dest_relpath);

    // Ensure destination directory exists
    if let Some(parent) = dest_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Move the file/directory
    std::fs::rename(&src_path, &dest_path)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_segments() {
        assert_eq!(path_segments(""), Vec::<&str>::new());
        assert_eq!(path_segments("/"), Vec::<&str>::new());
        assert_eq!(path_segments("a"), vec!["a"]);
        assert_eq!(path_segments("a/b/c"), vec!["a", "b", "c"]);
        assert_eq!(path_segments("/a/b/c"), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_join_paths() {
        assert_eq!(join_paths(&[]), "");
        assert_eq!(join_paths(&["a"]), "a");
        assert_eq!(join_paths(&["a", "b", "c"]), "a/b/c");
        assert_eq!(join_paths(&["", "a", "", "b"]), "a/b");
    }

    #[test]
    fn test_build_path() {
        assert_eq!(build_path("", "test"), "test");
        assert_eq!(build_path("parent", "test"), "parent/test");
    }

    #[test]
    fn test_is_subpath() {
        assert!(is_subpath("", "a/b/c"));
        assert!(is_subpath("a", "a/b/c"));
        assert!(is_subpath("a/b", "a/b/c"));
        assert!(is_subpath("a/b/c", "a/b/c"));
        assert!(!is_subpath("a/b/c", "a/b"));
        assert!(!is_subpath("a/c", "a/b/c"));
    }
}
