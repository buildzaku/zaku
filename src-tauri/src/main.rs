#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;

pub mod constants;
pub mod core;
pub mod types;

use core::{commands, shortcuts, state};
use types::AppState;

fn main() {
    let app = tauri::Builder::default()
        .manage(Mutex::new(AppState { active_space: None }))
        .plugin(tauri_plugin_store::Builder::default().build())
        .setup(|app| {
            state::initialize(app);
            shortcuts::initialize(app);

            return Ok(());
        })
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::space::create_space,
            commands::space::get_active_space,
            commands::space::set_active_space,
            commands::space::delete_active_space,
            commands::window::show_main_window,
            commands::dialog::open_directory_dialog
        ]);

    app.run(tauri::generate_context!())
        .expect("error while running the application");
}
