use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_notification::{NotificationExt, PermissionState};

use crate::types::ZakuError;

#[tauri::command]
pub fn is_notification_permission_granted(app_handle: AppHandle) -> Result<bool, ZakuError> {
    let permission_state = app_handle
        .notification()
        .permission_state()
        .map_err(|err| ZakuError {
            error: err.to_string(),
            message: "Failed to get current permissions state.".to_string(),
        })?;

    return Ok(permission_state == PermissionState::Granted);
}

#[tauri::command]
pub fn request_notification_permission(app_handle: AppHandle) -> Result<bool, ZakuError> {
    let permission_state = app_handle
        .notification()
        .request_permission()
        .map_err(|err| ZakuError {
            error: err.to_string(),
            message: "Failed to request for permissions.".to_string(),
        })?;

    return Ok(permission_state == PermissionState::Granted);
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DispatchNotificationOptions {
    pub title: String,
    pub body: String,
}

#[tauri::command]
pub fn dispatch_notification(
    options: DispatchNotificationOptions,
    app_handle: AppHandle,
) -> Result<(), ZakuError> {
    app_handle
        .notification()
        .builder()
        .title(&options.title)
        .body(&options.body)
        .show()
        .map_err(|err| ZakuError {
            error: err.to_string(),
            message: "Failed to dispatch notification.".to_string(),
        })?;

    return Ok(());
}
