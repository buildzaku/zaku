use serde::{Deserialize, Serialize};
use specta::Type;
use std::fs;
use std::path::Path;

use crate::collection::models::Collection;
use crate::error::{Error, Result};
use crate::request::models::HttpReq;
use crate::state::SharedState;

// DTO for the drop operation
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct HandleTreeNodeDropDto {
    pub src_relpath: String,
    pub dest_relpath: String,
}

#[derive(Clone, Debug)]
enum TreeNode {
    Collection(Box<Collection>),
    Request(Box<HttpReq>),
}

// Utility functions
fn path_segments(path: &str) -> Vec<&str> {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn navigate_to_collection<'a>(root: &'a mut Collection, path: &str) -> Result<&'a mut Collection> {
    let segments = path_segments(path);
    let mut current = root;

    for segment in segments {
        current = current
            .collections
            .iter_mut()
            .find(|col| col.meta.dir_name == segment)
            .ok_or_else(|| Error::InvalidPath(format!("Collection not found: {}", segment)))?;
    }

    Ok(current)
}

// Main handle drop function
pub fn handle_tree_node_drop(
    dto: &HandleTreeNodeDropDto,
    sharedstate: &mut SharedState,
) -> Result<()> {
    // Get the active space
    let active_space = sharedstate
        .active_space
        .as_mut()
        .ok_or_else(|| Error::InvalidPath("No active space found".to_string()))?;

    // Parse source and destination paths
    let src_path = Path::new(&dto.src_relpath);
    let dest_path = Path::new(&dto.dest_relpath);

    let src_filename = src_path
        .file_name()
        .ok_or_else(|| Error::InvalidPath("Invalid source path".to_string()))?
        .to_string_lossy()
        .to_string();

    let src_parent = src_path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let dest_parent = dest_path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    // Check if it's a collection or request based on file extension
    let is_collection = !src_filename.ends_with(".http");

    // Basic validation
    if src_parent == dest_parent {
        return Err(Error::InvalidPath(
            "Cannot drop node to the same parent".to_string(),
        ));
    }

    // Find the source parent collection
    let src_parent_collection = if src_parent.is_empty() {
        &mut active_space.root_collection
    } else {
        navigate_to_collection(&mut active_space.root_collection, &src_parent)?
    };

    // Find and remove the tree node from source
    let tree_node = if is_collection {
        // Find and remove collection
        let pos = src_parent_collection
            .collections
            .iter()
            .position(|col| col.meta.dir_name == src_filename)
            .ok_or_else(|| Error::InvalidPath(format!("Collection not found: {src_filename}")))?;

        let collection = src_parent_collection.collections.remove(pos);

        // Validate: don't allow moving collection into itself or its children
        if !dest_parent.is_empty()
            && dest_parent.starts_with(&format!("{src_parent}/{src_filename}"))
        {
            // Put it back and return error
            src_parent_collection.collections.insert(pos, collection);
            return Err(Error::InvalidPath(
                "Cannot move collection into itself or its children".to_string(),
            ));
        }

        TreeNode::Collection(Box::new(collection))
    } else {
        // Find and remove request
        let pos = src_parent_collection
            .requests
            .iter()
            .position(|req| req.meta.file_name == src_filename)
            .ok_or_else(|| Error::InvalidPath(format!("Request not found: {}", src_filename)))?;

        TreeNode::Request(Box::new(src_parent_collection.requests.remove(pos)))
    };

    // Find the destination parent collection
    let dest_parent_collection = if dest_parent.is_empty() {
        &mut active_space.root_collection
    } else {
        navigate_to_collection(&mut active_space.root_collection, &dest_parent)?
    };

    // Add the tree node to destination
    match tree_node {
        TreeNode::Collection(collection) => {
            // Check if collection with same dir_name already exists
            if dest_parent_collection
                .collections
                .iter()
                .any(|c| c.meta.dir_name == collection.meta.dir_name)
            {
                return Err(Error::InvalidPath(format!(
                    "Collection with directory name '{}' already exists",
                    collection.meta.dir_name
                )));
            }

            dest_parent_collection.collections.push(*collection);
            dest_parent_collection.collections.sort_by(|a, b| {
                a.meta
                    .dir_name
                    .to_lowercase()
                    .cmp(&b.meta.dir_name.to_lowercase())
            });
        }
        TreeNode::Request(request) => {
            // Check if request with same file_name already exists
            if dest_parent_collection
                .requests
                .iter()
                .any(|r| r.meta.file_name == request.meta.file_name)
            {
                return Err(Error::InvalidPath(format!(
                    "Request with file name '{}' already exists",
                    request.meta.file_name
                )));
            }

            dest_parent_collection.requests.push(*request);
            dest_parent_collection
                .requests
                .sort_by(|a, b| a.meta.file_name.cmp(&b.meta.file_name));
        }
    }

    // Move the actual file/directory on the filesystem
    let src_full_path = Path::new(&active_space.abspath).join(&dto.src_relpath);
    let dest_full_path = Path::new(&active_space.abspath).join(&dto.dest_relpath);

    // Ensure the destination directory exists
    if let Some(dest_dir) = dest_full_path.parent() {
        if !dest_dir.exists() {
            fs::create_dir_all(dest_dir)?;
        }
    }

    // Check if source exists
    if !src_full_path.exists() {
        return Err(Error::FileNotFound(format!(
            "Source path does not exist: {}",
            src_full_path.display()
        )));
    }

    // Check if destination already exists
    if dest_full_path.exists() {
        return Err(Error::InvalidPath(format!(
            "Destination path already exists: {}",
            dest_full_path.display()
        )));
    }

    // Perform the move
    fs::rename(&src_full_path, &dest_full_path)?;

    Ok(())
}
