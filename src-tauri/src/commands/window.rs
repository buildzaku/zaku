use tauri::Manager;

#[specta::specta]
#[tauri::command]
pub fn show_main_window(window: tauri::Window) {
    window.get_webview_window("main").unwrap().show().unwrap();
}
