use serde::{Deserialize, Serialize};
use specta::Type;
use std::fs;
use std::path::Path;

use crate::collection::models::Collection;
use crate::error::{Error, Result};
use crate::state::SharedState;

#[derive(Debug, Clone, PartialEq)]
pub enum NodeType {
    Collection,
    Request,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct HandleTreeNodeDropDto {
    pub src_relpath: String,
    pub dest_relpath: String,
}

fn get_node_type(abspath: &Path) -> Result<NodeType> {
    if !abspath.exists() {
        return Err(Error::FileNotFound(format!(
            "Path does not exist: {}",
            abspath.display()
        )));
    }

    if abspath.is_dir() {
        let dirname = abspath
            .file_name()
            .ok_or_else(|| Error::InvalidPath("Invalid directory path".to_string()))?
            .to_string_lossy();

        if dirname == ".zaku" {
            return Err(Error::InvalidPath(
                "Cannot move .zaku directory".to_string(),
            ));
        }

        return Ok(NodeType::Collection);
    } else if abspath.is_file() {
        let filename = abspath
            .file_name()
            .ok_or_else(|| Error::InvalidPath("Invalid file path".to_string()))?
            .to_string_lossy();

        if filename.ends_with(".toml") {
            return Ok(NodeType::Request);
        }
    }

    Err(Error::InvalidPath("Invalid node type".to_string()))
}

pub fn find_collection<'a>(root: &'a Collection, relpath: &Path) -> Result<&'a Collection> {
    let mut cur_collection = root;

    for component in relpath.components() {
        if let std::path::Component::Normal(segment) = component {
            let segment_str = segment.to_string_lossy();
            cur_collection = cur_collection
                .collections
                .iter()
                .find(|col| col.meta.dir_name == segment_str)
                .ok_or_else(|| {
                    Error::InvalidPath(format!("Collection not found: {}", segment_str))
                })?;
        }
    }

    Ok(cur_collection)
}

fn find_collection_mut<'a>(root: &'a mut Collection, relpath: &Path) -> Result<&'a mut Collection> {
    let mut cur_collection = root;

    for component in relpath.components() {
        if let std::path::Component::Normal(segment) = component {
            let segment_str = segment.to_string_lossy();
            cur_collection = cur_collection
                .collections
                .iter_mut()
                .find(|col| col.meta.dir_name == segment_str)
                .ok_or_else(|| {
                    Error::InvalidPath(format!("Collection not found: {}", segment_str))
                })?;
        }
    }

    Ok(cur_collection)
}

fn execute_filesystem_move(src_abspath: &Path, dest_abspath: &Path) -> Result<()> {
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
            fs::create_dir_all(dest_dir)?;
        }
    }

    fs::rename(src_abspath, dest_abspath)?;

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

    let src_abspath = Path::new(&active_space.abspath).join(&dto.src_relpath);
    let dest_abspath = Path::new(&active_space.abspath).join(&dto.dest_relpath);
    let node_type = get_node_type(&src_abspath)?;

    if src_parent_relpath == dest_parent_relpath {
        return Err(Error::InvalidPath("Cannot drop to same parent".into()));
    }

    if node_type == NodeType::Collection {
        let collection_path = src_parent_relpath.join(&src_fsname);

        if dest_parent_relpath.starts_with(&collection_path) {
            return Err(Error::InvalidPath(
                "Cannot move collection into itself".into(),
            ));
        }
    }

    // Check for duplicates
    let dest_collection = find_collection(&active_space.root_collection, &dest_parent_relpath)?;

    match node_type {
        NodeType::Collection => {
            if dest_collection
                .collections
                .iter()
                .any(|c| c.meta.dir_name == src_fsname)
            {
                return Err(Error::InvalidPath(format!(
                    "Collection '{}' already exists",
                    src_fsname
                )));
            }
        }
        NodeType::Request => {
            if dest_collection
                .requests
                .iter()
                .any(|r| r.meta.file_name == src_fsname)
            {
                return Err(Error::InvalidPath(format!(
                    "Request '{}' already exists",
                    src_fsname
                )));
            }
        }
    }

    // Check source exists
    let src_collection = find_collection(&active_space.root_collection, &src_parent_relpath)?;

    match node_type {
        NodeType::Collection => {
            if !src_collection
                .collections
                .iter()
                .any(|c| c.meta.dir_name == src_fsname)
            {
                return Err(Error::InvalidPath(format!(
                    "Collection '{}' not found",
                    src_fsname
                )));
            }
        }
        NodeType::Request => {
            if !src_collection
                .requests
                .iter()
                .any(|r| r.meta.file_name == src_fsname)
            {
                return Err(Error::InvalidPath(format!(
                    "Request '{}' not found",
                    src_fsname
                )));
            }
        }
    }

    // Move in tree
    match node_type {
        NodeType::Collection => {
            let src_collection =
                find_collection_mut(&mut active_space.root_collection, &src_parent_relpath)?;
            let node_idx = src_collection
                .collections
                .iter()
                .position(|c| c.meta.dir_name == src_fsname)
                .unwrap();
            let collection = src_collection.collections.remove(node_idx);

            let dest_collection =
                find_collection_mut(&mut active_space.root_collection, &dest_parent_relpath)?;
            dest_collection.collections.push(collection);
            dest_collection.collections.sort_by(|a, b| {
                a.meta
                    .dir_name
                    .to_lowercase()
                    .cmp(&b.meta.dir_name.to_lowercase())
            });
        }
        NodeType::Request => {
            let src_collection =
                find_collection_mut(&mut active_space.root_collection, &src_parent_relpath)?;
            let node_idx = src_collection
                .requests
                .iter()
                .position(|r| r.meta.file_name == src_fsname)
                .unwrap();
            let request = src_collection.requests.remove(node_idx);

            let dest_collection =
                find_collection_mut(&mut active_space.root_collection, &dest_parent_relpath)?;
            dest_collection.requests.push(request);
            dest_collection
                .requests
                .sort_by(|a, b| a.meta.file_name.cmp(&b.meta.file_name));
        }
    }

    execute_filesystem_move(&src_abspath, &dest_abspath)?;

    Ok(())
}
