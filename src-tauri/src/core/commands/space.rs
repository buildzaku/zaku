use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State, Wry};
use tauri_plugin_store::StoreCollection;

use crate::constants::ZakuStoreKey;
use crate::core::{space, store};
use crate::types::{
    AppState, CreateSpaceDto, Space, SpaceConfig, SpaceMeta, SpaceReference, ZakuError,
};

#[tauri::command]
pub fn create_space(
    create_space_dto: CreateSpaceDto,
    app_handle: AppHandle,
    stores: State<'_, StoreCollection<Wry>>,
    state: State<Mutex<AppState>>,
) -> Result<SpaceReference, ZakuError> {
    let location = PathBuf::from(create_space_dto.location.as_str());
    if !location.exists() {
        return Err(ZakuError {
            error: format!("Location does not exist: `{}`", create_space_dto.location),
        });
    }

    let space_root_path = location.join(create_space_dto.name.clone());
    let mut app_state = state.lock().unwrap();
    if app_state
        .saved_spaces
        .iter()
        .any(|space_reference| space_reference.path == space_root_path.to_string_lossy())
    {
        return Err(ZakuError {
            error: format!(
                "Space already exists in saved spaces store with path `{}`",
                space_root_path.display()
            ),
        });
    }
    if space_root_path.exists() {
        return Err(ZakuError {
            error: format!(
                "Directory already exists at `{}`",
                space_root_path.display()
            ),
        });
    }

    fs::create_dir(&space_root_path).expect("Failed to create space directory");

    let space_meta_path = space_root_path.join(".zaku");
    fs::create_dir(&space_meta_path).expect("Failed to create `.zaku` directory");

    let mut space_config_file =
        File::create(&space_meta_path.join("config.toml")).expect("Failed to create `config.toml`");

    let space_config = SpaceConfig {
        meta: SpaceMeta {
            name: create_space_dto.name.clone(),
        },
    };

    space_config_file
        .write_all(
            toml::to_string_pretty(&space_config)
                .expect("Failed to serialize space config")
                .as_bytes(),
        )
        .expect("Failed to write to config file");

    let space_reference = SpaceReference {
        path: space_root_path.to_string_lossy().to_string(),
        name: create_space_dto.name,
    };

    match space::parse_space(&space_root_path) {
        Ok(active_space) => {
            let app_data_dir = app_handle.path().app_data_dir().unwrap();

            tauri_plugin_store::with_store(
                app_handle.clone(),
                stores.clone(),
                app_data_dir.clone(),
                |store| {
                    store
                        .insert(
                            ZakuStoreKey::ActiveSpace.to_string(),
                            serde_json::json!({
                                "path": space_reference.path,
                                "name": space_reference.name,
                            }),
                        )
                        .map_err(|err| err.to_string())
                        .unwrap();

                    app_state.active_space = Some(active_space);

                    app_state.saved_spaces.push(space_reference.clone());

                    store
                        .insert(
                            ZakuStoreKey::SavedSpaces.to_string(),
                            serde_json::json!(app_state.saved_spaces),
                        )
                        .map_err(|err| err.to_string())
                        .unwrap();

                    store.save().unwrap();

                    return Ok(());
                },
            )
            .unwrap();

            return Ok(space_reference);
        }
        Err(err) => {
            return Err(ZakuError {
                error: format!(
                    "Failed to parse the space {}: {}",
                    space_root_path.display(),
                    err
                ),
            });
        }
    }
}

#[tauri::command]
pub fn get_active_space(state: State<Mutex<AppState>>) -> Option<Space> {
    let state = state.lock().unwrap();

    return state.active_space.clone();
}

#[tauri::command]
pub fn set_active_space(
    space_reference: SpaceReference,
    app_handle: AppHandle,
    stores: State<'_, StoreCollection<Wry>>,
    state: State<Mutex<AppState>>,
) -> Result<(), ZakuError> {
    let space_root_path = PathBuf::from(space_reference.path.as_str());

    if !space_root_path.exists() {
        return Err(ZakuError {
            error: format!("Directory does not exist at {}", space_root_path.display()),
        });
    }

    match space::parse_space(&space_root_path) {
        Ok(active_space) => {
            let app_data_dir = app_handle.path().app_data_dir().unwrap();

            tauri_plugin_store::with_store(
                app_handle.clone(),   // TODO - REMOVE .clone()
                stores.clone(),       // TODO - REMOVE .clone()
                app_data_dir.clone(), // TODO - REMOVE .clone()
                |store| {
                    store
                        .insert(
                            ZakuStoreKey::ActiveSpace.to_string(),
                            serde_json::json!(space_root_path.to_str()),
                        )
                        .map_err(|err| err.to_string())
                        .unwrap();
                    store.save().unwrap();

                    let mut saved_spaces = store::get_saved_spaces(store);

                    if !saved_spaces.iter().any(|space_reference| {
                        space_reference.path == space_root_path.to_str().unwrap()
                    }) {
                        println!("not inside store {:?}", space_reference);

                        saved_spaces.push(SpaceReference {
                            path: space_root_path.to_str().unwrap().to_string(),
                            name: space_reference.name.clone(),
                        });

                        store
                            .insert(
                                ZakuStoreKey::SavedSpaces.to_string(),
                                serde_json::json!(saved_spaces),
                            )
                            .map_err(|err| err.to_string())
                            .unwrap();
                        store.save().unwrap();
                    }

                    println!("saveddd spaces after {:?}", saved_spaces);

                    return Ok(());
                },
            )
            .unwrap();

            let mut app_state = state.lock().unwrap();
            app_state.active_space = Some(active_space);

            // TODO - REMOVE
            tauri_plugin_store::with_store(
                app_handle.clone(),
                stores.clone(),
                app_data_dir.clone(),
                |store| {
                    println!(
                        "another check to see saved spaces {:?}",
                        store::get_saved_spaces(store)
                    );

                    Ok(())
                },
            )
            .unwrap();

            return Ok(());
        }
        Err(err) => Err(ZakuError {
            error: format!("Unable to parse space: {}", err),
        }),
    }
}

#[tauri::command]
pub fn delete_space(
    space_reference: SpaceReference,
    app_handle: AppHandle,
    stores: State<'_, StoreCollection<Wry>>,
    state: State<Mutex<AppState>>,
) -> () {
    let app_data_dir = app_handle.path().app_data_dir().unwrap();
    let mut state = state.lock().unwrap();

    state.saved_spaces = state
        .saved_spaces
        .iter()
        .filter(|space| space.path != space_reference.path)
        .cloned()
        .collect();

    if let Some(active_space) = &state.active_space {
        if active_space.path == space_reference.path {
            state.active_space = None;
        }
    }

    tauri_plugin_store::with_store(app_handle, stores, app_data_dir, |store| {
        store
            .delete(ZakuStoreKey::ActiveSpace.to_string())
            .map_err(|err| err.to_string())
            .unwrap();

        store
            .insert(
                ZakuStoreKey::SavedSpaces.to_string(),
                serde_json::json!(state.saved_spaces),
            )
            .map_err(|err| err.to_string())
            .unwrap();

        store.save().unwrap();

        return Ok(());
    })
    .unwrap();

    // *state.lock().unwrap() = AppState {
    //     active_space: None,
    //     saved_spaces,
    // };

    return ();
}

#[tauri::command]
pub fn get_space_reference(path: String) -> Result<SpaceReference, ZakuError> {
    let space_root_path = PathBuf::from(path.as_str());

    match space::parse_space_config(&space_root_path) {
        Ok(space_config) => {
            let space_reference = SpaceReference {
                path: space_root_path.to_string_lossy().to_string(),
                name: space_config.meta.name,
            };

            return Ok(space_reference);
        }
        Err(err) => {
            return Err(ZakuError {
                error: format!(
                    "Failed to parse the space {}: {}",
                    space_root_path.display(),
                    err
                ),
            });
        }
    }
}

#[tauri::command]
pub fn get_saved_spaces(state: State<Mutex<AppState>>) -> Vec<SpaceReference> {
    let state = state.lock().unwrap();

    return state.saved_spaces.clone();
}
