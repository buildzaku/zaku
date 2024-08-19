use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DispatchNotificationOptions {
    pub title: String,
    pub body: String,
}

#[tauri::command]
pub fn dispatch_notification(options: DispatchNotificationOptions, app_handle: AppHandle) {
    app_handle
        .notification()
        .builder()
        .title(&options.title)
        .body(&options.body)
        .show()
        .unwrap();
}
