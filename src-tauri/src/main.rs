#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;

pub mod commands;
pub mod constants;
pub mod core;
pub mod models;
pub mod platform;

use core::{shortcuts, state};
use models::zaku::ZakuState;
use tauri_specta::{collect_commands, Builder};

fn main() {
    #[cfg(target_os = "linux")]
    platform::linux::initialize();

    let builder = Builder::<tauri::Wry>::new().commands(collect_commands![
        commands::state::get_zaku_state,
        commands::space::create_space,
        commands::space::set_active_space,
        commands::space::delete_space,
        commands::space::get_space_reference,
        commands::window::show_main_window,
        commands::dialog::open_dir_dialog,
        commands::notification::is_notification_permission_granted,
        commands::notification::request_notification_permission,
        commands::notification::dispatch_notification,
        commands::collection::create_collection,
        commands::request::create_request,
        commands::request::save_request_to_buffer,
        commands::request::write_buffer_request_to_fs,
        commands::request::http_req,
        commands::move_tree_item,
    ]);

    if std::env::var("GEN_BINDINGS").is_ok() {
        use specta_typescript::Typescript;
        use std::process::Command;

        builder
            .export(Typescript::default(), "./../src/lib/bindings.ts")
            .expect("Failed to export typescript bindings");

        Command::new("pnpm")
            .arg("format")
            .current_dir("./../src")
            .status()
            .expect("Failed to execute pnpm format");
    }

    let app = tauri::Builder::default()
        .manage(Mutex::new(ZakuState {
            active_space: None,
            space_references: Vec::new(),
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            state::initialize(app);
            shortcuts::initialize(app);

            #[cfg(target_os = "macos")]
            {
                use tauri::Manager;

                let webview_window = app.get_webview_window("main").unwrap();
                platform::macos::initialize(&webview_window);
            }

            return Ok(());
        })
        .invoke_handler(builder.invoke_handler());

    app.run(tauri::generate_context!())
        .expect("Error while running the application");
}
