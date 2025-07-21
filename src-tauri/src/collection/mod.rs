use std::{
    cell::RefCell,
    collections::HashMap,
    fs,
    path::{Component, Path, PathBuf},
    rc::Rc,
    vec::IntoIter,
};
use toml;

pub mod models;

#[cfg(test)]
pub mod tests;

use crate::{
    collection::models::{
        ColName, Collection, CollectionMeta, CollectionRcRefCell, CreateCollectionDto,
        CreateNewCollection,
    },
    error::{Error, Result},
    models::SanitizedSegment,
    request::{self, models::HttpReq},
    space::{self, parse_spacecfg},
    state::SharedState,
    store::spaces::buffer::SpaceBuf,
    utils,
};

pub fn parse_root_collection(space_abspath: &Path) -> Result<Collection> {
    let space_dirname = space_abspath
        .file_name()
        .unwrap_or(space_abspath.as_os_str())
        .to_string_lossy()
        .into_owned();
    let relative_space_root = "".to_string();
    let colname = colname_by_relpath(space_abspath).unwrap_or_else(|_| ColName {
        mappings: HashMap::new(),
    });
    let space_buffer = SpaceBuf::load(space_abspath)?;
    let spacebuf_rlock = space_buffer
        .read()
        .map_err(|_| Error::LockError("Failed to acquire read lock".into()))?;
    let space_config = parse_spacecfg(space_abspath).ok();

    let root_collection_ref_cell = Rc::new(RefCell::new(CollectionRcRefCell {
        meta: CollectionMeta {
            fsname: space_dirname,
            name: space_config.map(|config| config.meta.name),
            is_expanded: true,
        },
        requests: Vec::new(),
        collections: Vec::new(),
    }));

    let mut stack: Vec<(PathBuf, Rc<RefCell<CollectionRcRefCell>>)> = Vec::new();
    stack.push((
        PathBuf::from(&relative_space_root),
        Rc::clone(&root_collection_ref_cell),
    ));

    while let Some((path, collection_rc_refcell)) = stack.pop() {
        if let Ok(entries) = fs::read_dir(space_abspath.join(&path)) {
            let mut entries: Vec<_> = entries.flatten().collect();
            entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_lowercase());

            for entry in entries {
                let is_symlink = entry
                    .file_type()
                    .map(|file_type| file_type.is_symlink())
                    .unwrap_or(false);
                if is_symlink {
                    continue;
                }

                let entry_abspath = entry.path();

                if entry_abspath.is_dir() {
                    let name = entry_abspath
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned();
                    if name == ".zaku" {
                        continue;
                    }

                    let relpath = entry_abspath
                        .strip_prefix(space_abspath)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned();

                    let sub_collection = Rc::new(RefCell::new(CollectionRcRefCell {
                        meta: CollectionMeta {
                            fsname: name,
                            name: colname.mappings.get(&relpath).cloned(),
                            is_expanded: true,
                        },
                        requests: Vec::new(),
                        collections: Vec::new(),
                    }));

                    stack.push((PathBuf::from(&relpath), Rc::clone(&sub_collection)));
                    collection_rc_refcell
                        .borrow_mut()
                        .collections
                        .push(sub_collection);
                } else if entry_abspath.is_file() {
                    let req = request::parse_req(&entry_abspath, space_abspath, &spacebuf_rlock);
                    if let Some(req) = req {
                        collection_rc_refcell.borrow_mut().requests.push(req);
                    }
                }
            }

            collection_rc_refcell
                .borrow_mut()
                .collections
                .sort_by(|a, b| {
                    let a_meta = &a.borrow().meta;
                    let b_meta = &b.borrow().meta;
                    let a_name = a_meta
                        .name
                        .as_ref()
                        .unwrap_or(&a_meta.fsname)
                        .to_lowercase();
                    let b_name = b_meta
                        .name
                        .as_ref()
                        .unwrap_or(&b_meta.fsname)
                        .to_lowercase();

                    a_name.cmp(&b_name)
                });
            collection_rc_refcell
                .borrow_mut()
                .requests
                .sort_by(|a, b| a.meta.name.to_lowercase().cmp(&b.meta.name.to_lowercase()));
        }
    }

    let mut stack: Vec<(Collection, IntoIter<Rc<RefCell<CollectionRcRefCell>>>)> = Vec::new();
    let mut root_collection: Option<Collection> = None;

    {
        let root_collection_ref_cell = root_collection_ref_cell.borrow();
        let root_collection = Collection {
            meta: CollectionMeta {
                ..root_collection_ref_cell.meta.clone()
            },
            requests: root_collection_ref_cell
                .requests
                .iter()
                .map(|req| HttpReq { ..req.clone() })
                .collect(),
            collections: Vec::new(),
        };

        let sub_collections_iter = root_collection_ref_cell.collections.clone().into_iter();
        stack.push((root_collection, sub_collections_iter));
    }

    while let Some((cur_collection, mut sub_collections_iter)) = stack.pop() {
        if let Some(sub_collection_ref_cell) = sub_collections_iter.next() {
            stack.push((cur_collection, sub_collections_iter));

            let sub_collection_ref_cell = sub_collection_ref_cell.borrow();
            let sub_collection = Collection {
                meta: CollectionMeta {
                    ..sub_collection_ref_cell.meta.clone()
                },
                requests: sub_collection_ref_cell
                    .requests
                    .iter()
                    .map(|req| HttpReq { ..req.clone() })
                    .collect(),
                collections: Vec::new(),
            };

            let sub_collections_iter = sub_collection_ref_cell.collections.clone().into_iter();
            stack.push((sub_collection, sub_collections_iter));
        } else if let Some((mut parent_collection, parent_sub_collections_iter)) = stack.pop() {
            parent_collection.collections.push(cur_collection);
            stack.push((parent_collection, parent_sub_collections_iter));
        } else {
            root_collection = Some(cur_collection);
        }
    }

    root_collection
        .ok_or_else(|| Error::FileReadError("Failed to build collection: empty stack".to_string()))
}

/// Reads the collection names from `.zaku/collections/name.toml`
///
/// If the file doesn't exist, it creates a new one and returns an empty map. Used to
/// map sanitized relpaths back to their original names
///
/// - `space_abspath`: Absolute path of space
///
/// Returns a `Result` containing the collection's relpath-to-col-name map
pub fn colname_by_relpath(space_abspath: &Path) -> Result<ColName> {
    let colname_file_abspath = space_abspath.join(".zaku/collections/name.toml");

    let content = match fs::read_to_string(&colname_file_abspath) {
        Ok(content) => content,
        Err(_) => {
            if let Some(parent) = colname_file_abspath.parent() {
                fs::create_dir_all(parent)?;
            }

            let colname = ColName {
                mappings: HashMap::new(),
            };

            let serialized = toml::to_string_pretty(&colname)?;
            fs::write(&colname_file_abspath, &serialized)?;
            serialized
        }
    };

    let colname: ColName = toml::from_str(&content)?;

    Ok(colname)
}

/// Saves the collection's name in `.zaku/collections/name.toml` if
/// it doesn't already exist
///
/// This helps preserve the original casing and formatting for UI, while allowing
/// sanitized versions to be used as actual directory names
///
/// - `space_abspath`: Absolute path of space
/// - `collection_relpath`: Path relative to space where the collection resides
/// - `colname`: Original collection name
///
/// Returns a `Result` indicating success or failure
pub fn save_colname_if_missing(
    space_abspath: &Path,
    collection_relpath: &str,
    colname: &str,
) -> Result<()> {
    let colname_file_abspath = space_abspath.join(".zaku/collections/name.toml");

    let mut collection_name_by_relpath = colname_by_relpath(space_abspath)?;

    collection_name_by_relpath
        .mappings
        .entry(collection_relpath.to_string())
        .or_insert_with(|| colname.to_string());

    let toml_content = toml::to_string_pretty(&collection_name_by_relpath)?;

    fs::write(&colname_file_abspath, toml_content)?;

    Ok(())
}

// pub fn create_collections_all(space_abspath: &Path, dto: &CreateCollectionDto) -> Result<String> {
//     if dto.relpath.trim().is_empty() {
//         return Err(Error::FileNotFound("Collection name is missing".into()));
//     }

//     let relpath_no_bslashes = utils::rm_backslash(&dto.relpath);
//     let mut dirs = Vec::new();
//     for component in Path::new(&relpath_no_bslashes).components() {
//         if let std::path::Component::Normal(os_str) = component {
//             let colname = os_str.to_string_lossy();
//             let colname = colname.trim();
//             let dir_sanitized_name = utils::sanitize_name(colname);

//             if colname.is_empty() || dir_sanitized_name.is_empty() {
//                 continue;
//             }

//             dirs.push((dir_sanitized_name, colname.to_string()));
//         }
//     }

//     if dirs.is_empty() {
//         return Err(Error::InvalidPath(
//             "Collection path has no valid segments".into(),
//         ));
//     }

//     let collection_parent_abspath = space_abspath.join(&dto.parent_relpath);
//     let mut collections_relpath = String::new();

//     for (dir_sanitized_name, colname) in &dirs {
//         let cur_collection_relpath = PathBuf::from(&collections_relpath)
//             .join(dir_sanitized_name)
//             .to_string_lossy()
//             .to_string();

//         let target_dir = collection_parent_abspath.join(&cur_collection_relpath);
//         let dir_exists = fs::metadata(&target_dir).is_ok();
//         if !dir_exists {
//             fs::create_dir(&target_dir)?;
//         };

//         let cur_collection_relpath =
//             utils::join_strpaths(vec![&dto.parent_relpath, &cur_collection_relpath]);

//         save_colname_if_missing(space_abspath, &cur_collection_relpath, colname)
//             .map_err(|e| Error::FileReadError(format!("{cur_collection_relpath}: {e}")))?;

//         collections_relpath = PathBuf::from(&collections_relpath)
//             .join(dir_sanitized_name)
//             .to_string_lossy()
//             .to_string();
//     }

//     Ok(collections_relpath)
// }

pub fn create_collection_parents_if_missing(
    location_relpath: &Path,
    segments: &[SanitizedSegment],
    sharedstate: &mut SharedState,
) -> Result<PathBuf> {
    let mut parent_relpath = location_relpath.to_path_buf();

    for segment in segments {
        let acc_parent_relpath = parent_relpath.join(&segment.fsname);
        let space = sharedstate
            .space
            .as_ref()
            .ok_or_else(|| Error::FileNotFound("Active space not found".to_string()))?;
        let space_abspath = PathBuf::from(&space.abspath);
        let dir_abspath = space_abspath.join(&acc_parent_relpath);
        if !dir_abspath.exists() {
            create_collection(&parent_relpath, &segment.fsname, &segment.name, sharedstate)?;
        }

        parent_relpath = acc_parent_relpath;
    }

    Ok(parent_relpath)
}

pub fn create_collection(
    parent_relpath: &Path,
    fsname: &str,
    name: &str,
    sharedstate: &mut SharedState,
) -> Result<CreateNewCollection> {
    if fsname.trim().is_empty() {
        return Err(Error::FileNotFound(
            "Cannot create a collection without name".to_string(),
        ));
    }

    let space = sharedstate
        .space
        .clone()
        .ok_or_else(|| Error::FileNotFound("Active space not found".to_string()))?;
    let space_abspath = PathBuf::from(&space.abspath);

    let dir_abspath = space_abspath.join(parent_relpath).join(fsname);
    let dir_relpath = parent_relpath.join(fsname);

    fs::create_dir(&dir_abspath)?;

    save_colname_if_missing(&space_abspath, &dir_relpath.to_string_lossy(), name)?;

    let create_new_collection = CreateNewCollection {
        parent_relpath: parent_relpath.to_string_lossy().to_string(),
        relpath: dir_relpath.to_string_lossy().to_string(),
    };

    sharedstate.space = Some(space::parse_space(&space_abspath)?);

    Ok(create_new_collection)
}
