use tauri::{AppHandle, Manager, State, Wry};
use tauri_plugin_store::StoreCollection;

use crate::{constants::ZakuStoreKey, types::SpaceReference};

pub fn get_active_space(
    app_handle: AppHandle,
    stores: State<'_, StoreCollection<Wry>>,
) -> Option<SpaceReference> {
    let app_data_dir = app_handle.path().app_data_dir().unwrap();

    return tauri_plugin_store::with_store(app_handle, stores, app_data_dir, |store| {
        return Ok(store
            .get(ZakuStoreKey::ActiveSpace.to_string())
            .and_then(|value| {
                value.as_object().and_then(|object| {
                    let path = object.get("path").unwrap().as_str().unwrap().to_string();
                    let name = object.get("name").unwrap().as_str().unwrap().to_string();

                    return Some(SpaceReference { path, name });
                })
            }));
    })
    .unwrap_or(None);
}

pub fn set_active_space(
    space_reference: SpaceReference,
    app_handle: AppHandle,
    stores: State<'_, StoreCollection<Wry>>,
) {
    let app_data_dir = app_handle.path().app_data_dir().unwrap();

    tauri_plugin_store::with_store(app_handle, stores, app_data_dir, |store| {
        store
            .insert(
                ZakuStoreKey::ActiveSpace.to_string(),
                serde_json::json!({
                    "path": space_reference.path,
                    "name": space_reference.name,
                }),
            )
            .unwrap();

        store.save().unwrap();

        return Ok(());
    })
    .unwrap();
}

pub fn delete_active_space(app_handle: AppHandle, stores: State<'_, StoreCollection<Wry>>) {
    let app_data_dir = app_handle.path().app_data_dir().unwrap();

    return tauri_plugin_store::with_store(app_handle, stores, app_data_dir, |store| {
        store.delete(ZakuStoreKey::ActiveSpace.to_string()).unwrap();
        store.save().unwrap();

        return Ok(());
    })
    .unwrap();
}

pub fn get_saved_spaces(
    app_handle: AppHandle,
    stores: State<'_, StoreCollection<Wry>>,
) -> Vec<SpaceReference> {
    let app_data_dir = app_handle.path().app_data_dir().unwrap();

    return tauri_plugin_store::with_store(app_handle, stores, app_data_dir, |store| {
        let empty_array = serde_json::json!([]);

        return Ok(store
            .get(ZakuStoreKey::SavedSpaces.to_string())
            .unwrap_or_else(|| &empty_array)
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|value| {
                value.as_object().and_then(|object| {
                    return Some(SpaceReference {
                        path: object.get("path").unwrap().as_str().unwrap().to_string(),
                        name: object.get("name").unwrap().as_str().unwrap().to_string(),
                    });
                })
            })
            .collect());
    })
    .unwrap_or_else(|_| Vec::new());
}

pub fn set_saved_spaces(
    space_references: Vec<SpaceReference>,
    app_handle: AppHandle,
    stores: State<'_, StoreCollection<Wry>>,
) {
    let app_data_dir = app_handle.path().app_data_dir().unwrap();

    return tauri_plugin_store::with_store(app_handle, stores, app_data_dir, |store| {
        store
            .insert(
                ZakuStoreKey::SavedSpaces.to_string(),
                serde_json::json!(space_references),
            )
            .unwrap();

        store.save().unwrap();

        return Ok(());
    })
    .unwrap();
}
