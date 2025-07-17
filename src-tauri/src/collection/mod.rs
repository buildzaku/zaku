use std::{
    cell::RefCell,
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
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
            for entry in entries.flatten() {
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

/// Creates a collection directory (nested if needed) based on `relpath`
/// under the specified `parent_relpath`. Each segment is sanitized for
/// the filesystem and the original segment is saved as collection name
///
/// Example, if `relpath` is `"Settings/Notifications"`, it creates:
/// - Directories: `settings/notifications`
/// - Collection names saved:
///   - `settings` -> `"Settings"`
///   - `notifications` -> `"Notifications"`
///
/// Directories are created under `space_abspath/parent_relpath`
///
/// - `space_abspath`: Absolute path of space
/// - `create_collection_dto`: Contains `parent_relpath` and `relpath`
///
/// Returns a `Result`  containing the created collection's relative path
pub fn create_collections_all(
    space_abspath: &Path,
    create_collection_dto: &CreateCollectionDto,
) -> Result<String> {
    if create_collection_dto.relpath.trim().is_empty() {
        return Err(Error::FileNotFound("Collection name is missing".into()));
    }

    let mut dirs = Vec::new();
    for colname in create_collection_dto.relpath.split('/') {
        let colname = colname.trim();
        let dir_sanitized_name = utils::sanitize_path_segment(colname);

        if colname.is_empty() || dir_sanitized_name.is_empty() {
            continue;
        }

        dirs.push((dir_sanitized_name, colname.to_string()));
    }

    let collection_parent_abspath = space_abspath.join(&create_collection_dto.parent_relpath);
    let mut collections_relpath = String::new();

    for (dir_sanitized_name, colname) in &dirs {
        let mut cur_collection_relpath = collections_relpath.clone();

        if !cur_collection_relpath.is_empty() {
            cur_collection_relpath.push('/');
        }
        cur_collection_relpath.push_str(dir_sanitized_name);

        let target_dir = collection_parent_abspath.join(&cur_collection_relpath);
        let dir_exists = fs::metadata(&target_dir).is_ok();
        if !dir_exists {
            fs::create_dir(&target_dir)?;
        };

        let cur_collection_relpath = utils::join_str_paths(vec![
            &create_collection_dto.parent_relpath,
            &cur_collection_relpath,
        ]);

        save_colname_if_missing(space_abspath, &cur_collection_relpath, colname)
            .map_err(|e| Error::FileReadError(format!("{cur_collection_relpath}: {e}")))?;

        if !collections_relpath.is_empty() {
            collections_relpath.push('/');
        }
        collections_relpath.push_str(dir_sanitized_name);
    }

    Ok(collections_relpath)
}

/// Creates new collection directory/directories under the space
///
/// If the collection path contains nested segments (e.g. `"Settings/Notifications"`),
/// it creates all parent directories as needed and stores each segment's original name.
///
/// - `dto`: Contains `parent_relpath` and `relpath` of the new collection from space root
/// - `sharedstate`: Shared state of the app
///
/// Returns a `Result` containing the newly created collection's metadata
pub fn create_collection(
    dto: &CreateCollectionDto,
    sharedstate: &mut SharedState,
) -> Result<CreateNewCollection> {
    if dto.relpath.trim().is_empty() {
        return Err(Error::FileNotFound(
            "Cannot create a collection without name".to_string(),
        ));
    }

    let space = sharedstate
        .space
        .clone()
        .ok_or_else(|| Error::FileNotFound("Active space not found".to_string()))?;

    let space_abspath = PathBuf::from(&space.abspath);

    let (parsed_parent_relpath, colname) = match dto.relpath.rfind('/') {
        Some(last_slash_index) => {
            let parsed_parent_relpath = &dto.relpath[..last_slash_index];
            let colname = &dto.relpath[last_slash_index + 1..];

            (Some(parsed_parent_relpath.to_string()), colname.to_string())
        }
        None => (None, dto.relpath.clone()),
    };

    let colname = colname.trim();
    let dir_sanitized_name = colname
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join("-");

    let (dir_parent_relpath, dir_sanitized_name) = match parsed_parent_relpath {
        Some(ref parsed_parent_relpath) => {
            let dto = CreateCollectionDto {
                parent_relpath: dto.parent_relpath.clone(),
                relpath: parsed_parent_relpath.to_string(),
            };

            let dirs_sanitized_relpath = create_collections_all(&space_abspath, &dto)?;

            let dir_parent_relpath = utils::join_str_paths(vec![
                dto.parent_relpath.as_str(),
                dirs_sanitized_relpath.as_str(),
            ]);

            (dir_parent_relpath, dir_sanitized_name)
        }
        None => (dto.parent_relpath.clone(), dir_sanitized_name),
    };

    let dir_abspath = space_abspath
        .join(&dir_parent_relpath)
        .join(&dir_sanitized_name);
    let dir_relpath = utils::join_str_paths(vec![
        dir_parent_relpath.as_str(),
        dir_sanitized_name.as_str(),
    ]);

    fs::create_dir(&dir_abspath)?;

    save_colname_if_missing(&space_abspath, &dir_relpath, colname)?;

    let create_new_collection = CreateNewCollection {
        parent_relpath: dir_parent_relpath,
        relpath: dir_relpath,
    };

    sharedstate.space = Some(space::parse_space(&space_abspath)?);

    Ok(create_new_collection)
}
