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

    tauri_plugin_store::with_store(app_handle.clone(), stores.clone(), app_data_dir, |store| {
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

pub fn delete_space_reference(
    space_reference: SpaceReference,
    app_handle: AppHandle,
    stores: State<'_, StoreCollection<Wry>>,
) {
    let app_data_dir = app_handle.path().app_data_dir().unwrap();

    if let Some(active_space_reference) = get_active_space(app_handle.clone(), stores.clone()) {
        if active_space_reference.path == space_reference.path {
            tauri_plugin_store::with_store(
                app_handle.clone(),
                stores.clone(),
                app_data_dir,
                |store| {
                    store.delete(ZakuStoreKey::ActiveSpace.to_string()).unwrap();
                    store.save().unwrap();

                    return Ok(());
                },
            )
            .unwrap()
        }
    }

    let updated_space_references = {
        let mut space_references = get_space_references(app_handle.clone(), stores.clone());
        space_references.retain(|existing_space_reference| {
            existing_space_reference.path != space_reference.path
        });

        space_references
    };

    set_space_references(updated_space_references, app_handle.clone(), stores.clone());

    return ();
}

pub fn get_space_references(
    app_handle: AppHandle,
    stores: State<'_, StoreCollection<Wry>>,
) -> Vec<SpaceReference> {
    let app_data_dir = app_handle.path().app_data_dir().unwrap();

    return tauri_plugin_store::with_store(app_handle, stores, app_data_dir, |store| {
        let empty_array = serde_json::json!([]);

        return Ok(store
            .get(ZakuStoreKey::SpaceReferences.to_string())
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

pub fn set_space_references(
    space_references: Vec<SpaceReference>,
    app_handle: AppHandle,
    stores: State<'_, StoreCollection<Wry>>,
) {
    let app_data_dir = app_handle.path().app_data_dir().unwrap();

    return tauri_plugin_store::with_store(app_handle, stores, app_data_dir, |store| {
        store
            .insert(
                ZakuStoreKey::SpaceReferences.to_string(),
                serde_json::json!(space_references),
            )
            .unwrap();

        store.save().unwrap();

        return Ok(());
    })
    .unwrap();
}

pub fn update_space_references_if_needed(
    space_reference: SpaceReference,
    app_handle: AppHandle,
    stores: State<'_, StoreCollection<Wry>>,
) {
    let mut space_references = get_space_references(app_handle.clone(), stores.clone());
    let exists_in_space_references = space_references.iter().any(|existing_space_reference| {
        existing_space_reference.path == space_reference.path.clone()
    });

    if !exists_in_space_references {
        println!(
            "not inside space references, pushing now {:?}",
            space_reference
        );

        space_references.push(SpaceReference {
            path: space_reference.path.clone(),
            name: space_reference.name.clone(),
        });

        set_space_references(space_references, app_handle, stores);
    }

    return ();
}
