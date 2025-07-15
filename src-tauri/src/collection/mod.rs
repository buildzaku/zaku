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
    request, space,
    state::SharedState,
    utils,
};

pub fn parse_cols(parent_relpath: &str, col_abspath: &str) -> Result<Vec<Collection>> {
    let col_abspath = Path::new(col_abspath);
    let colname = colname_by_relpath(col_abspath).unwrap_or_else(|_| ColName {
        mappings: HashMap::new(),
    });

    let mut root_collections = Vec::new();
    let mut stack: Vec<(PathBuf, Rc<RefCell<CollectionRcRefCell>>)> = Vec::new();

    for entry in fs::read_dir(col_abspath)?.flatten() {
        let entry_path = entry.path();
        if !entry_path.is_dir() || entry.file_name() == ".zaku" {
            continue;
        }

        let dir_name = entry_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let relpath = Path::new(parent_relpath)
            .join(&dir_name)
            .to_string_lossy()
            .into_owned();

        let col_ref = Rc::new(RefCell::new(CollectionRcRefCell {
            meta: CollectionMeta {
                dir_name: dir_name.clone(),
                name: colname.mappings.get(&relpath).cloned(),
                is_expanded: true,
            },
            requests: Vec::new(),
            collections: Vec::new(),
        }));

        stack.push((PathBuf::from(&relpath), Rc::clone(&col_ref)));
        root_collections.push(Rc::clone(&col_ref));
    }

    while let Some((relpath, col_ref)) = stack.pop() {
        let abspath = col_abspath.join(&relpath);
        let reqs = request::parse_reqs(&abspath.to_string_lossy())?;
        col_ref.borrow_mut().requests = reqs;

        if let Ok(entries) = fs::read_dir(&abspath) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() || entry.file_name() == ".zaku" {
                    continue;
                }

                let dir_name = path.file_name().unwrap().to_string_lossy().into_owned();
                let sub_relpath = Path::new(&relpath)
                    .join(&dir_name)
                    .to_string_lossy()
                    .into_owned();

                let sub_ref = Rc::new(RefCell::new(CollectionRcRefCell {
                    meta: CollectionMeta {
                        dir_name: dir_name.clone(),
                        name: colname.mappings.get(&sub_relpath).cloned(),
                        is_expanded: true,
                    },
                    requests: Vec::new(),
                    collections: Vec::new(),
                }));

                stack.push((PathBuf::from(&sub_relpath), Rc::clone(&sub_ref)));
                col_ref.borrow_mut().collections.push(sub_ref);
            }
        }
    }

    let mut result = Vec::new();
    let mut build_stack: Vec<(Collection, IntoIter<Rc<RefCell<CollectionRcRefCell>>>)> = Vec::new();

    for col_ref in root_collections {
        let root = col_ref.borrow();
        let base = Collection {
            meta: root.meta.clone(),
            requests: root.requests.clone(),
            collections: Vec::new(),
        };
        let children = root.collections.clone().into_iter();
        build_stack.push((base, children));

        while let Some((cur, mut children)) = build_stack.pop() {
            if let Some(child_ref) = children.next() {
                build_stack.push((cur, children));
                let child = child_ref.borrow();
                let c = Collection {
                    meta: child.meta.clone(),
                    requests: child.requests.clone(),
                    collections: Vec::new(),
                };
                build_stack.push((c, child.collections.clone().into_iter()));
            } else if let Some((mut parent, parent_iter)) = build_stack.pop() {
                parent.collections.push(cur);
                build_stack.push((parent, parent_iter));
            } else {
                result.push(cur);
            }
        }
    }

    Ok(result)
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

/// Creates new collection directory/directories under the active space
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

    let active_space = sharedstate
        .active_space
        .clone()
        .ok_or_else(|| Error::FileNotFound("Active space not found".to_string()))?;

    let active_space_abspath = PathBuf::from(&active_space.abspath);

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

            let dirs_sanitized_relpath = create_collections_all(&active_space_abspath, &dto)?;

            let dir_parent_relpath = utils::join_str_paths(vec![
                dto.parent_relpath.as_str(),
                dirs_sanitized_relpath.as_str(),
            ]);

            (dir_parent_relpath, dir_sanitized_name)
        }
        None => (dto.parent_relpath.clone(), dir_sanitized_name),
    };

    let dir_abspath = active_space_abspath
        .join(&dir_parent_relpath)
        .join(&dir_sanitized_name);
    let dir_relpath = utils::join_str_paths(vec![
        dir_parent_relpath.as_str(),
        dir_sanitized_name.as_str(),
    ]);

    fs::create_dir(&dir_abspath)?;

    save_colname_if_missing(&active_space_abspath, &dir_relpath, colname)?;

    let create_new_collection = CreateNewCollection {
        parent_relpath: dir_parent_relpath,
        relpath: dir_relpath,
    };

    sharedstate.active_space = Some(space::parse_space(&active_space_abspath)?);

    Ok(create_new_collection)
}
