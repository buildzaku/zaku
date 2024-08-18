#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;
use tauri_plugin_store::StoreCollection;

pub mod constants;
pub mod core;
pub mod types;

use constants::ZakuStoreKey;

use core::{commands, space};
use types::AppState;

fn main() {
    let app = tauri::Builder::default()
        .manage(Mutex::new(AppState { active_space: None }))
        .plugin(tauri_plugin_store::Builder::default().build())
        .setup(|app| {
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

            return Ok(());
        })
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::space::create_space,
            commands::space::get_active_space,
            commands::space::set_active_space,
            commands::space::delete_active_space,
            commands::window::show_main_window
        ]);

    app.run(tauri::generate_context!())
        .expect("error while running the application");
}
