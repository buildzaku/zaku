use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OpenDirectoryDialogOptions {
    pub title: Option<String>,
}

#[tauri::command]
pub async fn open_directory_dialog<R: tauri::Runtime>(
    options: Option<OpenDirectoryDialogOptions>,
    app_handle: AppHandle<R>,
) -> Result<Option<String>, String> {
    let mut dialog_builder = app_handle.dialog().file();

    match options {
        Some(OpenDirectoryDialogOptions {
            title: Some(ref title),
        }) => {
            dialog_builder = dialog_builder.set_title(title);
        }
        _ => {}
    }

    let directory_path = dialog_builder.blocking_pick_folder();

    match directory_path {
        Some(path) => {
            return Ok(Some(path.to_string_lossy().to_string()));
        }
        None => {
            return Ok(None);
        }
    }
}
