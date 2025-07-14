use cookie::Cookie as RawCookie;
use std::{
    collections::HashMap,
    fs::{self, File},
    io::Write,
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
    commands::models::{
        CreateNewRequest, DispatchNotificationOptions, MoveTreeItemDto, OpenDirDialogOpt,
    },
    error::{CmdErr, CmdResult},
    notifications,
    request::{
        self,
        models::{CreateRequestDto, HttpReq, HttpRes},
    },
    space::{
        self,
        models::{
            CreateSpaceDto, RemoveCookieDto, SpaceConfigFile, SpaceCookie, SpaceMeta,
            SpaceReference,
        },
    },
    state::SharedState,
    store::{
        self,
        models::{SpaceCookies, SpaceSettings},
        spaces::buffer,
    },
    utils,
};

pub mod models;

pub fn collect() -> tauri_specta::Commands<tauri::Wry> {
    tauri_specta::collect_commands![
        get_shared_state,
        create_space,
        set_active_space,
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
        move_treeitem,
    ]
}

#[specta::specta]
#[tauri::command]
pub async fn create_collection(
    create_collection_dto: CreateCollectionDto,
    app_handle: tauri::AppHandle,
) -> CmdResult<CreateNewCollection> {
    let sharedstate_mtx = app_handle.state::<Mutex<SharedState>>();
    let mut sharedstate = sharedstate_mtx.lock().map_err(|e| CmdErr::Err {
        message: format!("State lock failed: {e}"),
    })?;

    collection::create_collection(&create_collection_dto, &mut sharedstate).map_err(|err| {
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
pub fn move_treeitem(
    move_treeitem_dto: MoveTreeItemDto,
    app_handle: tauri::AppHandle,
) -> CmdResult<()> {
    let sharedstate_mtx = app_handle.state::<Mutex<SharedState>>();
    let mut sharedstate = sharedstate_mtx.lock().unwrap();
    let active_space = sharedstate
        .active_space
        .clone()
        .expect("Active space not found");
    let active_space_abspath = PathBuf::from(&active_space.abspath);
    let MoveTreeItemDto {
        src_relpath,
        dest_relpath,
    } = move_treeitem_dto;
    let src_abspath = active_space_abspath.join(src_relpath);
    let dest_abspath = active_space_abspath.join(dest_relpath);

    fs::rename(src_abspath, dest_abspath).expect("Unable to move tree item");

    match space::parse_space(&active_space_abspath) {
        Ok(active_space) => sharedstate.active_space = Some(active_space),
        Err(err) => {
            return Err(CmdErr::Err {
                message: format!("Failed to parse space after moving the tree item: {err}"),
            });
        }
    }

    Ok(())
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
    create_req_dto: CreateRequestDto,
    app_handle: tauri::AppHandle,
) -> CmdResult<CreateNewRequest> {
    if create_req_dto.relpath.is_empty() {
        return Err(CmdErr::Err {
            message: "Cannot create a request without name".to_string(),
        });
    }

    let sharedstate_mtx = app_handle.state::<Mutex<SharedState>>();
    let mut sharedstate = sharedstate_mtx.lock().unwrap();
    let active_space = sharedstate
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
                    .map_err(|err| CmdErr::Err {
                    message: format!("Failed to create request's parent directories: {err}"),
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
        format!("{file_sanitized_name}.toml").as_str(),
    ]);

    request::create_reqtoml(&file_abspath, file_display_name).map_err(|err| CmdErr::Err {
        message: format!("Failed to create request file: {err}"),
    })?;

    let create_new_result = CreateNewRequest {
        parent_relpath: file_parent_relpath,
        relpath: file_relpath,
    };

    match space::parse_space(&active_space_abspath) {
        Ok(active_space) => sharedstate.active_space = Some(active_space),
        Err(err) => {
            return Err(CmdErr::Err {
                message: format!("Failed to parse space after creating the request: {err}"),
            });
        }
    }

    Ok(create_new_result)
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
    let active_space = store::get_active_spaceref().ok_or(CmdErr::Err {
        message: "No active space".into(),
    })?;
    let space_abspath = active_space.path.as_str();
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
    let location = PathBuf::from(create_space_dto.location.as_str());
    if !location.exists() {
        return Err(CmdErr::Err {
            message: format!("Location does not exist: {}", create_space_dto.location),
        });
    }

    let space_abspath = location.join(create_space_dto.name.clone());
    let mut spacerefs = store::get_spacerefs();
    let mut sharedstate = sharedstate_mtx.lock().unwrap();

    if spacerefs
        .iter()
        .any(|sr| sr.path == space_abspath.to_string_lossy())
    {
        return Err(CmdErr::Err {
            message: format!(
                "Space already exists in saved spaces: {}",
                space_abspath.to_string_lossy()
            ),
        });
    }
    if space_abspath.exists() {
        return Err(CmdErr::Err {
            message: format!(
                "Directory with this name already exists: {}",
                space_abspath.to_string_lossy()
            ),
        });
    }

    fs::create_dir(&space_abspath).expect("Failed to create space directory");

    let space_config_dir = space_abspath.join(".zaku");
    fs::create_dir(&space_config_dir).expect("Failed to create `.zaku` directory");

    let mut space_config_file =
        File::create(space_config_dir.join("config").with_extension("toml"))
            .expect("Failed to create `config.toml`");

    let space_config = SpaceConfigFile {
        meta: SpaceMeta {
            name: create_space_dto.name.clone(),
        },
    };

    space_config_file
        .write_all(
            toml::to_string_pretty(&space_config)
                .expect("Failed to serialize space config")
                .as_bytes(),
        )
        .expect("Failed to write to config file");

    let spaceref = SpaceReference {
        path: space_abspath.to_string_lossy().to_string(),
        name: create_space_dto.name,
    };

    store::set_active_spaceref(spaceref.clone()).map_err(|e| CmdErr::Err {
        message: e.to_string(),
    })?;
    spacerefs.push(spaceref.clone());
    store::set_spacerefs(spacerefs.clone()).map_err(|e| CmdErr::Err {
        message: e.to_string(),
    })?;

    match space::parse_space(&PathBuf::from(spaceref.clone().path)) {
        Ok(active_space) => {
            sharedstate.active_space = Some(active_space);
            sharedstate.spacerefs = spacerefs;
        }
        Err(_) => {
            // TODO - handle
        }
    }

    Ok(spaceref)
}

#[specta::specta]
#[tauri::command]
pub fn set_active_space(
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
            store::set_active_spaceref(space_reference.clone()).map_err(|e| CmdErr::Err {
                message: e.to_string(),
            })?;
            store::insert_spaceref_if_missing(space_reference.clone()).map_err(|e| {
                CmdErr::Err {
                    message: e.to_string(),
                }
            })?;

            sharedstate.active_space = Some(space);
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

    let active_space = store::get_active_spaceref();

    if active_space.is_none() {
        sharedstate.active_space = None;

        if let Some(valid_space_reference) = space::first_valid_spaceref() {
            store::set_active_spaceref(valid_space_reference.clone()).map_err(|e| CmdErr::Err {
                message: e.to_string(),
            })?;

            if let Ok(active_space) =
                space::parse_space(&PathBuf::from(&valid_space_reference.path))
            {
                sharedstate.active_space = Some(active_space);
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
