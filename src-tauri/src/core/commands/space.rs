use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use tauri::{AppHandle, State, Wry};
use tauri_plugin_store::StoreCollection;

use crate::core::{space, store};
use crate::types::{CreateSpaceDto, SpaceConfig, SpaceMeta, SpaceReference, ZakuError, ZakuState};

#[tauri::command]
pub fn create_space(
    create_space_dto: CreateSpaceDto,
    app_handle: AppHandle,
    stores: State<'_, StoreCollection<Wry>>,
    state: State<Mutex<ZakuState>>,
) -> Result<SpaceReference, ZakuError> {
    let location = PathBuf::from(create_space_dto.location.as_str());
    if !location.exists() {
        return Err(ZakuError {
            error: format!("Location does not exist: `{}`", create_space_dto.location),
        });
    }

    let space_root_path = location.join(create_space_dto.name.clone());
    let mut space_references = store::get_space_references(app_handle.clone(), stores.clone());
    let mut zaku_state = state.lock().unwrap();

    if space_references
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
                "Directory with the same name exists at `{}`",
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

    store::set_active_space(space_reference.clone(), app_handle.clone(), stores.clone());
    space_references.push(space_reference.clone());
    store::set_space_references(space_references.clone(), app_handle, stores);

    match space::parse_space(&PathBuf::from(space_reference.clone().path)) {
        Ok(active_space) => {
            zaku_state.active_space = Some(active_space);
            zaku_state.space_references = space_references;
        }
        Err(_) => {
            // TODO - handle
        }
    }

    return Ok(space_reference);
}

#[tauri::command]
pub fn set_active_space(
    space_reference: SpaceReference,
    app_handle: AppHandle,
    stores: State<'_, StoreCollection<Wry>>,
    state: State<Mutex<ZakuState>>,
) -> Result<(), ZakuError> {
    let mut zaku_state = state.lock().unwrap();
    let space_root_path = PathBuf::from(space_reference.path.as_str());

    if !space_root_path.exists() {
        return Err(ZakuError {
            error: format!("Directory does not exist at {}", space_root_path.display()),
        });
    }

    match space::parse_space(&space_root_path) {
        Ok(space) => {
            store::set_active_space(space_reference.clone(), app_handle.clone(), stores.clone());
            store::update_space_references_if_needed(
                space_reference.clone(),
                app_handle.clone(),
                stores.clone(),
            );

            zaku_state.active_space = Some(space);
            zaku_state.space_references =
                store::get_space_references(app_handle.clone(), stores.clone());

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
    state: State<Mutex<ZakuState>>,
) -> () {
    let mut zaku_state = state.lock().unwrap();
    store::delete_space_reference(space_reference, app_handle.clone(), stores.clone());

    let active_space = store::get_active_space(app_handle.clone(), stores.clone());

    if let None = active_space {
        zaku_state.active_space = None;

        match space::find_first_valid_space_reference(app_handle.clone(), stores.clone()) {
            Some(valid_space_reference) => {
                store::set_active_space(
                    valid_space_reference.clone(),
                    app_handle.clone(),
                    stores.clone(),
                );

                match space::parse_space(&PathBuf::from(valid_space_reference.clone().path)) {
                    Ok(active_space) => {
                        zaku_state.active_space = Some(active_space);
                    }
                    Err(_) => {}
                }
            }
            None => {}
        }
    }

    zaku_state.space_references = store::get_space_references(app_handle, stores);

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
