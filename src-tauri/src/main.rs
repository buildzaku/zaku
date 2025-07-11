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

use crate::state::ZakuState;

const BINDINGS_PATH: &str = "./../src/lib/bindings.ts";

fn main() {
    #[cfg(target_os = "linux")]
    platform::linux::initialize().expect("Failed to initialize Linux platform");

    let builder = tauri_specta::Builder::<tauri::Wry>::new()
        .commands(commands::collect())
        .error_handling(tauri_specta::ErrorHandlingMode::Result);

    if std::env::var("GEN_BINDINGS").is_ok() {
        use specta_typescript::{formatter, Typescript};

        builder
            .export(
                Typescript::default().formatter(formatter::prettier),
                BINDINGS_PATH,
            )
            .expect("Failed to export typescript bindings");
    }

    let app = tauri::Builder::default()
        .manage(Mutex::new(ZakuState {
            active_space: None,
            spacerefs: Vec::new(),
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            state::initialize(app)?;
            shortcuts::initialize(app)?;

            Ok(())
        })
        .invoke_handler(builder.invoke_handler());

    app.run(tauri::generate_context!())
        .expect("Error while running the application");
}
