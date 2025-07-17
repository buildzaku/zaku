use serde::{Deserialize, Serialize};
use specta::Type;
use std::{fmt, fs, path::Path};

use crate::collection::models::Collection;
use crate::error::{Error, Result};
use crate::state::SharedState;

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

fn detect_node(abspath: &Path) -> Result<NodeType> {
    if !abspath.exists() {
        return Err(Error::FileNotFound(format!(
            "Path does not exist: {}",
            abspath.display()
        )));
    }

    let fsname = abspath
        .file_name()
        .ok_or_else(|| Error::InvalidPath("Invalid path: no file name".into()))?
        .to_string_lossy();

    if abspath.is_dir() && fsname != ".zaku" {
        return Ok(NodeType::Collection);
    } else if abspath.is_file() && fsname.ends_with(".toml") {
        return Ok(NodeType::Request);
    } else {
        return Err(Error::InvalidPath("Invalid node type".into()));
    }
}

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
            fs::create_dir_all(dest_dir)?;
        }
    }

    fs::rename(src_abspath, dest_abspath)?;

    Ok(())
}

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

    let src_abspath = Path::new(&space.abspath).join(&dto.src_relpath);
    let dest_abspath = Path::new(&space.abspath).join(&dto.dest_relpath);
    let node_type = detect_node(&src_abspath)?;

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

    let dest_parent_col = find_collection(&space.root_collection, dest_parent_relpath)?;
    if node_exists_at_dest(&dest_parent_col, &node_type, &src_fsname) {
        return Err(Error::InvalidPath(format!(
            "{} '{}' already exists",
            node_type, src_fsname
        )));
    }

    let src_parent_col = find_collection(&space.root_collection, &src_parent_relpath)?;
    if !src_exists(&src_parent_col, &node_type, &src_fsname) {
        return Err(Error::InvalidPath(format!(
            "{} '{}' not found",
            node_type, src_fsname
        )));
    }

    match node_type {
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

    fsmove(&src_abspath, &dest_abspath)?;

    Ok(())
}
