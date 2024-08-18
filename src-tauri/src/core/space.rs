use std::collections::VecDeque;
use std::fs::{self, File};
use std::io::{Error, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State, Wry};
use tauri_plugin_store::StoreCollection;

use crate::constants::ZakuStoreKey;
use crate::types::{
    AppState, Collection, CreateSpaceDto, CreateSpaceResult, Request, Space, SpaceConfig,
    SpaceMeta, ZakuError,
};

#[tauri::command]
pub fn get_active_space(state: State<Mutex<AppState>>) -> Option<Space> {
    println!("getting active space");
    let state = state.lock().unwrap();

    return state.active_space.clone();
}

#[tauri::command]
pub fn set_active_space(
    path: String,
    app_handle: AppHandle,
    stores: State<'_, StoreCollection<Wry>>,
    state: State<Mutex<AppState>>,
) {
    println!("setting active space");

    let path = PathBuf::from(path.as_str());

    match path.exists() {
        true => match parse_space(&path) {
            Ok(active_space) => {
                let app_data_dir = app_handle.path().app_data_dir().unwrap();

                tauri_plugin_store::with_store(app_handle, stores, app_data_dir, |store| {
                    store
                        .insert(
                            ZakuStoreKey::ActiveSpacePath.to_string(),
                            serde_json::json!(path.to_str()),
                        )
                        .map_err(|e| e.to_string())
                        .unwrap();

                    store.save().unwrap();

                    let saved_path = store
                        .get(ZakuStoreKey::ActiveSpacePath.to_string())
                        .unwrap();

                    println!("Retrieved path: {}", saved_path);

                    return Ok(());
                })
                .unwrap();

                *state.lock().unwrap() = AppState {
                    active_space: Some(active_space),
                };
            }
            Err(err) => {
                eprintln!("Unable to set app state, {}", err);
            }
        },
        false => {
            eprintln!("Path does not exist.");
        }
    }
}

#[tauri::command]
pub fn create_space(
    create_space_dto: CreateSpaceDto,
    app_handle: AppHandle,
    stores: State<'_, StoreCollection<Wry>>,
    state: State<Mutex<AppState>>,
) -> Result<CreateSpaceResult, ZakuError> {
    println!("Creating new space");

    let location = PathBuf::from(create_space_dto.path.as_str());

    match location.exists() {
        true => {
            let space_path = location.join(create_space_dto.name.clone());

            if space_path.exists() {
                return Err(ZakuError {
                    error: format!("Directory already exists at {}", space_path.display()),
                });
            }

            fs::create_dir(&space_path).expect("Failed to create space directory");

            let space_meta_path = space_path.join(".zaku");
            fs::create_dir(&space_meta_path).expect("Failed to create .zaku directory");

            let mut space_config_file = File::create(&space_meta_path.join("config.toml"))
                .expect("Failed to create config.toml");

            let space_config = SpaceConfig {
                meta: SpaceMeta {
                    name: create_space_dto.name,
                },
            };

            space_config_file
                .write_all(
                    toml::to_string_pretty(&space_config)
                        .expect("Failed to serialize space config")
                        .as_bytes(),
                )
                .expect("Failed to write to config file");

            match parse_space(&space_path) {
                Ok(active_space) => {
                    let app_data_dir = app_handle.path().app_data_dir().unwrap();

                    tauri_plugin_store::with_store(app_handle, stores, app_data_dir, |store| {
                        store
                            .insert(
                                ZakuStoreKey::ActiveSpacePath.to_string(),
                                serde_json::json!(space_path.to_str()),
                            )
                            .map_err(|e| e.to_string())
                            .unwrap();

                        store.save().unwrap();

                        let saved_path = store
                            .get(ZakuStoreKey::ActiveSpacePath.to_string())
                            .unwrap();

                        println!("Retrieved path: {}", saved_path);

                        return Ok(());
                    })
                    .unwrap();

                    *state.lock().unwrap() = AppState {
                        active_space: Some(active_space),
                    };

                    return Ok(CreateSpaceResult {
                        path: space_path
                            .to_str()
                            .expect("Failed to convert space path to string")
                            .to_string(),
                    });
                }
                Err(err) => {
                    return Err(ZakuError {
                        error: format!(
                            "Failed to parse the space {}: {}",
                            space_path.display(),
                            err
                        ),
                    });
                }
            }
        }
        false => {
            return Err(ZakuError {
                error: format!("Path does not exist: {}", create_space_dto.path),
            });
        }
    }
}

#[tauri::command]
pub fn delete_active_space(
    app_handle: AppHandle,
    stores: State<'_, StoreCollection<Wry>>,
    state: State<Mutex<AppState>>,
) {
    println!("deleting active space");
    let app_data_dir = app_handle.path().app_data_dir().unwrap();

    tauri_plugin_store::with_store(app_handle, stores, app_data_dir, |store| {
        store
            .delete(ZakuStoreKey::ActiveSpacePath.to_string())
            .map_err(|e| e.to_string())
            .unwrap();

        store.save().unwrap();

        return Ok(());
    })
    .unwrap();

    *state.lock().unwrap() = AppState {
        active_space: None,
    };
}

pub fn parse_space(path: &Path) -> Result<Space, Error> {
    // Initialize a vector to store collections
    let mut collections: Vec<Collection> = Vec::new();
    // Initialize a vector to store requests found at the root level
    let mut requests: Vec<Request> = Vec::new();

    // Create the full path to the config.toml file in the space
    let config_path = path.join(".zaku/config.toml");

    // Reading the config file with error context
    let space_config_content = fs::read_to_string(&config_path).map_err(|e| {
        Error::new(
            ErrorKind::NotFound,
            format!("Failed to load {}: {}", config_path.display(), e),
        )
    })?;

    // Parsing space config with error context
    let space_config: SpaceConfig =
        toml::from_str(&space_config_content).map_err(|err| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse {}: {}", config_path.display(), err),
            )
        })?;

    // Stack to keep track of directories to process, starting with the root space directory
    let mut directories_to_process: VecDeque<PathBuf> = VecDeque::new();
    directories_to_process.push_back(path.to_path_buf());

    // Loop until all directories have been processed
    while let Some(current_directory) = directories_to_process.pop_front() {
        // Iterate over the entries in the current directory
        for directory_entry in fs::read_dir(&current_directory).map_err(|err| {
            Error::new(
                ErrorKind::Other,
                format!(
                    "Failed to read directory {}: {}",
                    current_directory.display(),
                    err
                ),
            )
        })? {
            let directory_entry = directory_entry.map_err(|e| {
                Error::new(
                    ErrorKind::Other,
                    format!(
                        "Failed to access entry in {}: {}",
                        current_directory.display(),
                        e
                    ),
                )
            })?;
            let entry_path = directory_entry.path(); // Get the full path of the entry
            let entry_name = entry_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(); // Get the entry's name as a string

            if entry_path.is_file() {
                // If the entry is a file and it ends with ".toml"
                if entry_name.ends_with(".toml") {
                    if current_directory == path {
                        // If the file is at the root level, add it to the requests vector
                        requests.push(Request { name: entry_name });
                    } else {
                        // If the file is inside a collection folder, add it to the appropriate collection
                        let parent_folder_name = current_directory
                            .file_name()
                            .unwrap()
                            .to_string_lossy()
                            .into_owned();
                        if let Some(collection) = collections
                            .iter_mut()
                            .find(|c| c.name == parent_folder_name)
                        {
                            collection.requests.push(Request { name: entry_name });
                        }
                    }
                }
            } else if entry_path.is_dir() {
                // If the entry is a directory
                if entry_name == ".zaku" && current_directory != path {
                    // Skip nested .zaku directories (only the root .zaku is allowed)
                    continue;
                } else if current_directory == path && entry_name != ".zaku" {
                    // If the directory is not .zaku and is at the root level, treat it as a collection
                    collections.push(Collection {
                        name: entry_name.clone(), // Name of the collection (folder name)
                        requests: Vec::new(),     // Initialize with an empty vector of requests
                    });
                    // Add this directory to the stack to process its contents
                    directories_to_process.push_back(entry_path);
                }
            }
        }
    }

    return Ok(Space {
        path: path.to_string_lossy().into_owned(),
        config: space_config,
        collections,
        requests,
    });
}
