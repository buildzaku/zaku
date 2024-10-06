use tauri::Manager;

#[tauri::command(rename_all = "snake_case")]
pub fn show_main_window(window: tauri::Window) {
    window.get_webview_window("main").unwrap().show().unwrap();
}
