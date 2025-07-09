use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::vec::IntoIter;

use crate::core::cookie::SpaceCookies;
use crate::core::store::spaces::settings::SpaceSettings;
use crate::models::buffer::SpaceBuf;
use crate::models::collection::{Collection, CollectionMeta};
use crate::models::request::HttpReq;
use crate::models::space::{Space, SpaceConfigFile, SpaceCookie, SpaceReference};

use super::{collection, request, store};

#[derive(Clone, Debug)]
pub struct CollectionRcRefCell {
    pub meta: CollectionMeta,
    pub requests: Vec<HttpReq>,
    pub collections: Vec<Rc<RefCell<CollectionRcRefCell>>>,
}

fn parse_root_collection(space_abspath: &Path) -> Result<Collection, Error> {
    let space_dirname = space_abspath
        .file_name()
        .unwrap_or_else(|| space_abspath.as_os_str())
        .to_string_lossy()
        .into_owned();
    let relative_space_root = "".to_string();
    let collection_name_by_relpath =
        collection::displayname_by_relpath(space_abspath).unwrap_or_else(|_| HashMap::new());
    let active_space_buffer = SpaceBuf::load(space_abspath);
    let active_spacebuf_rlock = SpaceBuf::acq_rlock(&active_space_buffer);
    let space_config = match parse_spacecfg(&space_abspath) {
        Ok(space_config) => Some(space_config),
        Err(_) => None,
    };

    let root_collection_ref_cell = Rc::new(RefCell::new(CollectionRcRefCell {
        meta: CollectionMeta {
            dir_name: space_dirname,
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
                            dir_name: name,
                            display_name: collection_name_by_relpath.get(&relpath).cloned(),
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
                    let relpath = entry_abspath
                        .strip_prefix(space_abspath)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned();
                    let req_buf = active_spacebuf_rlock.requests.get(&relpath);

                    if let Some(req_buf) = req_buf {
                        collection_rc_refcell
                            .borrow_mut()
                            .requests
                            .push(HttpReq::from_reqbuf(req_buf));
                    } else {
                        let file_name = entry_abspath
                            .file_name()
                            .unwrap()
                            .to_string_lossy()
                            .into_owned();

                        match request::parse_reqtoml(&entry_abspath) {
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
        } else {
            if let Some((mut parent_collection, parent_sub_collections_iter)) = stack.pop() {
                parent_collection.collections.push(cur_collection);

                stack.push((parent_collection, parent_sub_collections_iter));
            } else {
                root_collection = Some(cur_collection);
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

pub fn parse_space(space_abspath: &Path) -> Result<Space, Error> {
    match parse_root_collection(space_abspath) {
        Ok(root_collection) => match parse_spacecfg(&space_abspath) {
            Ok(space_config_file) => {
                let cookie_store = SpaceCookies::load(space_abspath.to_string_lossy().as_ref());
                let store = cookie_store.lock().unwrap();
                let cookies: Vec<SpaceCookie> = store
                    .iter_any()
                    .map(SpaceCookie::from_cookie_store)
                    .collect();
                let cookies_by_domain: HashMap<String, Vec<SpaceCookie>> =
                    cookies.into_iter().fold(
                        HashMap::new(),
                        |mut acc: HashMap<String, Vec<SpaceCookie>>, ck| {
                            acc.entry(ck.domain.clone()).or_default().push(ck);
                            acc
                        },
                    );
                let settings = SpaceSettings::load(space_abspath.to_string_lossy().as_ref());

                return Ok(Space {
                    abspath: space_abspath.to_string_lossy().into_owned(),
                    meta: space_config_file.meta,
                    root: root_collection,
                    cookies: cookies_by_domain,
                    settings,
                });
            }
            Err(err) => {
                eprintln!("{}", err);
                return Err(err);
            }
        },
        Err(err) => {
            eprintln!("{}", err);
            return Err(err);
        }
    }
}

pub fn parse_spacecfg(space_abspath: &Path) -> Result<SpaceConfigFile, Error> {
    return fs::read_to_string(space_abspath.join(".zaku/config.toml"))
        .map_err(|err| {
            Error::new(
                ErrorKind::NotFound,
                format!("Failed to load {}: {}", space_abspath.display(), err),
            )
        })
        .and_then(|content| {
            toml::from_str(&content).map_err(|err| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to parse {}: {}", space_abspath.display(), err),
                )
            })
        });
}

pub fn first_valid_spaceref() -> Option<SpaceReference> {
    return store::get_spacerefs()
        .into_iter()
        .find_map(|space_reference| {
            let space_abspath = PathBuf::from(&space_reference.path);

            match parse_spacecfg(&space_abspath) {
                Ok(_) => Some(space_reference),
                Err(_) => None,
            }
        });
}
