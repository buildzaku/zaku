use std::{path::PathBuf, sync::Mutex};

use tauri::{AppHandle, Manager};

use crate::{
    core::{self, collection, space},
    models::{
        collection::CreateCollectionDto,
        request::CreateRequestDto,
        zaku::{ZakuError, ZakuState},
        CreateNewCollectionOrRequest,
    },
};

#[tauri::command(rename_all = "snake_case")]
pub fn create_request(
    create_request_dto: CreateRequestDto,
    app_handle: AppHandle,
) -> Result<CreateNewCollectionOrRequest, ZakuError> {
    if create_request_dto.relative_path.is_empty() {
        return Err(ZakuError {
            error: "Cannot create a request without name".to_string(),
            message: "Cannot create a request without name".to_string(),
        });
    };

    println!("DTO\n{:#?}", create_request_dto.clone());

    let state = app_handle.state::<Mutex<ZakuState>>();
    let mut zaku_state = state.lock().unwrap();
    let active_space = zaku_state.active_space.clone().unwrap();
    let active_space_absolute_path = PathBuf::from(&active_space.absolute_path);

    let (parsed_parent_relative_path, file_display_name) =
        match create_request_dto.relative_path.rfind('/') {
            Some(last_slash) => {
                let start_to_second_last = &create_request_dto.relative_path[..last_slash];
                let last_part = &create_request_dto.relative_path[last_slash + 1..];

                (
                    Some(start_to_second_last.to_string()),
                    last_part.to_string(),
                )
            }
            None => (None, create_request_dto.relative_path.to_string()),
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

            let collections_sanitized_relative_path = collection::create_collections_all(
                &active_space_absolute_path,
                &create_collection_dto,
            )
            .map_err(|err| ZakuError {
                error: err.to_string(),
                message: "Failed to create request's parent directories".to_string(),
            })?;

            let file_parent_relative_path = vec![
                create_request_dto.parent_relative_path,
                collections_sanitized_relative_path,
            ]
            .iter()
            .filter(|&path| !path.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join("/");

            (file_parent_relative_path, file_sanitized_name)
        }
        None => (create_request_dto.parent_relative_path, file_sanitized_name),
    };

    println!(
        "file_parent_relative_path, {}",
        file_parent_relative_path.clone()
    );
    println!("file_sanitized_name, {}", file_sanitized_name.clone());
    let file_absolute_path = active_space_absolute_path
        .join(file_parent_relative_path.clone())
        .join(file_sanitized_name.clone());
    let file_relative_path = vec![
        file_parent_relative_path.clone(),
        format!("{}.toml", file_sanitized_name),
    ]
    .iter()
    .filter(|&path| !path.is_empty())
    .cloned()
    .collect::<Vec<_>>()
    .join("/");

    core::request::create_request_file(&file_absolute_path, &file_display_name).map_err(|err| {
        ZakuError {
            error: err.to_string(),
            message: "Failed to create request file".to_string(),
        }
    })?;

    let create_new_result = CreateNewCollectionOrRequest {
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
