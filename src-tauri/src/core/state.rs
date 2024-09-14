use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{App, Manager};
use tauri_plugin_store::StoreCollection;

use super::{space, store};
use crate::types::ZakuState;

pub fn initialize(app: &mut App) {
    let app_handle = app.handle();
    let stores = app.state::<StoreCollection<tauri::Wry>>();

    let active_space_reference =
        store::get_active_space_reference(app_handle.clone(), stores.clone()).or_else(|| {
            space::find_first_valid_space_reference(app_handle.clone(), stores.clone())
        });
    let space_references = store::get_space_references(app_handle.clone(), stores.clone());
    let state = app.app_handle().state::<Mutex<ZakuState>>();
    let mut zaku_state = state.lock().unwrap();

    if let Some(active_space_reference) = active_space_reference {
        let active_space_path = PathBuf::from(active_space_reference.path);

        match space::parse_space(&active_space_path) {
            Ok(active_space) => {
                zaku_state.active_space = Some(active_space);
            }
            Err(_) => {
                match space::find_first_valid_space_reference(app_handle.clone(), stores.clone()) {
                    Some(valid_space_reference) => {
                        store::set_active_space_reference(
                            valid_space_reference.clone(),
                            app_handle.clone(),
                            stores,
                        );

                        let valid_space_path = PathBuf::from(valid_space_reference.path);
                        let valid_space = space::parse_space(&valid_space_path).unwrap();
                        zaku_state.active_space = Some(valid_space);
                    }
                    None => {}
                }
            }
        };
    }

    zaku_state.space_references = space_references;

    return ();
}
