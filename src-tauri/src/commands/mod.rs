use std::{fs, path::PathBuf, sync::Mutex};
use tauri::{AppHandle, Manager};

use crate::core;
use crate::models::{
    zaku::{ZakuError, ZakuState},
    MoveTreeItemDto,
};

pub mod collection;
pub mod dialog;
pub mod notification;
pub mod request;
pub mod space;
pub mod state;
pub mod window;

#[specta::specta]
#[tauri::command]
pub fn move_tree_item(
    move_tree_item_dto: MoveTreeItemDto,
    app_handle: AppHandle,
) -> Result<(), ZakuError> {
    let state = app_handle.state::<Mutex<ZakuState>>();
    let mut zaku_state = state.lock().unwrap();
    let active_space = zaku_state
        .active_space
        .clone()
        .expect("Active space not found");
    let active_space_abspath = PathBuf::from(&active_space.abspath);
    let MoveTreeItemDto {
        src_relpath,
        dest_relpath,
    } = move_tree_item_dto;
    let src_abspath = active_space_abspath.join(src_relpath);
    let dest_abspath = active_space_abspath.join(dest_relpath);

    fs::rename(src_abspath, dest_abspath).expect("Unable to move tree item");

    match core::space::parse_space(&active_space_abspath) {
        Ok(active_space) => zaku_state.active_space = Some(active_space),
        Err(err) => {
            return Err(ZakuError {
                error: err.to_string(),
                message: "Failed to parse space after moving the tree item".to_string(),
            })
        }
    }

    return Ok(());
}
