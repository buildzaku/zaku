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
    if create_collection_dto.relative_path.is_empty() {
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
    let active_space_absolute_path = PathBuf::from(&active_space.absolute_path);

    let (parsed_parent_relative_path, dir_display_name) =
        match create_collection_dto.relative_path.rfind('/') {
            Some(last_slash_index) => {
                let parsed_parent_relative_path =
                    &create_collection_dto.relative_path[..last_slash_index];
                let dir_display_name = &create_collection_dto.relative_path[last_slash_index + 1..];

                (
                    Some(parsed_parent_relative_path.to_string()),
                    dir_display_name.to_string(),
                )
            }
            None => (None, create_collection_dto.relative_path),
        };

    let dir_display_name = dir_display_name.trim();
    let dir_sanitized_name = dir_display_name
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join("-");
    let (dir_parent_relative_path, dir_sanitized_name) = match parsed_parent_relative_path {
        Some(ref parsed_parent_relative_path) => {
            let create_collection_dto = CreateCollectionDto {
                parent_relative_path: create_collection_dto.parent_relative_path.clone(),
                relative_path: parsed_parent_relative_path.to_string(),
            };

            let dirs_sanitized_relative_path = collection::create_collections_all(
                &active_space_absolute_path,
                &create_collection_dto,
            )
            .map_err(|err| ZakuError {
                error: err.to_string(),
                message: "Failed to create collection's parent directories".to_string(),
            })?;

            let dir_parent_relative_path = utils::join_str_paths(vec![
                create_collection_dto.parent_relative_path.as_str(),
                dirs_sanitized_relative_path.as_str(),
            ]);

            (dir_parent_relative_path, dir_sanitized_name)
        }
        None => (
            create_collection_dto.parent_relative_path,
            dir_sanitized_name,
        ),
    };

    let dir_absolute_path = active_space_absolute_path
        .join(dir_parent_relative_path.clone())
        .join(dir_sanitized_name.clone());
    let dir_relative_path = utils::join_str_paths(vec![
        dir_parent_relative_path.clone().as_str(),
        dir_sanitized_name.as_str(),
    ]);

    fs::create_dir(&dir_absolute_path).map_err(|err| ZakuError {
        error: err.to_string(),
        message: "Failed to create collection".to_string(),
    })?;

    collection::save_display_name_if_not_exists(
        &active_space_absolute_path,
        &dir_relative_path,
        &dir_display_name,
    )
    .unwrap_or_else(|err| {
        eprintln!("Failed to save display name {}", err);
    });

    let create_new_result = CreateNewCollection {
        parent_relative_path: dir_parent_relative_path,
        relative_path: dir_relative_path,
    };

    match space::parse_space(&active_space_absolute_path) {
        Ok(active_space) => zaku_state.active_space = Some(active_space),
        Err(err) => {
            return Err(ZakuError {
                error: err.to_string(),
                message: "Failed to parse space after creating the collection".to_string(),
            })
        }
    }

    return Ok(create_new_result);
}
