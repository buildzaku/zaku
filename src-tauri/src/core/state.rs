use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{App, Manager};
use tauri_plugin_store::StoreCollection;

use super::space;
use crate::constants::ZakuStoreKey;
use crate::types::AppState;

pub fn initialize(app: &mut App) {
    let stores = app.state::<StoreCollection<tauri::Wry>>();
    let app_data_dir = app.path().app_data_dir().unwrap();

    let active_space_path: Option<PathBuf> = tauri_plugin_store::with_store(
        app.handle().clone(),
        stores.clone(),
        app_data_dir.clone(),
        |store| match store.get(ZakuStoreKey::ActiveSpacePath.to_string()) {
            Some(value) if value.is_string() => {
                let path_string = value.as_str().unwrap();

                Ok(Some(PathBuf::from(path_string)))
            }
            _ => Ok(None),
        },
    )
    .unwrap();

    match active_space_path {
        Some(path) => {
            match space::parse_space(&path) {
                Ok(active_space) => {
                    let state = app.app_handle().state::<Mutex<AppState>>();

                    *state.lock().unwrap() = AppState {
                        active_space: Some(active_space),
                    };
                }
                Err(_) => (),
            };
        }
        None => (),
    }
}
