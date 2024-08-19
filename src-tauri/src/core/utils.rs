use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{Code, Modifiers};

pub enum ShortcutCombination {
    CommandOptionI,
    ControlShiftI,
}

pub fn shortcut_combination(
    shortcut: &tauri_plugin_global_shortcut::Shortcut,
) -> Option<ShortcutCombination> {
    let modifiers = shortcut.mods;
    let code = shortcut.key;

    if modifiers.contains(Modifiers::SUPER)
        && modifiers.contains(Modifiers::ALT)
        && code == Code::KeyI
    {
        return Some(ShortcutCombination::CommandOptionI);
    } else if modifiers.contains(Modifiers::CONTROL)
        && modifiers.contains(Modifiers::SHIFT)
        && code == Code::KeyI
    {
        return Some(ShortcutCombination::ControlShiftI);
    } else {
        return None;
    }
}

pub fn toggle_devtools(app_handle: &AppHandle) {
    let webview_window = app_handle.get_webview_window("main").unwrap();

    if webview_window.is_devtools_open() {
        webview_window.close_devtools();
    } else {
        webview_window.open_devtools();
    }
}
