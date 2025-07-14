use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::vec::IntoIter;

use crate::space::models::{CreateSpaceDto, SpaceMeta};
use crate::state::SharedState;
use crate::{
    collection,
    collection::models::{Collection, CollectionMeta},
    error::Error,
    error::Result,
    request,
    request::models::HttpReq,
    space::models::{Space, SpaceConfigFile, SpaceCookie, SpaceReference},
    store,
    store::models::{SpaceCookies, SpaceSettings},
    store::spaces::buffer::SpaceBuf,
};

pub mod models;

#[derive(Clone, Debug)]
pub struct CollectionRcRefCell {
    pub meta: CollectionMeta,
    pub requests: Vec<HttpReq>,
    pub collections: Vec<Rc<RefCell<CollectionRcRefCell>>>,
}

pub fn create_space(dto: CreateSpaceDto, sharedstate: &mut SharedState) -> Result<SpaceReference> {
    let location = PathBuf::from(dto.location.as_str());
    if !location.exists() {
        return Err(Error::FileNotFound(format!(
            "Location does not exist: {}",
            dto.location
        )));
    }

    let space_abspath = location.join(dto.name.clone());
    let mut spacerefs = store::get_spacerefs();

    if spacerefs
        .iter()
        .any(|sr| sr.path == space_abspath.to_string_lossy())
    {
        return Err(Error::FileNotFound(format!(
            "Space already exists in saved spaces: {}",
            space_abspath.to_string_lossy()
        )));
    }
    if space_abspath.exists() {
        return Err(Error::FileNotFound(format!(
            "Directory with this name already exists: {}",
            space_abspath.to_string_lossy()
        )));
    }

    fs::create_dir(&space_abspath)?;
    let config_dir = space_abspath.join(".zaku");
    fs::create_dir(&config_dir)?;

    let mut config_file = File::create(config_dir.join("config.toml"))?;
    let config = SpaceConfigFile {
        meta: SpaceMeta {
            name: dto.name.clone(),
        },
    };

    config_file.write_all(toml::to_string_pretty(&config)?.as_bytes())?;

    let spaceref = SpaceReference {
        path: space_abspath.to_string_lossy().to_string(),
        name: dto.name,
    };

    store::set_active_spaceref(spaceref.clone())?;
    spacerefs.push(spaceref.clone());
    store::set_spacerefs(spacerefs.clone())?;

    if let Ok(active_space) = parse_space(&PathBuf::from(&spaceref.path)) {
        sharedstate.active_space = Some(active_space);
        sharedstate.spacerefs = spacerefs;
    }

    Ok(spaceref)
}

fn parse_root_collection(space_abspath: &Path) -> Result<Collection> {
    let space_dirname = space_abspath
        .file_name()
        .unwrap_or(space_abspath.as_os_str())
        .to_string_lossy()
        .into_owned();
    let relative_space_root = "".to_string();
    let collection_name_by_relpath =
        collection::displayname_by_relpath(space_abspath).unwrap_or_else(|_| HashMap::new());
    let active_space_buffer = SpaceBuf::load(space_abspath)?;
    let active_spacebuf_rlock = active_space_buffer
        .read()
        .map_err(|_| Error::LockError("Failed to acquire read lock".into()))?;
    let space_config = parse_spacecfg(space_abspath).ok();

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
                    if entry_abspath.extension().and_then(|e| e.to_str()) != Some("toml") {
                        continue;
                    }

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
                            Err(_) => {
                                eprintln!("Invalid request TOML: '{}'", entry_abspath.display());
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

pub fn parse_space(space_abspath: &Path) -> Result<Space> {
    let root_collection = parse_root_collection(space_abspath)?;
    let space_config_file = parse_spacecfg(space_abspath)?;
    let cookie_store = SpaceCookies::load(space_abspath.to_string_lossy().as_ref())?;
    let store = cookie_store.lock().unwrap();
    let cookies: Vec<SpaceCookie> = store
        .iter_any()
        .map(SpaceCookie::from_cookie_store)
        .collect();
    let cookies_by_domain: HashMap<String, Vec<SpaceCookie>> =
        cookies.into_iter().fold(HashMap::new(), |mut acc, ck| {
            acc.entry(ck.domain.clone()).or_default().push(ck);
            acc
        });

    let settings = SpaceSettings::load(&space_abspath.to_string_lossy())?;

    Ok(Space {
        abspath: space_abspath.to_string_lossy().into_owned(),
        meta: space_config_file.meta,
        root: root_collection,
        cookies: cookies_by_domain,
        settings,
    })
}

pub fn parse_spacecfg(space_abspath: &Path) -> Result<SpaceConfigFile> {
    let path = space_abspath.join(".zaku/config.toml");
    let content =
        fs::read_to_string(&path).map_err(|_| Error::FileNotFound(path.display().to_string()))?;
    let config = toml::from_str(&content)
        .map_err(|e| Error::FileReadError(format!("{}: {}", path.display(), e)))?;

    Ok(config)
}

pub fn first_valid_spaceref() -> Option<SpaceReference> {
    store::get_spacerefs()
        .into_iter()
        .find_map(|space_reference| {
            let space_abspath = PathBuf::from(&space_reference.path);

            match parse_spacecfg(&space_abspath) {
                Ok(_) => Some(space_reference),
                Err(_) => None,
            }
        })
}
