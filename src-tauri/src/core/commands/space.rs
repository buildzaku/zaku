use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;

use tauri::{AppHandle, State, Wry};
use tauri_plugin_store::StoreCollection;

use crate::core::{space, store};
use crate::types::{CreateSpaceDto, Space, SpaceConfig, SpaceMeta, SpaceReference, ZakuError};

#[tauri::command]
pub fn create_space(
    create_space_dto: CreateSpaceDto,
    app_handle: AppHandle,
    stores: State<'_, StoreCollection<Wry>>,
) -> Result<SpaceReference, ZakuError> {
    let location = PathBuf::from(create_space_dto.location.as_str());
    if !location.exists() {
        return Err(ZakuError {
            error: format!("Location does not exist: `{}`", create_space_dto.location),
        });
    }

    let space_root_path = location.join(create_space_dto.name.clone());
    let mut saved_spaces = store::get_saved_spaces(app_handle.clone(), stores.clone());

    if saved_spaces
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
    saved_spaces.push(space_reference.clone());
    store::set_saved_spaces(saved_spaces, app_handle, stores);

    return Ok(space_reference);
}

#[tauri::command]
pub fn get_active_space(
    app_handle: AppHandle,
    stores: State<'_, StoreCollection<Wry>>,
) -> Option<Space> {
    let active_space_reference = store::get_active_space(app_handle.clone(), stores.clone());

    if let Some(active_space_reference) = active_space_reference {
        match space::parse_space(&PathBuf::from(&active_space_reference.path)) {
            Ok(active_space) => return Some(active_space),
            Err(_) => match space::find_first_valid_space(app_handle.clone(), stores.clone()) {
                Some(valid_space_reference) => {
                    store::set_active_space(valid_space_reference.clone(), app_handle, stores);

                    return space::parse_space(&PathBuf::from(&valid_space_reference.path)).ok();
                }
                None => return None,
            },
        };
    } else {
        match space::find_first_valid_space(app_handle.clone(), stores.clone()) {
            Some(valid_space_reference) => {
                store::set_active_space(valid_space_reference.clone(), app_handle, stores);

                return space::parse_space(&PathBuf::from(&valid_space_reference.path)).ok();
            }
            None => return None,
        }
    }
}

#[tauri::command]
pub fn set_active_space(
    space_reference: SpaceReference,
    app_handle: AppHandle,
    stores: State<'_, StoreCollection<Wry>>,
) -> Result<(), ZakuError> {
    let space_root_path = PathBuf::from(space_reference.path.as_str());

    if !space_root_path.exists() {
        return Err(ZakuError {
            error: format!("Directory does not exist at {}", space_root_path.display()),
        });
    }

    match space::parse_space_config(&space_root_path) {
        Ok(_) => {
            store::set_active_space(space_reference.clone(), app_handle.clone(), stores.clone());
            let mut saved_spaces = store::get_saved_spaces(app_handle.clone(), stores.clone());
            let exists_in_saved_spaces = saved_spaces
                .iter()
                .any(|space_reference| space_reference.path == space_root_path.to_str().unwrap());

            if !exists_in_saved_spaces {
                println!("not inside store, pushing now {:?}", space_reference);

                saved_spaces.push(SpaceReference {
                    path: space_root_path.to_str().unwrap().to_string(),
                    name: space_reference.name.clone(),
                });

                store::set_saved_spaces(saved_spaces, app_handle, stores);
            }

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
) -> () {
    let active_space = store::get_active_space(app_handle.clone(), stores.clone());
    let mut saved_spaces = store::get_saved_spaces(app_handle.clone(), stores.clone());
    saved_spaces.retain(|space| space.path != space_reference.path);

    if let Some(active_space) = active_space {
        if active_space.path == space_reference.path {
            store::delete_active_space(app_handle.clone(), stores.clone());
        }
    }

    store::set_saved_spaces(saved_spaces, app_handle, stores);

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
pub fn get_saved_spaces(
    app_handle: AppHandle,
    stores: State<'_, StoreCollection<Wry>>,
) -> Vec<SpaceReference> {
    return store::get_saved_spaces(app_handle, stores);
}
