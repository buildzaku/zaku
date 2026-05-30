#[cfg(target_os = "macos")]
use gpui::SystemMenuType;
use gpui::{App, Menu, MenuItem};

use metadata::ZAKU_NAME;

pub fn app_menu(_cx: &mut App) -> Vec<Menu> {
    vec![
        Menu {
            name: ZAKU_NAME.into(),
            disabled: false,
            items: vec![
                MenuItem::action(format!("About {ZAKU_NAME}"), actions::zaku::About),
                MenuItem::separator(),
                MenuItem::submenu(Menu {
                    name: "Settings".into(),
                    disabled: false,
                    items: vec![
                        MenuItem::action("Open Settings File", actions::zaku::OpenSettingsFile),
                        MenuItem::action("Open Keymap File", actions::zaku::OpenKeymapFile),
                    ],
                }),
                MenuItem::separator(),
                #[cfg(target_os = "macos")]
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                #[cfg(target_os = "macos")]
                MenuItem::separator(),
                #[cfg(target_os = "macos")]
                MenuItem::action(format!("Hide {ZAKU_NAME}"), actions::zaku::Hide),
                #[cfg(target_os = "macos")]
                MenuItem::action("Hide Others", actions::zaku::HideOthers),
                #[cfg(target_os = "macos")]
                MenuItem::action("Show All", actions::zaku::ShowAll),
                #[cfg(target_os = "macos")]
                MenuItem::separator(),
                MenuItem::action(format!("Quit {ZAKU_NAME}"), actions::zaku::Quit),
            ],
        },
        Menu {
            name: "File".into(),
            disabled: false,
            items: vec![
                MenuItem::action("New Window", actions::workspace::NewWindow),
                MenuItem::separator(),
                MenuItem::action("Open…", actions::workspace::Open::default()),
                MenuItem::separator(),
                MenuItem::action("Close Project", actions::workspace::CloseProject),
                MenuItem::action("Close Window", actions::workspace::CloseWindow),
            ],
        },
        Menu {
            name: "View".into(),
            disabled: false,
            items: vec![
                MenuItem::action("Toggle Left Dock", actions::workspace::ToggleLeftDock),
                MenuItem::action("Toggle Bottom Dock", actions::workspace::ToggleBottomDock),
                MenuItem::separator(),
                MenuItem::action("Project Panel", actions::project_panel::ToggleFocus),
                MenuItem::action("Response Panel", actions::response_panel::ToggleFocus),
            ],
        },
        Menu {
            name: "Window".into(),
            disabled: false,
            items: vec![
                MenuItem::action("Minimize", actions::zaku::Minimize),
                MenuItem::action("Zoom", actions::zaku::Zoom),
                MenuItem::separator(),
            ],
        },
    ]
}
