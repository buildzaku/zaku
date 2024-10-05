use std::{path::PathBuf, sync::Mutex};

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

    collection::create_collections_all(&active_space_absolute_path, create_collection_dto)
        .map_err(|err| ZakuError {
            error: err.to_string(),
            message: "Failed to create collection directory or it's parent directories".to_string(),
        })?;

    let active_space =
        space::parse_space(&active_space_absolute_path).map_err(|err| ZakuError {
            error: err.to_string(),
            message: "Failed to parse space after creating the collection".to_string(),
        })?;

    zaku_state.active_space = Some(active_space);

    return Ok(());
}
