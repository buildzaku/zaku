use reqwest::Client;
use std::time::Instant;
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{AppHandle, Manager};

use crate::models::request::HttpErr;
use crate::{
    core::{self, buffer, collection, space},
    models::{
        collection::CreateCollectionDto,
        request::{CreateRequestDto, HttpReq, HttpRes},
        zaku::{ZakuError, ZakuState},
        CreateNewRequest,
    },
    utils,
};

#[specta::specta]
#[tauri::command]
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

#[specta::specta]
#[tauri::command]
pub fn save_request_to_buffer(absolute_space_path: &str, relative_path: &str, request: HttpReq) {
    let absolute = Path::new(absolute_space_path);
    let relative = Path::new(relative_path);
    buffer::save_request_to_space_buffer(absolute, relative, request);
}

#[specta::specta]
#[tauri::command]
pub fn write_buffer_request_to_fs(absolute_space_path: &str, request_relative_path: &str) {
    let absolute = Path::new(absolute_space_path);
    let relative = Path::new(request_relative_path);
    buffer::write_buffer_request_to_fs(absolute, relative).unwrap();
}

#[specta::specta]
#[tauri::command]
pub async fn http_req(req: HttpReq) -> Result<HttpRes, HttpErr> {
    let client = Client::new();
    let cfg = &req.config;

    let url = cfg.url.raw.clone().ok_or(HttpErr {
        message: "missing URL".into(),
        code: None,
    })?;

    let method = reqwest::Method::from_bytes(cfg.method.as_bytes()).map_err(|e| HttpErr {
        message: e.to_string(),
        code: None,
    })?;

    let mut builder = client.request(method, &url);

    for (enabled, key, value) in &cfg.headers {
        if *enabled {
            builder = builder.header(key, value);
        }
    }

    let query: Vec<_> = cfg
        .parameters
        .iter()
        .filter(|(enabled, _, _)| *enabled)
        .map(|(_, k, v)| (k.as_str(), v.as_str()))
        .collect();
    if !query.is_empty() {
        builder = builder.query(&query);
    }

    if let Some(ct) = &cfg.content_type {
        builder = builder.header("Content-Type", ct);
    }
    if let Some(body) = &cfg.body {
        builder = builder.body(body.clone());
    }

    let start = Instant::now();
    let resp = builder.send().await.map_err(|e| HttpErr {
        message: e.to_string(),
        code: None,
    })?;
    let elapsed_ms = start.elapsed().as_millis() as u32;

    let status = resp.status().as_u16();

    let headers: Vec<(String, String)> = resp
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();

    let cookies = resp
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|v| {
            let parts: Vec<&str> = v.split(';').collect();
            let kv: Vec<&str> = parts[0].splitn(2, '=').collect();
            if kv.len() == 2 {
                (kv[0].trim().to_string(), kv[1].trim().to_string())
            } else {
                (kv[0].trim().to_string(), "".to_string())
            }
        })
        .collect::<Vec<(String, String)>>();

    let data = resp.text().await.map_err(|e| HttpErr {
        message: e.to_string(),
        code: Some(status),
    })?;

    let size_bytes = Some(data.len() as u32);

    return Ok(HttpRes {
        status: Some(status),
        data,
        headers: Some(headers),
        cookies: if cookies.is_empty() {
            None
        } else {
            Some(cookies)
        },
        size_bytes,
        elapsed_ms: Some(elapsed_ms),
    });
}
