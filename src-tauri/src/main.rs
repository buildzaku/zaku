#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod types;
pub mod workspace;

use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;
use tauri_plugin_store::StoreCollection;
use types::AppState;

fn main() {
    let app = tauri::Builder::default()
        .manage(Mutex::new(AppState {
            active_workspace: None,
        }))
        .plugin(tauri_plugin_store::Builder::default().build())
        .setup(|app| {
            let stores = app.state::<StoreCollection<tauri::Wry>>();
            let app_data_dir = app.path().app_data_dir().unwrap();

            let active_workspace_path: Option<PathBuf> = tauri_plugin_store::with_store(
                app.handle().clone(),
                stores.clone(),
                app_data_dir.clone(),
                |store| match store.get("active_workspace_path") {
                    Some(value) if value.is_string() => {
                        let path_string = value.as_str().unwrap();

                        Ok(Some(PathBuf::from(path_string)))
                    }
                    _ => Ok(None),
                },
            )
            .unwrap();

            match active_workspace_path {
                Some(path) => {
                    // Proceed with loading the workspace if active_workspace_path exists
                    match workspace::parse_workspace(&path) {
                        Ok(active_workspace) => {
                            let state = app.app_handle().state::<Mutex<AppState>>();

                            *state.lock().unwrap() = AppState {
                                active_workspace: Some(active_workspace),
                            };
                        }
                        Err(e) => {
                            eprintln!("Failed to parse workspace: {}", e);
                        }
                    };
                }
                None => {
                    eprintln!("PATH NOT FOUND!!");
                }
            }

            println!("returnrnrn");

            return Ok(());
        })
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            workspace::get_active_workspace,
            workspace::set_active_workspace,
        ]);

    app.run(tauri::generate_context!())
        .expect("error while running the application");
}
