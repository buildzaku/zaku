#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod constants;
pub mod core;
pub mod types;

use core::{commands, shortcuts};

fn main() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .setup(|app| {
            shortcuts::initialize(app);

            return Ok(());
        })
        .invoke_handler(tauri::generate_handler![
            commands::space::create_space,
            commands::space::get_active_space,
            commands::space::set_active_space,
            commands::space::delete_space,
            commands::space::get_space_reference,
            commands::space::get_saved_spaces,
            commands::window::show_main_window,
            commands::dialog::open_directory_dialog,
            commands::notification::is_notification_permission_granted,
            commands::notification::request_notification_permission,
            commands::notification::dispatch_notification
        ]);

    app.run(tauri::generate_context!())
        .expect("error while running the application");
}
