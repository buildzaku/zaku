use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::vec::IntoIter;

use crate::models::buffer::SpaceBuf;
use crate::models::collection::{Collection, CollectionMeta};
use crate::models::request::HttpReq;
use crate::models::space::{Space, SpaceConfigFile, SpaceReference};

use super::{collection, request, store};

#[derive(Clone, Debug)]
pub struct CollectionRcRefCell {
    pub meta: CollectionMeta,
    pub requests: Vec<HttpReq>,
    pub collections: Vec<Rc<RefCell<CollectionRcRefCell>>>,
}

fn parse_root_collection(absolute_space_root: &Path) -> Result<Collection, Error> {
    let space_dir_name = absolute_space_root
        .file_name()
        .unwrap_or_else(|| absolute_space_root.as_os_str())
        .to_string_lossy()
        .into_owned();
    let relative_space_root = "".to_string();
    let collection_name_by_relative_path =
        collection::display_name_by_relative_path(absolute_space_root)
            .unwrap_or_else(|_| HashMap::new());
    let active_space_buffer = SpaceBuf::load(absolute_space_root);
    let active_space_buffer_rlock = SpaceBuf::acquire_read_lock(&active_space_buffer);
    let space_config = match parse_space_config(&absolute_space_root) {
        Ok(space_config) => Some(space_config),
        Err(_) => None,
    };

    let root_collection_ref_cell = Rc::new(RefCell::new(CollectionRcRefCell {
        meta: CollectionMeta {
            dir_name: space_dir_name,
            display_name: space_config.map(|config| config.meta.name),
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
        if let Ok(entries) = fs::read_dir(absolute_space_root.join(&path)) {
            for entry in entries.flatten() {
                let is_symlink = entry
                    .file_type()
                    .map(|file_type| file_type.is_symlink())
                    .unwrap_or(false);
                if is_symlink {
                    continue;
                }

                let absolute_entry_path = entry.path();

                if absolute_entry_path.is_dir() {
                    let name = absolute_entry_path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned();
                    if name == ".zaku" {
                        continue;
                    }

                    let relative_path = absolute_entry_path
                        .strip_prefix(absolute_space_root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned();

                    let sub_collection = Rc::new(RefCell::new(CollectionRcRefCell {
                        meta: CollectionMeta {
                            dir_name: name,
                            display_name: collection_name_by_relative_path
                                .get(&relative_path)
                                .cloned(),
                            is_expanded: true,
                        },
                        requests: Vec::new(),
                        collections: Vec::new(),
                    }));

                    stack.push((PathBuf::from(&relative_path), Rc::clone(&sub_collection)));
                    collection_rc_refcell
                        .borrow_mut()
                        .collections
                        .push(sub_collection);
                } else if absolute_entry_path.is_file() {
                    let relative_path = absolute_entry_path
                        .strip_prefix(absolute_space_root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned();
                    let req_buf = active_space_buffer_rlock.requests.get(&relative_path);

                    if let Some(req_buf) = req_buf {
                        collection_rc_refcell
                            .borrow_mut()
                            .requests
                            .push(HttpReq::from_reqbuf(req_buf));
                    } else {
                        let file_name = absolute_entry_path
                            .file_name()
                            .unwrap()
                            .to_string_lossy()
                            .into_owned();

                        match request::parse_request_file(&absolute_entry_path) {
                            Ok(req_toml) => {
                                collection_rc_refcell
                                    .borrow_mut()
                                    .requests
                                    .push(HttpReq::from_reqtoml(&req_toml, file_name));
                            }
                            Err(err) => {
                                eprintln!("{}", err);
                                eprintln!("Unable to parse the request file")
                            }
                        }
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

    while let Some((current_collection, mut sub_collections_iter)) = stack.pop() {
        if let Some(sub_collection_ref_cell) = sub_collections_iter.next() {
            stack.push((current_collection, sub_collections_iter));

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
        } else {
            if let Some((mut parent_collection, parent_sub_collections_iter)) = stack.pop() {
                parent_collection.collections.push(current_collection);

                stack.push((parent_collection, parent_sub_collections_iter));
            } else {
                root_collection = Some(current_collection);
            }
        }
    }

    match root_collection {
        Some(collection) => {
            return Ok(collection);
        }
        None => {
            return Err(Error::new(
                ErrorKind::NotFound,
                "Failed to build collection, stack is empty no collection to return",
            ));
        }
    }
}

pub fn parse_space(absolute_space_root: &Path) -> Result<Space, Error> {
    match parse_root_collection(absolute_space_root) {
        Ok(root_collection) => {
            match parse_space_config(&absolute_space_root) {
                Ok(space_config_file) => {
                    return Ok(Space {
                        absolute_path: absolute_space_root.to_string_lossy().into_owned(),
                        meta: space_config_file.meta,
                        root: root_collection,
                    });
                }
                Err(err) => {
                    eprintln!("{}", err);

                    return Err(err);
                }
            };
        }
        Err(err) => {
            eprintln!("{}", err);

            return Err(err);
        }
    }
}

pub fn parse_space_config(absolute_space_root: &Path) -> Result<SpaceConfigFile, Error> {
    return fs::read_to_string(absolute_space_root.join(".zaku/config.toml"))
        .map_err(|err| {
            Error::new(
                ErrorKind::NotFound,
                format!("Failed to load {}: {}", absolute_space_root.display(), err),
            )
        })
        .and_then(|content| {
            toml::from_str(&content).map_err(|err| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to parse {}: {}", absolute_space_root.display(), err),
                )
            })
        });
}

pub fn find_first_valid_space_reference() -> Option<SpaceReference> {
    return store::get_space_references()
        .into_iter()
        .find_map(|space_reference| {
            let absolute_space_root = PathBuf::from(&space_reference.path);

            match parse_space_config(&absolute_space_root) {
                Ok(_) => Some(space_reference),
                Err(_) => None,
            }
        });
}
