#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;

pub mod commands;
pub mod constants;
pub mod core;
pub mod models;
pub mod platform;
pub mod utils;

use core::{shortcuts, state};
use models::zaku::ZakuState;
use tauri::Manager;

fn main() {
    #[cfg(target_os = "linux")]
    platform::linux::initialize();

    #[cfg(debug_assertions)]
    models::generate_bindings().expect("Failed to generate TypeScript bindings");

    let app = tauri::Builder::default()
        .manage(Mutex::new(ZakuState {
            active_space: None,
            space_references: Vec::new(),
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            state::initialize(app);
            shortcuts::initialize(app);

            #[cfg(target_os = "macos")]
            {
                let webview_window = app.get_webview_window("main").unwrap();
                platform::macos::initialize(&webview_window);
            }

            return Ok(());
        })
        .invoke_handler(tauri::generate_handler![
            commands::state::get_zaku_state,
            commands::space::create_space,
            commands::space::set_active_space,
            commands::space::delete_space,
            commands::space::get_space_reference,
            commands::window::show_main_window,
            commands::dialog::open_directory_dialog,
            commands::notification::is_notification_permission_granted,
            commands::notification::request_notification_permission,
            commands::notification::dispatch_notification,
            commands::collection::create_collection,
            commands::request::create_request,
            commands::move_tree_item,
        ]);

    app.run(tauri::generate_context!())
        .expect("Error while running the application");
}
