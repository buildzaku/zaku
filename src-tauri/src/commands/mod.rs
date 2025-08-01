use cookie::Cookie as RawCookie;
use dirs;
use std::{collections::HashMap, path::PathBuf, sync::OnceLock, time::Instant};
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_notification::{NotificationExt, PermissionState};

use crate::{
    collection::{
        self,
        models::{CreateCollectionDto, CreateNewCollection},
    },
    commands::models::{CreateNewRequest, DispatchNotificationOptions, OpenDirDialogOpt},
    error::{CmdErr, CmdResult, ErrorKind},
    notifications,
    request::{
        self,
        models::{CreateRequestDto, HttpReq, HttpRes, ReqToml},
    },
    space::{
        self,
        models::{CreateSpaceDto, RemoveCookieDto, SerializedCookie, SpaceReference},
    },
    store::{
        self, ReqBuffer, SpaceCookieStore, SpaceSettings, StateStore,
        spaces::{buffer::SpaceBufferStore, settings::SpaceSettingsStore},
        state::SharedState,
    },
    tree_node::{self, MoveTreeNodeDto},
};

pub mod models;

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Returns the absolute path to the application's data directory.
pub fn datadir_abspath() -> PathBuf {
    DATA_DIR
        .get_or_init(|| {
            dirs::data_dir()
                .expect("Unable to get data directory")
                .join("Zaku")
        })
        .clone()
}

pub fn collect() -> tauri_specta::Commands<tauri::Wry> {
    tauri_specta::collect_commands![
        get_shared_state,
        create_space,
        set_space,
        remove_space,
        get_spaceref,
        remove_cookie,
        get_space_cookies,
        save_space_settings,
        show_main_window,
        open_dir_dialog,
        is_notif_enabled,
        request_notif_access,
        dispatch_notif,
        create_collection,
        create_req,
        write_req_to_space_buffer,
        write_reqbuf_to_reqtoml,
        http_req,
        move_tree_node
    ]
}

#[specta::specta]
#[tauri::command]
pub fn get_shared_state() -> CmdResult<SharedState> {
    let state_store_abspath = store::utils::state_store_abspath(&datadir_abspath());
    let state_store = StateStore::get(&state_store_abspath).map_err(|err| CmdErr {
        kind: ErrorKind::FileReadError,
        message: "Unable to load state store".to_string(),
        details: Some(err.to_string()),
    })?;

    let shared_state = SharedState::from_state_store(&state_store).map_err(|err| CmdErr {
        kind: ErrorKind::ParseError,
        message: "Unable to parse space".to_string(),
        details: Some(err.to_string()),
    })?;

    Ok(shared_state)
}

#[specta::specta]
#[tauri::command]
pub async fn create_collection(dto: CreateCollectionDto) -> CmdResult<CreateNewCollection> {
    let state_store_abspath = store::utils::state_store_abspath(&datadir_abspath());
    let state_store = StateStore::get(&state_store_abspath).map_err(|err| CmdErr {
        kind: ErrorKind::FileReadError,
        message: "Unable to load state store".to_string(),
        details: Some(err.to_string()),
    })?;

    let space_abspath = state_store
        .spaceref
        .as_ref()
        .ok_or(CmdErr {
            kind: ErrorKind::SpaceNotFoundError,
            message: "No current space selected".to_string(),
            details: None,
        })?
        .abspath
        .clone();

    let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
        &dto.location_relpath,
        &dto.relpath,
        &space_abspath,
    )
    .map_err(|err| CmdErr {
        kind: ErrorKind::FileWriteError,
        message: "Unable to create parent collections".to_string(),
        details: Some(err.to_string()),
    })?;

    collection::create_collection(&parent_relpath, &col_segment, &space_abspath).map_err(|err| {
        CmdErr {
            kind: ErrorKind::FileWriteError,
            message: "Unable to create collection".to_string(),
            details: Some(err.to_string()),
        }
    })
}

#[specta::specta]
#[tauri::command]
pub async fn open_dir_dialog(
    options: Option<OpenDirDialogOpt>,
    app_handle: tauri::AppHandle,
) -> CmdResult<Option<String>> {
    let mut dialog_builder = app_handle.dialog().file();

    if let Some(OpenDirDialogOpt {
        title: Some(ref title),
    }) = options
    {
        dialog_builder = dialog_builder.set_title(title);
    }

    let directory_path = dialog_builder.blocking_pick_folder();

    match directory_path {
        Some(path) => {
            let path_buf = path.into_path().map_err(|err| CmdErr {
                kind: ErrorKind::DialogOpenError,
                message: "Unable to process selected directory".to_string(),
                details: Some(err.to_string()),
            })?;

            Ok(Some(path_buf.to_string_lossy().to_string()))
        }
        None => Ok(None),
    }
}

#[specta::specta]
#[tauri::command]
pub fn is_notif_enabled(app_handle: tauri::AppHandle) -> CmdResult<bool> {
    let permission_state = app_handle
        .notification()
        .permission_state()
        .map_err(|err| CmdErr {
            kind: ErrorKind::NotificationPermissionError,
            message: "Unable to get notification permissions".to_string(),
            details: Some(err.to_string()),
        })?;

    Ok(permission_state == PermissionState::Granted)
}

#[specta::specta]
#[tauri::command]
pub fn request_notif_access(app_handle: tauri::AppHandle) -> CmdResult<bool> {
    let permission_state = app_handle
        .notification()
        .request_permission()
        .map_err(|err| CmdErr {
            kind: ErrorKind::NotificationPermissionError,
            message: "Unable to request notification permissions".to_string(),
            details: Some(err.to_string()),
        })?;

    Ok(permission_state == PermissionState::Granted)
}

#[specta::specta]
#[tauri::command]
pub fn dispatch_notif(
    options: DispatchNotificationOptions,
    app_handle: tauri::AppHandle,
) -> CmdResult<()> {
    app_handle
        .notification()
        .builder()
        .title(&options.title)
        .body(&options.body)
        .show()
        .map_err(|err| CmdErr {
            kind: ErrorKind::NotificationDispatchError,
            message: "Unable to dispatch notification".to_string(),
            details: Some(err.to_string()),
        })?;

    Ok(())
}

#[specta::specta]
#[tauri::command]
pub async fn create_req(dto: CreateRequestDto) -> CmdResult<CreateNewRequest> {
    let state_store_abspath = store::utils::state_store_abspath(&datadir_abspath());
    let state_store = StateStore::get(&state_store_abspath).map_err(|err| CmdErr {
        kind: ErrorKind::FileReadError,
        message: "Unable to load state store".to_string(),
        details: Some(err.to_string()),
    })?;

    let space_abspath = state_store
        .spaceref
        .as_ref()
        .ok_or(CmdErr {
            kind: ErrorKind::SpaceNotFoundError,
            message: "No current space selected".to_string(),
            details: None,
        })?
        .abspath
        .clone();

    let (parent_relpath, req_segment) = collection::create_parent_collections_if_missing(
        &dto.location_relpath,
        &dto.relpath,
        &space_abspath,
    )
    .map_err(|err| CmdErr {
        kind: ErrorKind::FileWriteError,
        message: "Unable to create parent collections".to_string(),
        details: Some(err.to_string()),
    })?;

    request::create_req(&parent_relpath, &req_segment, &space_abspath).map_err(|err| CmdErr {
        kind: ErrorKind::FileWriteError,
        message: "Unable to create request".to_string(),
        details: Some(err.to_string()),
    })
}

#[specta::specta]
#[tauri::command]
pub async fn write_req_to_space_buffer(space_abspath: PathBuf, request: HttpReq) -> CmdResult<()> {
    let sbf_store_abspath = store::utils::sbf_store_abspath(&datadir_abspath(), &space_abspath);
    let sbf_store = SpaceBufferStore::get(&sbf_store_abspath).map_err(|err| CmdErr {
        kind: ErrorKind::FileReadError,
        message: "Unable to load space buffer".to_string(),
        details: Some(err.to_string()),
    })?;

    SpaceBufferStore::update(&sbf_store, |sbf_store| {
        let mut sbf_store_mtx = sbf_store.lock().expect("Failed to lock SpaceBufferStore");

        let req_buf = ReqBuffer::from_req(&request);

        sbf_store_mtx
            .requests
            .insert(request.meta.relpath.clone(), req_buf);
    })
    .map_err(|err| CmdErr {
        kind: ErrorKind::FileWriteError,
        message: "Unable to save request".to_string(),
        details: Some(err.to_string()),
    })?;

    Ok(())
}

#[specta::specta]
#[tauri::command]
pub async fn write_reqbuf_to_reqtoml(
    space_abspath: PathBuf,
    req_relpath: PathBuf,
) -> CmdResult<()> {
    let space_abspath = &space_abspath;
    let sbf_store_abspath = store::utils::sbf_store_abspath(&datadir_abspath(), space_abspath);
    let sbf_store = SpaceBufferStore::get(&sbf_store_abspath).map_err(|err| CmdErr {
        kind: ErrorKind::FileReadError,
        message: "Unable to load space buffer".to_string(),
        details: Some(err.to_string()),
    })?;

    SpaceBufferStore::update(&sbf_store, |sbf_store| {
        let mut sbf_store_mtx = sbf_store.lock().unwrap();

        if let Some(req_buf) = sbf_store_mtx.requests.get(&req_relpath) {
            let req_toml = ReqToml::from_reqbuf(req_buf);
            let _ = request::update_reqtoml(&space_abspath.join(&req_relpath), &req_toml);
        }

        sbf_store_mtx.requests.remove(&req_relpath);
    })
    .map_err(|err| CmdErr {
        kind: ErrorKind::FileWriteError,
        message: "Unable to save request".to_string(),
        details: Some(err.to_string()),
    })?;

    Ok(())
}

#[specta::specta]
#[tauri::command]
pub async fn http_req(req: HttpReq, app_handle: tauri::AppHandle) -> CmdResult<HttpRes> {
    let store_abspath = store::utils::state_store_abspath(&datadir_abspath());
    let state_store = StateStore::get(&store_abspath).map_err(|err| CmdErr {
        kind: ErrorKind::FileReadError,
        message: "Unable to load store".to_string(),
        details: Some(err.to_string()),
    })?;

    let spaceref = state_store.spaceref.clone().ok_or(CmdErr {
        kind: ErrorKind::SpaceNotFoundError,
        message: "Unable to find current space".to_string(),
        details: None,
    })?;

    let sck_store_abspath = store::utils::sck_store_abspath(&datadir_abspath(), &spaceref.abspath);
    let mut sck_store = SpaceCookieStore::get(&sck_store_abspath).map_err(|err| CmdErr {
        kind: ErrorKind::FileReadError,
        message: "Unable to load cookies".to_string(),
        details: Some(err.to_string()),
    })?;

    let sst_store_abspath = store::utils::sst_store_abspath(&datadir_abspath(), &spaceref.abspath);
    let sst_store = SpaceSettingsStore::get(&sst_store_abspath).map_err(|err| CmdErr {
        kind: ErrorKind::FileReadError,
        message: "Unable to load space settings".to_string(),
        details: Some(err.to_string()),
    })?;

    let client = reqwest::Client::builder()
        .cookie_provider(sck_store.cookies.clone())
        .build()
        .map_err(|err| CmdErr {
            kind: ErrorKind::NetworkError,
            message: "Unable to build request".to_string(),
            details: Some(err.to_string()),
        })?;

    let cfg = &req.config;
    let url = cfg.url.raw.clone().ok_or(CmdErr {
        kind: ErrorKind::InvalidUrlError,
        message: "URL field is empty".to_string(),
        details: None,
    })?;

    let method = reqwest::Method::from_bytes(cfg.method.as_bytes()).map_err(|e| CmdErr {
        kind: ErrorKind::InvalidUrlError,
        message: "Invalid HTTP method".to_string(),
        details: Some(e.to_string()),
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
    let resp = builder.send().await.map_err(|err| CmdErr {
        kind: ErrorKind::NetworkError,
        message: "Unable to send request".to_string(),
        details: Some(err.to_string()),
    })?;

    if sst_store.notifications.audio.on_req_finish {
        let app_handle = app_handle.clone();
        tokio::spawn(async move {
            let _ = notifications::play_finish(&app_handle); // TODO - handle failures, send toast to UI?
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
        .map(|ck| SerializedCookie::from_raw_cookie(&ck))
        .collect::<Vec<SerializedCookie>>();
    let data = resp.text().await.map_err(|err| CmdErr {
        kind: ErrorKind::NetworkError,
        message: "Unable to read response".to_string(),
        details: Some(err.to_string()),
    })?;
    let size_bytes = Some(data.len() as u32);

    sck_store.update(|_| {}).map_err(|err| CmdErr {
        kind: ErrorKind::FileWriteError,
        message: "Unable to save cookies".to_string(),
        details: Some(err.to_string()),
    })?;

    Ok(HttpRes {
        status: Some(status),
        data,
        headers,
        cookies,
        size_bytes,
        elapsed_ms: Some(elapsed_ms),
    })
}

#[specta::specta]
#[tauri::command]
pub async fn create_space(create_space_dto: CreateSpaceDto) -> CmdResult<SpaceReference> {
    let state_store_abspath = store::utils::state_store_abspath(&datadir_abspath());
    let mut state_store = StateStore::get(&state_store_abspath).map_err(|err| CmdErr {
        kind: ErrorKind::FileReadError,
        message: "Unable to load state store".to_string(),
        details: Some(err.to_string()),
    })?;

    let space_ref =
        space::create_space(create_space_dto, &mut state_store).map_err(|err| CmdErr {
            kind: ErrorKind::FileWriteError,
            message: "Unable to create space, make sure the location exists".to_string(),
            details: Some(err.to_string()),
        })?;

    Ok(space_ref)
}

#[specta::specta]
#[tauri::command]
pub fn set_space(space_reference: SpaceReference) -> CmdResult<()> {
    let space_abspath = &space_reference.abspath;
    if !space_abspath.exists() {
        return Err(CmdErr {
            kind: ErrorKind::FileNotFoundError,
            message: "Unable to find space directory".to_string(),
            details: Some(space_abspath.to_string_lossy().to_string()),
        });
    }

    let state_store_abspath = store::utils::state_store_abspath(&datadir_abspath());
    let mut state_store = StateStore::get(&state_store_abspath).map_err(|e| CmdErr {
        kind: ErrorKind::FileReadError,
        message: "Unable to load state store for validation".to_string(),
        details: Some(e.to_string()),
    })?;

    space::parse_space(space_abspath, &state_store).map_err(|err| CmdErr {
        kind: ErrorKind::ParseError,
        message: "Unable to load space".to_string(),
        details: Some(err.to_string()),
    })?;

    state_store
        .update(|state| {
            state.spaceref = Some(space_reference.clone());

            let spaceref_exists = state
                .spacerefs
                .iter()
                .any(|r| r.abspath == space_reference.abspath);

            if !spaceref_exists {
                state.spacerefs.push(space_reference.clone());
            }
        })
        .map_err(|e| CmdErr {
            kind: ErrorKind::FileWriteError,
            message: "Unable to save space".to_string(),
            details: Some(e.to_string()),
        })?;

    Ok(())
}

#[specta::specta]
#[tauri::command]
pub fn remove_space(space_reference: SpaceReference) -> CmdResult<()> {
    let state_store_abspath = store::utils::state_store_abspath(&datadir_abspath());
    let mut state_store = StateStore::get(&state_store_abspath).map_err(|e| CmdErr {
        kind: ErrorKind::FileReadError,
        message: "Unable to load state store".to_string(),
        details: Some(e.to_string()),
    })?;

    state_store
        .update(|state| {
            state
                .spacerefs
                .retain(|r| r.abspath != space_reference.abspath);

            if let Some(spaceref) = &state.spaceref {
                if spaceref.abspath == space_reference.abspath {
                    state.spaceref = None;
                }
            }
        })
        .map_err(|e| CmdErr {
            kind: ErrorKind::FileWriteError,
            message: "Unable to remove space".to_string(),
            details: Some(e.to_string()),
        })?;

    if state_store.spaceref.is_none() {
        if let Some(valid_space_reference) = space::first_valid_spaceref(&state_store) {
            state_store
                .update(|state| {
                    state.spaceref = Some(valid_space_reference.clone());
                })
                .map_err(|e| CmdErr {
                    kind: ErrorKind::FileWriteError,
                    message: "Unable to set fallback space".to_string(),
                    details: Some(e.to_string()),
                })?;
        }
    }

    Ok(())
}

#[specta::specta]
#[tauri::command]
pub fn get_spaceref(path: PathBuf) -> CmdResult<SpaceReference> {
    let space_abspath = path;

    match space::parse_spacecfg(&space_abspath) {
        Ok(space_config_file) => {
            let space_reference = SpaceReference {
                abspath: space_abspath,
                name: space_config_file.meta.name,
            };

            Ok(space_reference)
        }
        Err(err) => Err(CmdErr {
            kind: ErrorKind::ParseError,
            message: "Unable to parse space".to_string(),
            details: Some(err.to_string()),
        }),
    }
}

#[specta::specta]
#[tauri::command]
pub async fn get_space_cookies(
    space_abspath: PathBuf,
) -> CmdResult<HashMap<String, Vec<SerializedCookie>>> {
    let sck_store_abspath = store::utils::sck_store_abspath(&datadir_abspath(), &space_abspath);
    let sck_store = SpaceCookieStore::get(&sck_store_abspath).map_err(|e| CmdErr {
        kind: ErrorKind::CookieError,
        message: "Unable to load cookies".to_string(),
        details: Some(e.to_string()),
    })?;
    let sck_store_mtx = sck_store.cookies.lock().map_err(|e| {
        eprintln!("Failed to acquire cookie store lock: {e}");

        CmdErr {
            kind: ErrorKind::InternalError,
            message: "Unable to access cookie store :(".to_string(),
            details: Some(e.to_string()),
        }
    })?;

    let cookies: Vec<SerializedCookie> = sck_store_mtx
        .iter_any()
        .map(SerializedCookie::from_cookie_store)
        .collect();

    let cookies_by_domain: HashMap<String, Vec<SerializedCookie>> = cookies.into_iter().fold(
        HashMap::new(),
        |mut acc: HashMap<String, Vec<SerializedCookie>>, ck| {
            acc.entry(ck.domain.clone()).or_default().push(ck);
            acc
        },
    );

    Ok(cookies_by_domain)
}

#[specta::specta]
#[tauri::command]
pub fn remove_cookie(space_abspath: PathBuf, rm_cookie_dto: RemoveCookieDto) -> CmdResult<bool> {
    let RemoveCookieDto { domain, path, name } = rm_cookie_dto;

    let sck_store_abspath = store::utils::sck_store_abspath(&datadir_abspath(), &space_abspath);
    let mut sck_store = SpaceCookieStore::get(&sck_store_abspath).map_err(|e| CmdErr {
        kind: ErrorKind::CookieError,
        message: "Unable to load cookies".to_string(),
        details: Some(e.to_string()),
    })?;

    sck_store
        .update(|cookies| {
            let mut locked = cookies.lock().unwrap();
            locked.remove(&domain, &path, &name);
        })
        .map(|_| true)
        .map_err(|e| CmdErr {
            kind: ErrorKind::CookieError,
            message: "Unable to remove cookie".to_string(),
            details: Some(e.to_string()),
        })
}

#[specta::specta]
#[tauri::command]
pub async fn save_space_settings(
    space_abspath: PathBuf,
    space_settings: SpaceSettings,
) -> CmdResult<()> {
    let sst_store_abspath = store::utils::sst_store_abspath(&datadir_abspath(), &space_abspath);
    let mut sst_store = SpaceSettingsStore::get(&sst_store_abspath).map_err(|err| CmdErr {
        kind: ErrorKind::FileReadError,
        message: "Unable to load space settings".to_string(),
        details: Some(err.to_string()),
    })?;

    sst_store
        .update(|cur_settings| {
            *cur_settings = space_settings;
        })
        .map_err(|err| CmdErr {
            kind: ErrorKind::FileWriteError,
            message: "Unable to save space settings".to_string(),
            details: Some(err.to_string()),
        })?;

    Ok(())
}

#[specta::specta]
#[tauri::command]
pub fn show_main_window(window: tauri::Window) -> CmdResult<()> {
    window.get_webview_window("main").unwrap().show().unwrap();

    Ok(())
}

#[specta::specta]
#[tauri::command]
pub async fn move_tree_node(dto: MoveTreeNodeDto) -> CmdResult<()> {
    let state_store_abspath = store::utils::state_store_abspath(&datadir_abspath());
    let state_store = StateStore::get(&state_store_abspath).map_err(|err| CmdErr {
        kind: ErrorKind::FileReadError,
        message: "Unable to load state store".to_string(),
        details: Some(err.to_string()),
    })?;

    let space_abspath = state_store
        .spaceref
        .as_ref()
        .ok_or(CmdErr {
            kind: ErrorKind::SpaceNotFoundError,
            message: "No current space selected".to_string(),
            details: None,
        })?
        .abspath
        .clone();

    tree_node::move_tree_node(&dto, &space_abspath, &state_store).map_err(|err| CmdErr {
        kind: ErrorKind::FileWriteError,
        message: format!("Unable to move the {}", dto.node_type),
        details: Some(err.to_string()),
    })
}
