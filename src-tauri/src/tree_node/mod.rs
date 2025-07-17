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

pub fn navigate_to_collection<'a>(
    root: &'a mut Collection,
    relpath: &str,
) -> Result<&'a mut Collection> {
    let segments = path_segments(relpath);
    let mut current = root;

    for segment in segments {
        current = current
            .collections
            .iter_mut()
            .find(|col| col.meta.dir_name == segment)
            .ok_or_else(|| Error::InvalidPath(format!("Collection not found: {segment}")))?;
    }

    Ok(current)
}

fn filesystem_move(src: &Path, dest: &Path) -> Result<()> {
    // Check if source exists
    if !src.exists() {
        return Err(Error::FileNotFound(format!(
            "Source path does not exist: {}",
            src.display()
        )));
    }

    // Check if destination already exists
    if dest.exists() {
        return Err(Error::InvalidPath(format!(
            "Destination path already exists: {}",
            dest.display()
        )));
    }

    // Ensure destination directory exists
    if let Some(dest_dir) = dest.parent() {
        if !dest_dir.exists() {
            fs::create_dir_all(dest_dir)?;
        }
    }

    // Perform move
    fs::rename(src, dest)?;
    Ok(())
}

pub fn handle_tree_node_drop(
    dto: &HandleTreeNodeDropDto,
    sharedstate: &mut SharedState,
) -> Result<()> {
    let active_space = sharedstate
        .active_space
        .as_mut()
        .ok_or_else(|| Error::InvalidPath("No active space found".to_string()))?;

    let src_abspath = Path::new(&active_space.abspath).join(&dto.src_relpath);
    let dest_abspath = Path::new(&active_space.abspath).join(&dto.dest_relpath);

    let src_filename = Path::new(&dto.src_relpath)
        .file_name()
        .ok_or_else(|| Error::InvalidPath("Invalid source path".to_string()))?
        .to_string_lossy()
        .to_string();

    let src_parent = Path::new(&dto.src_relpath)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let dest_parent = Path::new(&dto.dest_relpath)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    println!("Debug info:");
    println!("  src_relpath: {}", dto.src_relpath);
    println!("  dest_relpath: {}", dto.dest_relpath);
    println!("  src_filename: {src_filename}");
    println!("  src_parent: '{src_parent}'");
    println!("  dest_parent: '{dest_parent}'");
    println!("  dest_parent segments: {:?}", path_segments(&dest_parent));

    // Validate node type
    let src_abs_path = Path::new(&active_space.abspath).join(&dto.src_relpath);
    let is_collection = src_abs_path.is_dir() && {
        let dirname = src_abs_path
            .file_name()
            .ok_or_else(|| Error::InvalidPath("Invalid source directory path".to_string()))?
            .to_string_lossy();
        dirname != ".zaku"
    };

    let is_request = src_abs_path.is_file() && {
        let filename = src_abs_path
            .file_name()
            .ok_or_else(|| Error::InvalidPath("Invalid source file path".to_string()))?
            .to_string_lossy();
        filename.ends_with(".toml")
    };

    if !is_collection && !is_request {
        return Err(Error::InvalidPath(
            "Source must be either a directory (not .zaku) or a .toml file".to_string(),
        ));
    }

    // Validate not same parent
    if src_parent == dest_parent {
        return Err(Error::InvalidPath(
            "Cannot drop node to the same parent".to_string(),
        ));
    }

    // Additional validation for collections moving into themselves
    if is_collection {
        let collection_path = if src_parent.is_empty() {
            src_filename.clone()
        } else {
            format!("{src_parent}/{src_filename}")
        };

        if !dest_parent.is_empty() && dest_parent.starts_with(&collection_path) {
            return Err(Error::InvalidPath(
                "Cannot move collection into itself or its children".to_string(),
            ));
        }
    }

    // TRANSACTION PATTERN: Validate destination first (no mutations yet)

    // Check if destination collection exists and validate duplicates
    if !dest_parent.is_empty() {
        // Make sure destination collection exists
        navigate_to_collection(&mut active_space.root_collection, &dest_parent)?;
    }

    // Check for duplicates in destination
    let dest_collection = if dest_parent.is_empty() {
        &active_space.root_collection
    } else {
        navigate_to_collection(&mut active_space.root_collection, &dest_parent)?
    };

    if is_collection {
        if dest_collection
            .collections
            .iter()
            .any(|c| c.meta.dir_name == src_filename)
        {
            return Err(Error::InvalidPath(format!(
                "Collection with directory name '{src_filename}' already exists"
            )));
        }
    } else if dest_collection
        .requests
        .iter()
        .any(|r| r.meta.file_name == src_filename)
    {
        return Err(Error::InvalidPath(format!(
            "Request with file name '{src_filename}' already exists"
        )));
    }

    // Check if source exists in its parent
    let src_collection = if src_parent.is_empty() {
        &active_space.root_collection
    } else {
        navigate_to_collection(&mut active_space.root_collection, &src_parent)?
    };

    if is_collection {
        if !src_collection
            .collections
            .iter()
            .any(|col| col.meta.dir_name == src_filename)
        {
            return Err(Error::InvalidPath(format!(
                "Collection not found: {src_filename}"
            )));
        }
    } else if !src_collection
        .requests
        .iter()
        .any(|req| req.meta.file_name == src_filename)
    {
        return Err(Error::InvalidPath(format!(
            "Request not found: {src_filename}"
        )));
    }

    // All validations passed, now perform the operations

    // Step 1: Remove from source
    let src_parent_collection = if src_parent.is_empty() {
        &mut active_space.root_collection
    } else {
        navigate_to_collection(&mut active_space.root_collection, &src_parent)?
    };

    let tree_node = if is_collection {
        let pos = src_parent_collection
            .collections
            .iter()
            .position(|col| col.meta.dir_name == src_filename)
            .unwrap(); // We already validated this exists

        TreeNode::Collection(Box::new(src_parent_collection.collections.remove(pos)))
    } else {
        let pos = src_parent_collection
            .requests
            .iter()
            .position(|req| req.meta.file_name == src_filename)
            .unwrap(); // We already validated this exists

        TreeNode::Request(Box::new(src_parent_collection.requests.remove(pos)))
    };

    // Step 2: Add to destination
    let dest_parent_collection = if dest_parent.is_empty() {
        &mut active_space.root_collection
    } else {
        navigate_to_collection(&mut active_space.root_collection, &dest_parent)?
    };

    match tree_node {
        TreeNode::Collection(collection) => {
            dest_parent_collection.collections.push(*collection);
            dest_parent_collection.collections.sort_by(|a, b| {
                a.meta
                    .dir_name
                    .to_lowercase()
                    .cmp(&b.meta.dir_name.to_lowercase())
            });
        }
        TreeNode::Request(request) => {
            dest_parent_collection.requests.push(*request);
            dest_parent_collection
                .requests
                .sort_by(|a, b| a.meta.file_name.cmp(&b.meta.file_name));
        }
    }

    filesystem_move(&src_abspath, &dest_abspath)?;

    Ok(())
}
