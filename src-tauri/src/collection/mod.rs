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
        Collection, CollectionMeta, CollectionRcRefCell, CreateNewCollection,
        SpaceCollectionsMetadataStore,
    },
    error::{Error, Result},
    models::SanitizedSegment,
    request::{self, models::HttpReq},
    space::parse_spacecfg,
    store::{self, SpaceBufferStore, StateStore},
    utils,
};

pub fn parse_root_collection(space_abspath: &Path, state_store: &StateStore) -> Result<Collection> {
    let space_dirname = space_abspath
        .file_name()
        .unwrap_or(space_abspath.as_os_str())
        .to_string_lossy()
        .into_owned();
    let spaceroot_relpath = PathBuf::from("");
    let scmt_store = SpaceCollectionsMetadataStore::get(space_abspath)?;

    let sbf_store_abspath =
        store::utils::sbf_store_abspath(state_store.datadir_abspath(), space_abspath);
    let sbf_store = SpaceBufferStore::get(&sbf_store_abspath)?;
    let sbf_store_mtx = sbf_store
        .lock()
        .map_err(|_| Error::LockError("Failed to acquire mutex lock".into()))?;
    let space_config = parse_spacecfg(space_abspath).ok();

    let root_collection_ref_cell = Rc::new(RefCell::new(CollectionRcRefCell {
        meta: CollectionMeta {
            fsname: space_dirname,
            name: space_config.map(|config| config.meta.name),
            is_expanded: true,
            relpath: spaceroot_relpath.clone(),
        },
        requests: Vec::new(),
        collections: Vec::new(),
    }));

    let mut stack: Vec<(PathBuf, Rc<RefCell<CollectionRcRefCell>>)> = Vec::new();
    stack.push((spaceroot_relpath, Rc::clone(&root_collection_ref_cell)));

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

                    let relpath = entry_abspath.strip_prefix(space_abspath)?.to_path_buf();
                    let sub_collection = Rc::new(RefCell::new(CollectionRcRefCell {
                        meta: CollectionMeta {
                            fsname: name,
                            name: scmt_store.mappings.get(&relpath).cloned(),
                            is_expanded: true,
                            relpath: relpath.clone(),
                        },
                        requests: Vec::new(),
                        collections: Vec::new(),
                    }));

                    stack.push((relpath, Rc::clone(&sub_collection)));
                    collection_rc_refcell
                        .borrow_mut()
                        .collections
                        .push(sub_collection);
                } else if entry_abspath.is_file() {
                    let req = request::parse_req(&entry_abspath, space_abspath, &sbf_store_mtx);
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
/// - `space_abspath`: Absolute path to the space directory
///
/// Returns a `Result<(PathBuf, SanitizedSegment)>` with the final parent path and target segment
pub fn create_parent_collections_if_missing(
    location_relpath: &Path,
    relpath: &Path,
    space_abspath: &Path,
) -> Result<(PathBuf, SanitizedSegment)> {
    let segments = utils::to_sanitized_segments(relpath)?;
    let (last_segment, relpath_segments) = segments.split_last().unwrap();

    let mut location_relpath = location_relpath.to_path_buf();
    for segment in relpath_segments {
        let target_path = location_relpath.join(&segment.fsname);
        let dir_abspath = space_abspath.join(&target_path);

        if !dir_abspath.exists() {
            create_collection(&location_relpath, segment, space_abspath)?;
        }

        location_relpath = target_path;
    }

    Ok((location_relpath, last_segment.clone()))
}

/// Creates a new collection directory in the specified parent path
///
/// Creates a new directory for the collection and saves the collection name mapping
///
/// - `location_relpath`: Relative path of location where collection needs to be created
/// - `col_segment`: Sanitized segment containing the collection name and filesystem name
/// - `space_abspath`: Absolute path to the space directory
///
/// Returns a `Result<CreateNewCollection>` containing the created collection's paths
pub fn create_collection(
    location_relpath: &Path,
    col_segment: &SanitizedSegment,
    space_abspath: &Path,
) -> Result<CreateNewCollection> {
    if col_segment.fsname.trim().is_empty() {
        return Err(Error::InvalidName(
            "Cannot create a collection without name".to_string(),
        ));
    }

    let col_abspath = space_abspath
        .join(location_relpath)
        .join(&col_segment.fsname);
    let col_relpath = location_relpath.join(&col_segment.fsname);

    fs::create_dir(&col_abspath)?;

    let mut scmt_store = SpaceCollectionsMetadataStore::get(space_abspath)?;
    scmt_store.update(|metadata| {
        let mapping_exists = metadata.mappings.contains_key(&col_relpath);
        if !mapping_exists {
            metadata
                .mappings
                .insert(col_relpath.clone(), col_segment.name.clone());
        }
    })?;

    let create_new_collection = CreateNewCollection {
        location_relpath: location_relpath.to_path_buf(),
        relpath: col_relpath,
    };

    Ok(create_new_collection)
}
