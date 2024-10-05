use std::{fs, path::PathBuf, sync::Mutex};

use tauri::{AppHandle, Manager};

use crate::{
    core::{collection, space},
    models::{
        collection::CreateCollectionDto,
        zaku::{ZakuError, ZakuState},
    },
};

#[tauri::command(rename_all = "snake_case")]
pub fn create_collection(
    create_collection_dto: CreateCollectionDto,
    app_handle: AppHandle,
) -> Result<(), ZakuError> {
    let state = app_handle.state::<Mutex<ZakuState>>();
    let mut zaku_state = state.lock().unwrap();
    let active_space = zaku_state.active_space.clone().unwrap();
    let active_space_absolute_path = PathBuf::from(&active_space.absolute_path);
    let folder_relative_path = create_collection_dto
        .folder_relative_path
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join("-");
    let collection_absolute_path = active_space_absolute_path
        .join(create_collection_dto.relative_location.clone())
        .join(folder_relative_path.clone());

    if let Some(parent) = collection_absolute_path.parent() {
        fs::create_dir_all(parent).map_err(|err| ZakuError {
            error: err.to_string(),
            message: "Failed to create collection directory or it's parent directories".to_string(),
        })?;
    }

    let collection_relative_path = format!(
        "{}/{}",
        create_collection_dto.relative_location, folder_relative_path
    );

    collection::save_display_name(
        &active_space_absolute_path,
        &collection_relative_path,
        &create_collection_dto.display_name,
    )
    .unwrap_or_else(|err| {
        eprintln!("Failed to save display name {}", err);
    });

    match space::parse_space(&active_space_absolute_path) {
        Ok(active_space) => {
            zaku_state.active_space = Some(active_space);

            return Ok(());
        }
        Err(err) => {
            return Err(ZakuError {
                error: err.to_string(),
                message: "Failed to parse space after creating the collection".to_string(),
            });
        }
    }
}
