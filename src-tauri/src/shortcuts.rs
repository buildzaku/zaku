use tauri::Manager;
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

use crate::{error::Result, utils};

pub fn initialize(app: &mut tauri::App) -> Result<()> {
    #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
    {
        use tauri_plugin_global_shortcut::ShortcutState;

        #[cfg(target_os = "macos")]
        let shortcuts = ["Command+Option+I"];

        #[cfg(target_os = "windows")]
        let shortcuts = ["Control+Shift+I"];

        #[cfg(target_os = "linux")]
        let shortcuts = ["Control+Shift+I", "Command+Option+I"];

        app.handle().plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcuts(shortcuts)?
                .with_handler(|handler_app, shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        match shortcut_combination(shortcut) {
                            #[cfg(target_os = "macos")]
                            Some(ShortcutCombination::CommandOptionI) => {
                                utils::toggle_devtools(handler_app.app_handle());
                            }
                            #[cfg(target_os = "macos")]
                            Some(ShortcutCombination::ControlShiftI) => {}

                            #[cfg(target_os = "windows")]
                            Some(ShortcutCombination::ControlShiftI) => {
                                utils::toggle_devtools(handler_app.app_handle());
                            }
                            #[cfg(target_os = "windows")]
                            Some(ShortcutCombination::CommandOptionI) => {}

                            #[cfg(target_os = "linux")]
                            Some(ShortcutCombination::ControlShiftI) => {
                                utils::toggle_devtools(handler_app.app_handle());
                            }
                            #[cfg(target_os = "linux")]
                            Some(ShortcutCombination::CommandOptionI) => {
                                utils::toggle_devtools(handler_app.app_handle());
                            }

                            None => {}
                        }
                    }
                })
                .build(),
        )?
    }

    Ok(())
}

pub enum ShortcutCombination {
    CommandOptionI,
    ControlShiftI,
}

pub fn shortcut_combination(shortcut: &Shortcut) -> Option<ShortcutCombination> {
    let modifiers = shortcut.mods;
    let code = shortcut.key;

    if modifiers.contains(Modifiers::SUPER)
        && modifiers.contains(Modifiers::ALT)
        && code == Code::KeyI
    {
        Some(ShortcutCombination::CommandOptionI)
    } else if modifiers.contains(Modifiers::CONTROL)
        && modifiers.contains(Modifiers::SHIFT)
        && code == Code::KeyI
    {
        Some(ShortcutCombination::ControlShiftI)
    } else {
        None
    }
}
