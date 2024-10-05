use std::{fs, path::PathBuf, sync::Mutex};

use tauri::{AppHandle, Manager};

use crate::{
    core::{collection, space},
    models::{
        collection::CreateCollectionDto,
        zaku::{ZakuError, ZakuState},
    },
};

#[tauri::command(rename_all = "snake_case")]
pub fn create_collection(
    create_collection_dto: CreateCollectionDto,
    app_handle: AppHandle,
) -> Result<(), ZakuError> {
    let state = app_handle.state::<Mutex<ZakuState>>();
    let mut zaku_state = state.lock().unwrap();
    let active_space = zaku_state.active_space.clone().unwrap();
    let active_space_absolute_path = PathBuf::from(&active_space.absolute_path);
    // let collection_parent_absolute_path =
    //     active_space_absolute_path.join(create_collection_dto.parent_relative_path.clone());

    // println!("INITIAL -> {}", create_collection_dto.relative_path.clone());

    // let mut collection_information = Vec::new();
    // for display_name in create_collection_dto.relative_path.split('/') {
    //     let display_name = display_name.trim();
    //     let sanitized_folder_name = display_name
    //         .to_lowercase()
    //         .split_whitespace()
    //         .collect::<Vec<&str>>()
    //         .join("-");

    //     collection_information.push((sanitized_folder_name.clone(), display_name.to_string()));
    // }

    // println!("VECTOR \n{:#?}", collection_information.clone());

    // let mut collection_relative_path = String::new();

    // for (folder_sanitized_name, folder_display_name) in &collection_information {
    //     if folder_sanitized_name.is_empty() {
    //         continue;
    //     }

    //     let mut current_collection_relative_path = collection_relative_path.clone();

    //     if !current_collection_relative_path.is_empty() {
    //         current_collection_relative_path.push_str("/");
    //     }
    //     current_collection_relative_path.push_str(folder_sanitized_name);

    //     fs::create_dir(
    //         &collection_parent_absolute_path.join(current_collection_relative_path.clone()),
    //     )
    //     .map_err(|err| ZakuError {
    //         error: err.to_string(),
    //         message: "Failed to create collection directory or it's parent directories".to_string(),
    //     })?;

    //     let current_collection_relative_path_from_root = vec![
    //         create_collection_dto.parent_relative_path.as_str(),
    //         current_collection_relative_path.as_str(),
    //     ]
    //     .into_iter()
    //     .filter(|path| !path.is_empty())
    //     .collect::<Vec<&str>>()
    //     .join("/");

    //     // let mut current_collection_relative_path_from_root = String::new();
    //     // current_collection_relative_path_from_root
    //     //     .push_str(&create_collection_dto.parent_relative_path);
    //     // if !current_collection_relative_path_from_root.is_empty() {
    //     //     current_collection_relative_path_from_root.push_str("/");
    //     // }
    //     // current_collection_relative_path_from_root.push_str(&current_collection_relative_path);

    //     println!(
    //         "DISPLAY NAME {} -> {}",
    //         current_collection_relative_path_from_root.clone(),
    //         folder_display_name.clone()
    //     );

    //     collection::save_display_name(
    //         &active_space_absolute_path,
    //         &current_collection_relative_path_from_root,
    //         &folder_display_name,
    //     )
    //     .unwrap_or_else(|err| {
    //         eprintln!("Failed to save display name {}", err);
    //     });

    //     if !collection_relative_path.is_empty() {
    //         collection_relative_path.push_str("/");
    //     }
    //     collection_relative_path.push_str(&folder_sanitized_name);
    // }

    // println!("FINAL PATH -> `{}`", collection_relative_path);

    collection::create_collections_all(&active_space_absolute_path, create_collection_dto)
        .map_err(|err| ZakuError {
            error: err.to_string(),
            message: "Failed to create collection directory or it's parent directories".to_string(),
        })?;

    println!("================================================");

    match space::parse_space(&active_space_absolute_path) {
        Ok(active_space) => {
            zaku_state.active_space = Some(active_space);

            return Ok(());
        }
        Err(err) => {
            return Err(ZakuError {
                error: err.to_string(),
                message: "Failed to parse space after creating the collection".to_string(),
            });
        }
    }
}
