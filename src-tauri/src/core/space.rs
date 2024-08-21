use std::collections::VecDeque;
use std::ffi::OsStr;
use std::fs::{self};
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};

use tauri::{AppHandle, State, Wry};
use tauri_plugin_store::StoreCollection;

use crate::types::{Collection, Request, Space, SpaceConfig, SpaceReference};

use super::store;

pub fn parse_space(path: &Path) -> Result<Space, Error> {
    let space_root_path = path;
    let space_config = parse_space_config(&space_root_path)?;

    let mut collections: Vec<Collection> = Vec::new();
    let mut requests: Vec<Request> = Vec::new();
    let mut directories: VecDeque<PathBuf> = VecDeque::new();

    directories.push_back(space_root_path.to_path_buf());

    while let Some(current_directory) = directories.pop_front() {
        let entries = fs::read_dir(&current_directory).map_err(|err| {
            Error::new(
                ErrorKind::Other,
                format!(
                    "Failed to read directory {}: {}",
                    current_directory.display(),
                    err
                ),
            )
        })?;

        for entry in entries {
            let entry_path = entry
                .map_err(|err| {
                    Error::new(
                        ErrorKind::Other,
                        format!(
                            "Failed to access sub directory in {}: {}",
                            current_directory.display(),
                            err
                        ),
                    )
                })?
                .path();

            if entry_path.is_file() && entry_path.extension() == Some(OsStr::new("toml")) {
                if current_directory == space_root_path {
                    requests.push(Request {
                        name: entry_path
                            .file_name()
                            .unwrap()
                            .to_string_lossy()
                            .into_owned(),
                    });
                } else {
                    let parent_directory = current_directory.file_name().unwrap().to_string_lossy();
                    let target_collection = collections
                        .iter_mut()
                        .find(|collection| collection.name == parent_directory);

                    match target_collection {
                        Some(collection) => {
                            collection.requests.push(Request {
                                name: entry_path
                                    .file_name()
                                    .unwrap()
                                    .to_string_lossy()
                                    .into_owned(),
                            });
                        }
                        None => (),
                    }
                }
            } else if entry_path.is_dir() {
                let entry_name = entry_path.file_name().unwrap().to_string_lossy();
                if entry_name == ".zaku" && current_directory != space_root_path {
                    continue;
                }

                if current_directory == space_root_path && entry_name != ".zaku" {
                    collections.push(Collection {
                        name: entry_name.into_owned(),
                        requests: Vec::new(),
                    });
                }

                directories.push_back(entry_path);
            }
        }
    }

    return Ok(Space {
        path: space_root_path.to_string_lossy().into_owned(),
        config: space_config,
        collections,
        requests,
    });
}

pub fn parse_space_config(space_root_path: &Path) -> Result<SpaceConfig, Error> {
    return fs::read_to_string(space_root_path.join(".zaku/config.toml"))
        .map_err(|err| {
            Error::new(
                ErrorKind::NotFound,
                format!("Failed to load {}: {}", space_root_path.display(), err),
            )
        })
        .and_then(|content| {
            toml::from_str(&content).map_err(|err| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to parse {}: {}", space_root_path.display(), err),
                )
            })
        });
}

pub fn find_first_valid_space(
    app_handle: AppHandle,
    stores: State<'_, StoreCollection<Wry>>,
) -> Option<SpaceReference> {
    return store::get_saved_spaces(app_handle, stores)
        .into_iter()
        .find_map(|space_reference| {
            let space_root_path = PathBuf::from(&space_reference.path);

            match parse_space_config(&space_root_path) {
                Ok(_) => Some(space_reference),
                Err(_) => None,
            }
        });
}
