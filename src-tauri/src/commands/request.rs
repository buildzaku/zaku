use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{AppHandle, Manager};

use crate::{
    core::{self, buffer, collection, space},
    models::{
        collection::CreateCollectionDto,
        request::{CreateRequestDto, Request},
        zaku::{ZakuError, ZakuState},
        CreateNewRequest,
    },
    utils,
};

#[tauri::command(rename_all = "snake_case")]
pub fn create_request(
    create_request_dto: CreateRequestDto,
    app_handle: AppHandle,
) -> Result<CreateNewRequest, ZakuError> {
    if create_request_dto.relative_path.is_empty() {
        return Err(ZakuError {
            error: "Cannot create a request without name".to_string(),
            message: "Cannot create a request without name".to_string(),
        });
    };

    let state = app_handle.state::<Mutex<ZakuState>>();
    let mut zaku_state = state.lock().unwrap();
    let active_space = zaku_state
        .active_space
        .clone()
        .expect("Active space not found");
    let active_space_absolute_path = PathBuf::from(&active_space.absolute_path);

    let (parsed_parent_relative_path, file_display_name) =
        match create_request_dto.relative_path.rfind('/') {
            Some(last_slash_index) => {
                let parsed_parent_relative_path =
                    &create_request_dto.relative_path[..last_slash_index];
                let file_display_name = &create_request_dto.relative_path[last_slash_index + 1..];

                (
                    Some(parsed_parent_relative_path.to_string()),
                    file_display_name.to_string(),
                )
            }
            None => (None, create_request_dto.relative_path),
        };

    let file_display_name = file_display_name.trim();
    let file_sanitized_name = file_display_name
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join("-");
    let (file_parent_relative_path, file_sanitized_name) = match parsed_parent_relative_path {
        Some(ref parsed_parent_relative_path) => {
            let create_collection_dto = CreateCollectionDto {
                parent_relative_path: create_request_dto.parent_relative_path.clone(),
                relative_path: parsed_parent_relative_path.to_string(),
            };

            let dirs_sanitized_relative_path = collection::create_collections_all(
                &active_space_absolute_path,
                &create_collection_dto,
            )
            .map_err(|err| ZakuError {
                error: err.to_string(),
                message: "Failed to create request's parent directories".to_string(),
            })?;

            let file_parent_relative_path = utils::join_str_paths(vec![
                create_request_dto.parent_relative_path.as_str(),
                dirs_sanitized_relative_path.as_str(),
            ]);

            (file_parent_relative_path, file_sanitized_name)
        }
        None => (create_request_dto.parent_relative_path, file_sanitized_name),
    };

    let file_absolute_path = active_space_absolute_path
        .join(file_parent_relative_path.clone())
        .join(file_sanitized_name.clone());
    let file_relative_path = utils::join_str_paths(vec![
        file_parent_relative_path.clone().as_str(),
        format!("{}.toml", file_sanitized_name).as_str(),
    ]);

    core::request::create_request_file(&file_absolute_path, &file_display_name).map_err(|err| {
        ZakuError {
            error: err.to_string(),
            message: "Failed to create request file".to_string(),
        }
    })?;

    let create_new_result = CreateNewRequest {
        parent_relative_path: file_parent_relative_path,
        relative_path: file_relative_path,
    };

    match space::parse_space(&active_space_absolute_path) {
        Ok(active_space) => zaku_state.active_space = Some(active_space),
        Err(err) => {
            return Err(ZakuError {
                error: err.to_string(),
                message: "Failed to parse space after creating the request".to_string(),
            })
        }
    }

    return Ok(create_new_result);
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_request_to_buffer(absolute_space_path: &Path, relative_path: &Path, request: Request) {
    buffer::save_request_to_space_buffer(absolute_space_path, relative_path, request);
}

#[tauri::command(rename_all = "snake_case")]
pub fn write_buffer_request_to_fs(absolute_space_path: &Path, request_relative_path: &Path) {
    buffer::write_buffer_request_to_fs(absolute_space_path, request_relative_path).unwrap();
}
