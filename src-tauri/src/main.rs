#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use specta_typescript::{Typescript, formatter};
use std::path::PathBuf;

pub mod collection;
pub mod commands;
pub mod error;
pub mod models;
pub mod notifications;
pub mod platform;
pub mod request;
pub mod shortcuts;
pub mod space;
pub mod store;
pub mod tree_node;
pub mod utils;

fn ts_bindings_path() -> PathBuf {
    PathBuf::from("..")
        .join("src")
        .join("lib")
        .join("bindings.ts")
}

fn main() {
    #[cfg(target_os = "linux")]
    platform::linux::initialize().expect("Failed to initialize linux platform");

    let builder = tauri_specta::Builder::<tauri::Wry>::new()
        .commands(commands::collect())
        .error_handling(tauri_specta::ErrorHandlingMode::Result);

    #[cfg(debug_assertions)]
    builder
        .export(
            Typescript::default().formatter(formatter::prettier),
            ts_bindings_path(),
        )
        .expect("Failed to export typescript bindings");

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            shortcuts::initialize(app)?;

            Ok(())
        })
        .invoke_handler(builder.invoke_handler());

    app.run(tauri::generate_context!())
        .expect("Error while running the application");
}
