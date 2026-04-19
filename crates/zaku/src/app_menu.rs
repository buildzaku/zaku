#[cfg(target_os = "macos")]
use gpui::SystemMenuType;
use gpui::{App, Menu, MenuItem};

#[cfg(target_os = "macos")]
use actions::zaku::{Hide, HideOthers, ShowAll};
use actions::{
    workspace::{self, project_panel, response_panel},
    zaku::{About, Minimize, Quit, Zoom},
};
use metadata::ZAKU_NAME;

pub fn app_menu(_cx: &mut App) -> Vec<Menu> {
    vec![
        Menu {
            name: ZAKU_NAME.into(),
            disabled: false,
            items: vec![
                MenuItem::action(format!("About {ZAKU_NAME}"), About),
                MenuItem::separator(),
                #[cfg(target_os = "macos")]
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                #[cfg(target_os = "macos")]
                MenuItem::separator(),
                #[cfg(target_os = "macos")]
                MenuItem::action(format!("Hide {ZAKU_NAME}"), Hide),
                #[cfg(target_os = "macos")]
                MenuItem::action("Hide Others", HideOthers),
                #[cfg(target_os = "macos")]
                MenuItem::action("Show All", ShowAll),
                #[cfg(target_os = "macos")]
                MenuItem::separator(),
                MenuItem::action(format!("Quit {ZAKU_NAME}"), Quit),
            ],
        },
        Menu {
            name: "File".into(),
            disabled: false,
            items: vec![
                MenuItem::action("New Window", workspace::NewWindow),
                MenuItem::separator(),
                MenuItem::action("Open…", workspace::Open::default()),
                MenuItem::separator(),
                MenuItem::action("Close Project", workspace::CloseProject),
                MenuItem::action("Close Window", workspace::CloseWindow),
            ],
        },
        Menu {
            name: "View".into(),
            disabled: false,
            items: vec![
                MenuItem::action("Toggle Left Dock", workspace::ToggleLeftDock),
                MenuItem::action("Toggle Bottom Dock", workspace::ToggleBottomDock),
                MenuItem::separator(),
                MenuItem::action("Project Panel", project_panel::ToggleFocus),
                MenuItem::action("Response Panel", response_panel::ToggleFocus),
            ],
        },
        Menu {
            name: "Window".into(),
            disabled: false,
            items: vec![
                MenuItem::action("Minimize", Minimize),
                MenuItem::action("Zoom", Zoom),
                MenuItem::separator(),
            ],
        },
    ]
}
