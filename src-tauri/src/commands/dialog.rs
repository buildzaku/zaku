use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

use crate::models::OpenDirDialogOpt;

#[specta::specta]
#[tauri::command]
pub async fn open_dir_dialog(
    options: Option<OpenDirDialogOpt>,
    app_handle: AppHandle<tauri::Wry>,
) -> Result<Option<String>, String> {
    let mut dialog_builder = app_handle.dialog().file();

    if let Some(OpenDirDialogOpt {
        title: Some(ref title),
    }) = options
    {
        dialog_builder = dialog_builder.set_title(title);
    }

    let directory_path = dialog_builder.blocking_pick_folder();

    match directory_path {
        Some(path) => Ok(Some(
            path.into_path().unwrap().to_string_lossy().to_string(),
        )),
        None => Ok(None),
    }
}
