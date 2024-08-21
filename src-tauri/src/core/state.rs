use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{App, Manager};
use tauri_plugin_store::StoreCollection;

use super::{space, store};
use crate::{constants::ZakuStoreKey, types::AppState};

pub fn initialize(app: &mut App) {
    let stores = app.state::<StoreCollection<tauri::Wry>>();
    let app_data_dir = app.path().app_data_dir().unwrap();

    let active_space = store::get_active_space(app.handle().clone(), stores.clone());
    let saved_spaces = store::get_saved_spaces(app.handle().clone(), stores.clone());

    let state = app.app_handle().state::<Mutex<AppState>>();
    let mut app_state = state.lock().unwrap();

    if let Some(active_space_reference) = active_space {
        match space::parse_space(&PathBuf::from(&active_space_reference.path)) {
            Ok(active_space) => {
                app_state.active_space = Some(active_space);
            }
            Err(_) => {
                for space_reference in &saved_spaces {
                    if let Ok(valid_space) =
                        space::parse_space(&PathBuf::from(&space_reference.path))
                    {
                        app_state.active_space = Some(valid_space);

                        tauri_plugin_store::with_store(
                            app.handle().clone(),
                            stores.clone(),
                            app_data_dir.clone(),
                            |store| {
                                store
                                    .insert(
                                        ZakuStoreKey::ActiveSpace.to_string(),
                                        serde_json::json!({
                                            "path": space_reference.path,
                                            "name": space_reference.name,
                                        }),
                                    )
                                    .map_err(|err| err.to_string())
                                    .unwrap();
                                store.save().unwrap();

                                return Ok(());
                            },
                        )
                        .unwrap();

                        break;
                    }
                }
            }
        }
    } else {
        for space_reference in &saved_spaces {
            if let Ok(valid_space) = space::parse_space(&PathBuf::from(&space_reference.path)) {
                app_state.active_space = Some(valid_space);

                tauri_plugin_store::with_store(
                    app.handle().clone(),
                    stores.clone(),
                    app_data_dir.clone(),
                    |store| {
                        store
                            .insert(
                                ZakuStoreKey::ActiveSpace.to_string(),
                                serde_json::json!({
                                    "path": space_reference.path,
                                    "name": space_reference.name,
                                }),
                            )
                            .map_err(|err| err.to_string())
                            .unwrap();
                        store.save().unwrap();

                        return Ok(());
                    },
                )
                .unwrap();

                break;
            }
        }
    }

    app_state.saved_spaces = saved_spaces;

    return ();
}
