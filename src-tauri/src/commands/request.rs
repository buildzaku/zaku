use cookie::Cookie as RawCookie;
use std::sync::Arc;
use std::time::Instant;
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{AppHandle, Manager};

use crate::core::cookie::SpaceCookies;
use crate::core::store;
use crate::core::store::spaces::settings::SpaceSettings;
use crate::models::request::HttpErr;
use crate::models::space::SpaceCookie;
use crate::notification;
use crate::{
    core::utils,
    core::{self, buffer, collection, space},
    models::{
        collection::CreateCollectionDto,
        request::{CreateRequestDto, HttpReq, HttpRes},
        zaku::{ZakuError, ZakuState},
        CreateNewRequest,
    },
};

#[specta::specta]
#[tauri::command]
pub fn create_req(
    create_req_dto: CreateRequestDto,
    app_handle: AppHandle,
) -> Result<CreateNewRequest, ZakuError> {
    if create_req_dto.relpath.is_empty() {
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
    let active_space_abspath = PathBuf::from(&active_space.abspath);

    let (parsed_parent_relpath, file_display_name) = match create_req_dto.relpath.rfind('/') {
        Some(last_slash_index) => {
            let parsed_parent_relpath = &create_req_dto.relpath[..last_slash_index];
            let file_display_name = &create_req_dto.relpath[last_slash_index + 1..];

            (
                Some(parsed_parent_relpath.to_string()),
                file_display_name.to_string(),
            )
        }
        None => (None, create_req_dto.relpath),
    };

    let file_display_name = file_display_name.trim();
    let file_sanitized_name = file_display_name
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join("-");
    let (file_parent_relpath, file_sanitized_name) = match parsed_parent_relpath {
        Some(ref parsed_parent_relpath) => {
            let create_collection_dto = CreateCollectionDto {
                parent_relpath: create_req_dto.parent_relpath.clone(),
                relpath: parsed_parent_relpath.to_string(),
            };

            let dirs_sanitized_relpath =
                collection::create_collections_all(&active_space_abspath, &create_collection_dto)
                    .map_err(|err| ZakuError {
                    error: err.to_string(),
                    message: "Failed to create request's parent directories".to_string(),
                })?;

            let file_parent_relpath = utils::join_str_paths(vec![
                create_req_dto.parent_relpath.as_str(),
                dirs_sanitized_relpath.as_str(),
            ]);

            (file_parent_relpath, file_sanitized_name)
        }
        None => (create_req_dto.parent_relpath, file_sanitized_name),
    };

    let file_abspath = active_space_abspath
        .join(file_parent_relpath.clone())
        .join(file_sanitized_name.clone());
    let file_relpath = utils::join_str_paths(vec![
        file_parent_relpath.clone().as_str(),
        format!("{}.toml", file_sanitized_name).as_str(),
    ]);

    core::request::create_reqtoml(&file_abspath, &file_display_name).map_err(|err| ZakuError {
        error: err.to_string(),
        message: "Failed to create request file".to_string(),
    })?;

    let create_new_result = CreateNewRequest {
        parent_relpath: file_parent_relpath,
        relpath: file_relpath,
    };

    match space::parse_space(&active_space_abspath) {
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
pub fn persist_to_reqbuf(space_abspath: &str, relpath: &str, request: HttpReq) {
    let abs = Path::new(space_abspath);
    let rel = Path::new(relpath);
    buffer::persist_req_to_spacebuf(abs, rel, request);
}

#[specta::specta]
#[tauri::command]
pub fn write_reqbuf_to_reqtoml(space_abspath: &str, req_relpath: &str) {
    let abs = Path::new(space_abspath);
    let rel = Path::new(req_relpath);
    buffer::write_reqbuf_to_reqtoml(abs, rel).unwrap();
}

#[specta::specta]
#[tauri::command]
pub async fn http_req(req: HttpReq) -> Result<HttpRes, HttpErr> {
    let active_space = store::get_active_spaceref().ok_or(HttpErr {
        message: "no active space".into(),
        code: None,
    })?;
    let space_abspath = active_space.path.as_str();
    let cookie_store = SpaceCookies::load(space_abspath);
    let space_settings = SpaceSettings::load(space_abspath);
    let client = reqwest::Client::builder()
        .cookie_provider(Arc::clone(&cookie_store))
        .build()
        .map_err(|e| HttpErr {
            message: e.to_string(),
            code: None,
        })?;
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
    if space_settings.notifications.audio.on_req_complete {
        tokio::spawn(async {
            let _ = notification::play_notif_sound().await; // TODO - handle failures, send toast to UI?
        });
    }

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
        .filter_map(|v| RawCookie::parse(v).ok())
        .map(|ck| SpaceCookie::from_raw_cookie(&ck))
        .collect::<Vec<SpaceCookie>>();
    let data = resp.text().await.map_err(|e| HttpErr {
        message: e.to_string(),
        code: Some(status),
    })?;
    let size_bytes = Some(data.len() as u32);
    SpaceCookies::persist(space_abspath);

    return Ok(HttpRes {
        status: Some(status),
        data,
        headers,
        cookies,
        size_bytes,
        elapsed_ms: Some(elapsed_ms),
    });
}
