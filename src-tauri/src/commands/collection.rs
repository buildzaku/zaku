use std::{fs, path::PathBuf, sync::Mutex};
use tauri::{AppHandle, Manager};

use crate::{
    core::utils,
    core::{collection, space},
    models::{
        collection::CreateCollectionDto,
        zaku::{ZakuError, ZakuState},
        CreateNewCollection,
    },
};

#[specta::specta]
#[tauri::command]
pub fn create_collection(
    create_collection_dto: CreateCollectionDto,
    app_handle: AppHandle,
) -> Result<CreateNewCollection, ZakuError> {
    if create_collection_dto.relpath.is_empty() {
        return Err(ZakuError {
            error: "Cannot create a collection without name".to_string(),
            message: "Cannot create a collection without name".to_string(),
        });
    };

    let state = app_handle.state::<Mutex<ZakuState>>();
    let mut zaku_state = state.lock().unwrap();
    let active_space = zaku_state
        .active_space
        .clone()
        .expect("Active space not found");
    let active_space_abspath = PathBuf::from(&active_space.abspath);

    let (parsed_parent_relpath, dir_display_name) = match create_collection_dto.relpath.rfind('/') {
        Some(last_slash_index) => {
            let parsed_parent_relpath = &create_collection_dto.relpath[..last_slash_index];
            let dir_display_name = &create_collection_dto.relpath[last_slash_index + 1..];

            (
                Some(parsed_parent_relpath.to_string()),
                dir_display_name.to_string(),
            )
        }
        None => (None, create_collection_dto.relpath),
    };

    let dir_display_name = dir_display_name.trim();
    let dir_sanitized_name = dir_display_name
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join("-");
    let (dir_parent_relpath, dir_sanitized_name) = match parsed_parent_relpath {
        Some(ref parsed_parent_relpath) => {
            let create_collection_dto = CreateCollectionDto {
                parent_relpath: create_collection_dto.parent_relpath.clone(),
                relpath: parsed_parent_relpath.to_string(),
            };

            let dirs_sanitized_relpath =
                collection::create_collections_all(&active_space_abspath, &create_collection_dto)
                    .map_err(|err| ZakuError {
                    error: err.to_string(),
                    message: "Failed to create collection's parent directories".to_string(),
                })?;

            let dir_parent_relpath = utils::join_str_paths(vec![
                create_collection_dto.parent_relpath.as_str(),
                dirs_sanitized_relpath.as_str(),
            ]);

            (dir_parent_relpath, dir_sanitized_name)
        }
        None => (create_collection_dto.parent_relpath, dir_sanitized_name),
    };

    let dir_abspath = active_space_abspath
        .join(dir_parent_relpath.clone())
        .join(dir_sanitized_name.clone());
    let dir_relpath = utils::join_str_paths(vec![
        dir_parent_relpath.clone().as_str(),
        dir_sanitized_name.as_str(),
    ]);

    fs::create_dir(&dir_abspath).map_err(|err| ZakuError {
        error: err.to_string(),
        message: "Failed to create collection".to_string(),
    })?;

    collection::save_displayname_if_missing(&active_space_abspath, &dir_relpath, &dir_display_name)
        .unwrap_or_else(|err| {
            eprintln!("Failed to save display name {}", err);
        });

    let create_new_collection = CreateNewCollection {
        parent_relpath: dir_parent_relpath,
        relpath: dir_relpath,
    };

    match space::parse_space(&active_space_abspath) {
        Ok(active_space) => zaku_state.active_space = Some(active_space),
        Err(err) => {
            return Err(ZakuError {
                error: err.to_string(),
                message: "Failed to parse space after creating the collection".to_string(),
            })
        }
    }

    return Ok(create_new_collection);
}
