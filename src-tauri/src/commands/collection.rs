use std::{fs, path::PathBuf, sync::Mutex};

use tauri::{AppHandle, Manager};

use crate::{
    core::space,
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
    let active_space_path = PathBuf::from(&active_space.absolute_path);
    let collection_path = active_space_path
        .join(
            create_collection_dto
                .relative_location
                .strip_prefix("/")
                .unwrap(),
        )
        .join(create_collection_dto.folder_name.clone());

    println!("active space absolute_path: {}", active_space.absolute_path);
    println!(
        "collection relative_location: {}",
        create_collection_dto.relative_location.to_string()
    );
    println!(
        "collection folder_name: {}",
        create_collection_dto.folder_name.to_string()
    );
    println!(
        "Trying to create: {}",
        collection_path.to_string_lossy().into_owned()
    );

    match fs::create_dir_all(&collection_path) {
        Ok(_) => match space::parse_space(&active_space_path) {
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
        },
        Err(err) => {
            return Err(ZakuError {
                error: err.to_string(),
                message: "Failed to create collection directory or it's parent directories"
                    .to_string(),
            });
        }
    }
}
