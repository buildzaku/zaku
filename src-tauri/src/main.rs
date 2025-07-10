#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;

pub mod collection;
pub mod commands;
pub mod error;
pub mod notifications;
pub mod platform;
pub mod request;
pub mod shortcuts;
pub mod space;
pub mod state;
pub mod store;
pub mod utils;

use tauri_specta::Builder;

use crate::state::ZakuState;

fn main() {
    #[cfg(target_os = "linux")]
    platform::linux::initialize();

    let builder = Builder::<tauri::Wry>::new().commands(commands::collect());

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
            spacerefs: Vec::new(),
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
