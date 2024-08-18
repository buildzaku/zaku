use tauri::Manager;

#[tauri::command]
pub fn show_main_window(window: tauri::Window) {
    window.get_webview_window("main").unwrap().show().unwrap();
}
