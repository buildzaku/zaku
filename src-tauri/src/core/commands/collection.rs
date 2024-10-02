use std::{fs, path::PathBuf, sync::Mutex};

use tauri::{AppHandle, Manager};

use crate::{
    core::space,
    models::{collection::CreateCollectionDto, zaku::ZakuState},
};

#[tauri::command(rename_all = "snake_case")]
pub fn create_collection(create_collection_dto: CreateCollectionDto, app_handle: AppHandle) {
    let state = app_handle.state::<Mutex<ZakuState>>();
    let mut zaku_state = state.lock().unwrap();
    let active_space = zaku_state.active_space.clone().unwrap();
    let active_space_path = PathBuf::from(&active_space.absolute_path);
    let collection_path = active_space_path
        .join(create_collection_dto.relative_location)
        .join(create_collection_dto.folder_name);

    fs::create_dir_all(&collection_path)
        .expect("Failed to create collection directory or it's parent directories");

    match space::parse_space(&active_space_path) {
        Ok(active_space) => {
            zaku_state.active_space = Some(active_space);
        }
        Err(_) => {}
    }

    return ();
}
