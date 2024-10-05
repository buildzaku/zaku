use std::{fs, path::PathBuf, sync::Mutex};

use tauri::{AppHandle, Manager};

use crate::{
    core::{self, space},
    models::{
        request::CreateRequestDto,
        zaku::{ZakuError, ZakuState},
    },
};

#[tauri::command(rename_all = "snake_case")]
pub fn create_request(
    create_request_dto: CreateRequestDto,
    app_handle: AppHandle,
) -> Result<(), ZakuError> {
    if create_request_dto.file_relative_path.is_empty() {
        return Err(ZakuError {
            error: "Cannot create a request without name".to_string(),
            message: "Cannot create a request without name".to_string(),
        });
    };

    let state = app_handle.state::<Mutex<ZakuState>>();
    let mut zaku_state = state.lock().unwrap();
    let active_space = zaku_state.active_space.clone().unwrap();

    let active_space_absolute_path = PathBuf::from(&active_space.absolute_path);
    let file_relative_path = create_request_dto
        .file_relative_path
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join("-");
    let file_absolute_path = active_space_absolute_path
        .join(create_request_dto.relative_location.clone())
        .join(file_relative_path);

    if let Some(parent) = file_absolute_path.parent() {
        fs::create_dir_all(parent).map_err(|err| ZakuError {
            error: err.to_string(),
            message: "Failed to create request's collection directory or it's parent directories"
                .to_string(),
        })?;
    }

    println!("creating with {:#?}", &file_absolute_path);

    core::request::create_request_file(&file_absolute_path, &create_request_dto.display_name)
        .map_err(|err| ZakuError {
            error: err.to_string(),
            message: "Failed to create request file".to_string(),
        })?;

    let active_space =
        space::parse_space(&active_space_absolute_path).map_err(|err| ZakuError {
            error: err.to_string(),
            message: "Failed to parse space after creating the request".to_string(),
        })?;

    zaku_state.active_space = Some(active_space);

    return Ok(());
}
