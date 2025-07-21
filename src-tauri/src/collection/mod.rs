use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    vec::IntoIter,
};

pub mod models;

#[cfg(test)]
pub mod tests;

use crate::{
    collection::models::{
        ColName, Collection, CollectionMeta, CollectionRcRefCell, CreateNewCollection,
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
    let colnames = ColName::load(space_abspath)?;
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

                    let relpath = entry_abspath.strip_prefix(space_abspath).unwrap();

                    let sub_collection = Rc::new(RefCell::new(CollectionRcRefCell {
                        meta: CollectionMeta {
                            fsname: name,
                            name: colnames.get(relpath),
                            is_expanded: true,
                        },
                        requests: Vec::new(),
                        collections: Vec::new(),
                    }));

                    stack.push((relpath.to_path_buf(), Rc::clone(&sub_collection)));
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

/// Creates parent collection directories if they don't exist
///
/// Parses the `relpath` into segments and creates any missing parent collection
/// directories, starting from the location relpath and working down the hierarchy
///
/// - `location_relpath`: Starting relative path within the space
/// - `relpath`: Path string to parse into collection segments
/// - `sharedstate`: Mutable reference to the application's shared state
///
/// Returns a `Result<(PathBuf, SanitizedSegment)>` with the final parent path and target segment
pub fn create_parent_collections_if_missing(
    location_relpath: &Path,
    relpath: &str,
    sharedstate: &mut SharedState,
) -> Result<(PathBuf, SanitizedSegment)> {
    let segments = utils::to_sanitized_segments(relpath)?;
    let (last_segment, relpath_segments) = segments.split_last().unwrap();

    let mut current_parent = location_relpath.to_path_buf();

    for segment in relpath_segments {
        let target_path = current_parent.join(&segment.fsname);

        let space = sharedstate
            .space
            .as_ref()
            .ok_or_else(|| Error::FileNotFound("Active space not found".to_string()))?;
        let space_abspath = PathBuf::from(&space.abspath);
        let dir_abspath = space_abspath.join(&target_path);

        if !dir_abspath.exists() {
            create_collection(&current_parent, segment, sharedstate)?;
        }

        current_parent = target_path;
    }

    Ok((current_parent, last_segment.clone()))
}

/// Creates a new collection directory in the specified parent path
///
/// Creates a new directory for the collection, saves the collection name mapping,
/// and updates the shared state
///
/// - `parent_relpath`: Relative path to the parent directory
/// - `col_segment`: Sanitized segment containing the collection name and filesystem name
/// - `sharedstate`: Mutable reference to the application's shared state
///
/// Returns a `Result<CreateNewCollection>` containing the created collection's paths
pub fn create_collection(
    parent_relpath: &Path,
    col_segment: &SanitizedSegment,
    sharedstate: &mut SharedState,
) -> Result<CreateNewCollection> {
    if col_segment.fsname.trim().is_empty() {
        return Err(Error::FileNotFound(
            "Cannot create a collection without name".to_string(),
        ));
    }

    let space = sharedstate
        .space
        .clone()
        .ok_or_else(|| Error::FileNotFound("Active space not found".to_string()))?;
    let space_abspath = PathBuf::from(&space.abspath);

    let dir_abspath = space_abspath.join(parent_relpath).join(&col_segment.fsname);
    let dir_relpath = parent_relpath.join(&col_segment.fsname);

    fs::create_dir(&dir_abspath)?;

    let mut colnames = ColName::load(&space_abspath)?;
    colnames.set(&dir_relpath, &col_segment.name)?;

    let create_new_collection = CreateNewCollection {
        parent_relpath: parent_relpath.to_string_lossy().to_string(),
        relpath: dir_relpath.to_string_lossy().to_string(),
    };

    sharedstate.space = Some(space::parse_space(&space_abspath)?);

    Ok(create_new_collection)
}
