use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State, Wry};
use tauri_plugin_store::StoreCollection;

use crate::constants::ZakuStoreKey;
use crate::core::space;
use crate::types::{
    AppState, CreateSpaceDto, CreateSpaceResult, Space, SpaceConfig, SpaceMeta, ZakuError,
};

#[tauri::command]
pub fn create_space(
    create_space_dto: CreateSpaceDto,
    app_handle: AppHandle,
    stores: State<'_, StoreCollection<Wry>>,
    state: State<Mutex<AppState>>,
) -> Result<CreateSpaceResult, ZakuError> {
    let location = PathBuf::from(create_space_dto.path.as_str());
    if !location.exists() {
        return Err(ZakuError {
            error: format!("Path does not exist: {}", create_space_dto.path),
        });
    }

    let space_root_path = location.join(create_space_dto.name.clone());
    if space_root_path.exists() {
        return Err(ZakuError {
            error: format!("Directory already exists at {}", space_root_path.display()),
        });
    }

    fs::create_dir(&space_root_path).expect("Failed to create space directory");

    let space_meta_path = space_root_path.join(".zaku");
    fs::create_dir(&space_meta_path).expect("Failed to create `.zaku` directory");

    let mut space_config_file =
        File::create(&space_meta_path.join("config.toml")).expect("Failed to create `config.toml`");

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

    match space::parse_space(&space_root_path) {
        Ok(active_space) => {
            let app_data_dir = app_handle.path().app_data_dir().unwrap();

            tauri_plugin_store::with_store(app_handle, stores, app_data_dir, |store| {
                store
                    .insert(
                        ZakuStoreKey::ActiveSpacePath.to_string(),
                        serde_json::json!(space_root_path.to_str()),
                    )
                    .map_err(|err| err.to_string())
                    .unwrap();

                store.save().unwrap();

                return Ok(());
            })
            .unwrap();

            *state.lock().unwrap() = AppState {
                active_space: Some(active_space),
            };

            return Ok(CreateSpaceResult {
                path: space_root_path
                    .to_str()
                    .expect("Failed to convert space path to string")
                    .to_string(),
            });
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
    space_root_path: String,
    app_handle: AppHandle,
    stores: State<'_, StoreCollection<Wry>>,
    state: State<Mutex<AppState>>,
) -> Result<(), ZakuError> {
    let space_root_path = PathBuf::from(space_root_path.as_str());

    if !space_root_path.exists() {
        return Err(ZakuError {
            error: format!("Directory does not exist at {}", space_root_path.display()),
        });
    }

    match space::parse_space(&space_root_path) {
        Ok(active_space) => {
            let app_data_dir = app_handle.path().app_data_dir().unwrap();

            tauri_plugin_store::with_store(app_handle, stores, app_data_dir, |store| {
                store
                    .insert(
                        ZakuStoreKey::ActiveSpacePath.to_string(),
                        serde_json::json!(space_root_path.to_str()),
                    )
                    .map_err(|err| err.to_string())
                    .unwrap();
                store.save().unwrap();

                return Ok(());
            })
            .unwrap();

            *state.lock().unwrap() = AppState {
                active_space: Some(active_space),
            };

            return Ok(());
        }
        Err(err) => {
            return Err(ZakuError {
                error: format!("Unable to parse space: {}", err),
            });
        }
    }
}

#[tauri::command]
pub fn delete_active_space(
    app_handle: AppHandle,
    stores: State<'_, StoreCollection<Wry>>,
    state: State<Mutex<AppState>>,
) -> () {
    let app_data_dir = app_handle.path().app_data_dir().unwrap();

    tauri_plugin_store::with_store(app_handle, stores, app_data_dir, |store| {
        store
            .delete(ZakuStoreKey::ActiveSpacePath.to_string())
            .map_err(|err| err.to_string())
            .unwrap();

        store.save().unwrap();

        return Ok(());
    })
    .unwrap();

    *state.lock().unwrap() = AppState { active_space: None };

    return ();
}
