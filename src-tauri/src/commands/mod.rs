use cookie::Cookie as RawCookie;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_notification::{NotificationExt, PermissionState};

use crate::{
    collection::{
        self,
        models::{CreateCollectionDto, CreateNewCollection},
    },
    commands::models::{CreateNewRequest, DispatchNotificationOptions, OpenDirDialogOpt},
    error::{CmdErr, CmdResult},
    notifications,
    request::{
        self,
        models::{CreateRequestDto, HttpReq, HttpRes},
    },
    space::{
        self,
        models::{CreateSpaceDto, RemoveCookieDto, SpaceCookie, SpaceReference},
    },
    state::SharedState,
    store::{
        self,
        models::{SpaceCookies, SpaceSettings},
        spaces::buffer,
    },
    tree_node::{self, MoveTreeNodeDto},
};

pub mod models;

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
        persist_to_reqbuf,
        write_reqbuf_to_reqtoml,
        http_req,
        move_tree_node
    ]
}

#[specta::specta]
#[tauri::command]
pub async fn create_collection(
    dto: CreateCollectionDto,
    app_handle: tauri::AppHandle,
) -> CmdResult<CreateNewCollection> {
    let sharedstate_mtx = app_handle.state::<Mutex<SharedState>>();
    let mut sharedstate = sharedstate_mtx.lock().map_err(|e| CmdErr::Err {
        message: format!("State lock failed: {e}"),
    })?;

    let (parent_relpath, col_segment) = collection::create_parent_collections_if_missing(
        &dto.location_relpath,
        &dto.relpath,
        &mut sharedstate,
    )
    .map_err(|err| CmdErr::Err {
        message: format!("Failed to create parent collections: {err}"),
    })?;

    collection::create_collection(&parent_relpath, &col_segment, &mut sharedstate).map_err(|err| {
        CmdErr::Err {
            message: format!("Failed to create collection: {err}"),
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
        Some(path) => Ok(Some(
            path.into_path().unwrap().to_string_lossy().to_string(),
        )),
        None => Ok(None),
    }
}

#[specta::specta]
#[tauri::command]
pub fn is_notif_enabled(app_handle: tauri::AppHandle) -> CmdResult<bool> {
    let permission_state = app_handle
        .notification()
        .permission_state()
        .map_err(|err| CmdErr::Err {
            message: format!("Failed to get current permissions state: {err}"),
        })?;

    Ok(permission_state == PermissionState::Granted)
}

#[specta::specta]
#[tauri::command]
pub fn request_notif_access(app_handle: tauri::AppHandle) -> CmdResult<bool> {
    let permission_state = app_handle
        .notification()
        .request_permission()
        .map_err(|err| CmdErr::Err {
            message: format!("Failed to request for permissions: {err}"),
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
        .map_err(|err| CmdErr::Err {
            message: format!("Failed to dispatch notification: {err}"),
        })?;

    Ok(())
}

#[specta::specta]
#[tauri::command]
pub async fn create_req(
    dto: CreateRequestDto,
    app_handle: tauri::AppHandle,
) -> CmdResult<CreateNewRequest> {
    let sharedstate_mtx = app_handle.state::<Mutex<SharedState>>();
    let mut sharedstate = sharedstate_mtx.lock().map_err(|e| CmdErr::Err {
        message: format!("State lock failed: {e}"),
    })?;

    let (parent_relpath, req_segment) = collection::create_parent_collections_if_missing(
        &dto.location_relpath,
        &dto.relpath,
        &mut sharedstate,
    )
    .map_err(|err| CmdErr::Err {
        message: format!("Failed to create parent collections: {err}"),
    })?;

    request::create_req(&parent_relpath, &req_segment, &mut sharedstate).map_err(|err| {
        CmdErr::Err {
            message: format!("Failed to create request: {err}"),
        }
    })
}

#[specta::specta]
#[tauri::command]
pub async fn persist_to_reqbuf(
    space_abspath: &str,
    relpath: &str,
    request: HttpReq,
) -> CmdResult<()> {
    let abs = Path::new(space_abspath);
    let rel = Path::new(relpath);

    buffer::persist_req_to_spacebuf(abs, rel, request).map_err(|e| CmdErr::Err {
        message: e.to_string(),
    })?;

    Ok(())
}

#[specta::specta]
#[tauri::command]
pub async fn write_reqbuf_to_reqtoml(space_abspath: &str, req_relpath: &str) -> CmdResult<()> {
    let abs = Path::new(space_abspath);
    let rel = Path::new(req_relpath);
    buffer::write_reqbuf_to_reqtoml(abs, rel).unwrap();

    Ok(())
}

#[specta::specta]
#[tauri::command]
pub async fn http_req(req: HttpReq, app_handle: tauri::AppHandle) -> CmdResult<HttpRes> {
    let spaceref = store::get_spaceref().ok_or(CmdErr::Err {
        message: "No space found".into(),
    })?;
    let space_abspath = spaceref.path.as_str();
    let cookie_store = SpaceCookies::load(space_abspath).map_err(|e| CmdErr::Err {
        message: e.to_string(),
    })?;
    let space_settings = SpaceSettings::load(space_abspath).map_err(|e| CmdErr::Err {
        message: e.to_string(),
    })?;
    let client = reqwest::Client::builder()
        .cookie_provider(Arc::clone(&cookie_store))
        .build()
        .map_err(|e| CmdErr::Err {
            message: e.to_string(),
        })?;
    let cfg = &req.config;
    let url = cfg.url.raw.clone().ok_or(CmdErr::Err {
        message: "Missing URL".into(),
    })?;
    let method = reqwest::Method::from_bytes(cfg.method.as_bytes()).map_err(|e| CmdErr::Err {
        message: e.to_string(),
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
    let resp = builder.send().await.map_err(|e| CmdErr::Err {
        message: e.to_string(),
    })?;
    if space_settings.notifications.audio.on_req_finish {
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
        .map(|ck| SpaceCookie::from_raw_cookie(&ck))
        .collect::<Vec<SpaceCookie>>();
    let data = resp.text().await.map_err(|e| CmdErr::Http {
        message: e.to_string(),
        code: Some(status),
    })?;
    let size_bytes = Some(data.len() as u32);
    SpaceCookies::persist(space_abspath).map_err(|e| CmdErr::Err {
        message: e.to_string(),
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
pub async fn create_space(
    create_space_dto: CreateSpaceDto,
    sharedstate_mtx: tauri::State<'_, Mutex<SharedState>>,
) -> CmdResult<SpaceReference> {
    let mut sharedstate = sharedstate_mtx.lock().map_err(|e| CmdErr::Err {
        message: format!("Failed to acquire lock: {e}"),
    })?;

    space::create_space(create_space_dto, &mut sharedstate).map_err(|e| CmdErr::Err {
        message: format!("Failed to create space: {e}"),
    })
}

#[specta::specta]
#[tauri::command]
pub fn set_space(
    space_reference: SpaceReference,
    sharedstate_mtx: tauri::State<Mutex<SharedState>>,
) -> CmdResult<()> {
    let mut sharedstate = sharedstate_mtx.lock().unwrap();
    let space_abspath = PathBuf::from(space_reference.path.as_str());

    if !space_abspath.exists() {
        return Err(CmdErr::Err {
            message: format!(
                "Directory does not exist: {}",
                space_abspath.to_string_lossy()
            ),
        });
    }

    match space::parse_space(&space_abspath) {
        Ok(space) => {
            store::set_spaceref(space_reference.clone()).map_err(|e| CmdErr::Err {
                message: e.to_string(),
            })?;
            store::insert_spaceref_if_missing(space_reference.clone()).map_err(|e| {
                CmdErr::Err {
                    message: e.to_string(),
                }
            })?;

            sharedstate.space = Some(space);
            sharedstate.spacerefs = store::get_spacerefs();

            Ok(())
        }
        Err(err) => Err(CmdErr::Err {
            message: format!("Unable to parse space: {err}"),
        }),
    }
}

#[specta::specta]
#[tauri::command]
pub fn remove_space(
    space_reference: SpaceReference,
    sharedstate_mtx: tauri::State<Mutex<SharedState>>,
) -> CmdResult<()> {
    let mut sharedstate = sharedstate_mtx.lock().unwrap();
    store::remove_spaceref(space_reference).map_err(|e| CmdErr::Err {
        message: e.to_string(),
    })?;

    let spaceref = store::get_spaceref();

    if spaceref.is_none() {
        sharedstate.space = None;

        if let Some(valid_space_reference) = space::first_valid_spaceref() {
            store::set_spaceref(valid_space_reference.clone()).map_err(|e| CmdErr::Err {
                message: e.to_string(),
            })?;

            if let Ok(space) = space::parse_space(&PathBuf::from(&valid_space_reference.path)) {
                sharedstate.space = Some(space);
            }
        }
    }

    sharedstate.spacerefs = store::get_spacerefs();

    Ok(())
}

#[specta::specta]
#[tauri::command]
pub fn get_spaceref(path: String) -> CmdResult<SpaceReference> {
    let space_abspath = PathBuf::from(path.as_str());

    match space::parse_spacecfg(&space_abspath) {
        Ok(space_config_file) => {
            let space_reference = SpaceReference {
                path: space_abspath.to_string_lossy().to_string(),
                name: space_config_file.meta.name,
            };

            Ok(space_reference)
        }
        Err(err) => Err(CmdErr::Err {
            message: format!("Unable to parse space: {err}"),
        }),
    }
}

#[specta::specta]
#[tauri::command]
pub async fn get_space_cookies(
    space_abspath: &str,
) -> CmdResult<HashMap<String, Vec<SpaceCookie>>> {
    let cookie_store = SpaceCookies::load(space_abspath).map_err(|e| CmdErr::Err {
        message: e.to_string(),
    })?;
    let store = cookie_store.lock().map_err(|_| CmdErr::Err {
        message: "Failed to lock the cookie store (CookieStoreLockFailed)".into(),
    })?;

    let cookies: Vec<SpaceCookie> = store
        .iter_any()
        .map(SpaceCookie::from_cookie_store)
        .collect();

    let cookies_by_domain: HashMap<String, Vec<SpaceCookie>> = cookies.into_iter().fold(
        HashMap::new(),
        |mut acc: HashMap<String, Vec<SpaceCookie>>, ck| {
            acc.entry(ck.domain.clone()).or_default().push(ck);
            acc
        },
    );

    Ok(cookies_by_domain)
}

#[specta::specta]
#[tauri::command]
pub fn remove_cookie(space_abspath: &str, rm_cookie_dto: RemoveCookieDto) -> CmdResult<bool> {
    SpaceCookies::remove(space_abspath, rm_cookie_dto)
        .map(|opt| opt.is_some())
        .map_err(|e| CmdErr::Err {
            message: e.to_string(),
        })
}

#[specta::specta]
#[tauri::command]
pub async fn save_space_settings(space_abspath: &str, settings: SpaceSettings) -> CmdResult<()> {
    SpaceSettings::persist(space_abspath, &settings).map_err(|err| CmdErr::Err {
        message: format!("Failed to persist space settings: {err}"),
    })?;

    Ok(())
}

#[specta::specta]
#[tauri::command]
pub fn get_shared_state(
    sharedstate_mtx: tauri::State<Mutex<SharedState>>,
) -> CmdResult<SharedState> {
    match sharedstate_mtx.lock() {
        Ok(sharedstate) => Ok(sharedstate.clone()),
        Err(e) => Err(CmdErr::Err {
            message: format!("State lock error: {e}"),
        }),
    }
}

#[specta::specta]
#[tauri::command]
pub fn show_main_window(window: tauri::Window) -> CmdResult<()> {
    window.get_webview_window("main").unwrap().show().unwrap();

    Ok(())
}

#[specta::specta]
#[tauri::command]
pub async fn move_tree_node(dto: MoveTreeNodeDto, app_handle: tauri::AppHandle) -> CmdResult<()> {
    let sharedstate_mtx = app_handle.state::<Mutex<SharedState>>();
    let mut sharedstate = sharedstate_mtx.lock().map_err(|e| CmdErr::Err {
        message: format!("State lock failed: {e}"),
    })?;

    tree_node::move_tree_node(&dto, &mut sharedstate).map_err(|err| CmdErr::Err {
        message: format!("Failed to handle drop: {err}"),
    })
}
