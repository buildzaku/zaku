use futures::{StreamExt, channel::mpsc::UnboundedReceiver};
use gpui::{App, BorrowAppContext, KeyBinding, Task};
use settings::{KeymapFile, KeymapFileLoadResult, SettingsStore};

use actions::{workspace::CloseWindow, zaku::Quit};
use workspace::{Root, Workspace};

pub fn init(cx: &mut App) {
    register_actions(cx);

    cx.observe_new(|_root: &mut Root, window, cx| {
        let Some(window) = window else {
            return;
        };

        let root_handle = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            root_handle
                .update(cx, |root, cx| {
                    root.close_window(&CloseWindow, window, cx);
                    false
                })
                .unwrap_or(true)
        });
    })
    .detach();
    cx.on_window_closed(|cx| {
        if cx.windows().is_empty() {
            cx.quit();
        }
    })
    .detach();
}

fn register_actions(cx: &mut App) {
    cx.on_action(|_: &Quit, cx| {
        cx.spawn(async move |cx| {
            let workspace_windows = cx.update(|cx| {
                cx.windows()
                    .into_iter()
                    .filter_map(|window| window.downcast::<Root>())
                    .collect::<Vec<_>>()
            });

            let mut flush_tasks = Vec::new();
            for window in &workspace_windows {
                match window.update(cx, |root, window, cx| {
                    root.workspace().update(cx, |workspace, cx| {
                        workspace.flush_serialization(window, cx)
                    })
                }) {
                    Ok(flush_task) => flush_tasks.push(flush_task),
                    Err(error) => {
                        log::error!("Failed to flush workspace serialization before quit: {error}");
                    }
                }
            }

            futures::future::join_all(flush_tasks).await;
            cx.update(|cx| cx.quit());
        })
        .detach();
    })
    .on_action(|_: &CloseWindow, cx| Workspace::close_window(cx));
}

pub fn handle_settings_file_changes(
    mut user_settings_file_rx: UnboundedReceiver<String>,
    user_settings_watcher: Task<()>,
    cx: &mut App,
) {
    let user_content = cx
        .foreground_executor()
        .block_on(user_settings_file_rx.next())
        .unwrap();

    cx.update_global::<SettingsStore, _>(|store, cx| {
        let result = store.set_user_settings(&user_content, cx);
        if let settings::ParseStatus::Failed { error } = &result {
            log::error!("Failed to load user settings: {error}");
        }
    });

    cx.spawn(async move |cx| {
        let _user_settings_watcher = user_settings_watcher;
        while let Some(content) = user_settings_file_rx.next().await {
            cx.update_global(|store: &mut SettingsStore, cx| {
                let result = store.set_user_settings(&content, cx);
                if let settings::ParseStatus::Failed { error } = &result {
                    log::error!("Failed to load user settings: {error}");
                }
                cx.refresh_windows();
            });
        }
    })
    .detach();
}

pub fn handle_keymap_file_changes(
    mut user_keymap_file_rx: UnboundedReceiver<String>,
    user_keymap_watcher: Task<()>,
    cx: &mut App,
) {
    let (keyboard_layout_tx, mut keyboard_layout_rx) = futures::channel::mpsc::unbounded();

    #[cfg(target_os = "windows")]
    {
        let mut current_layout_id = cx.keyboard_layout().id().to_string();
        cx.on_keyboard_layout_change(move |cx| {
            let next_layout_id = cx.keyboard_layout().id();
            if next_layout_id != current_layout_id {
                current_layout_id = next_layout_id.to_string();
                keyboard_layout_tx.unbounded_send(()).ok();
            }
        })
        .detach();
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let mut current_mapping = cx.keyboard_mapper().get_key_equivalents().cloned();
        cx.on_keyboard_layout_change(move |cx| {
            let next_mapping = cx.keyboard_mapper().get_key_equivalents();
            if current_mapping.as_ref() != next_mapping {
                current_mapping = next_mapping.cloned();
                keyboard_layout_tx.unbounded_send(()).ok();
            }
        })
        .detach();
    }

    load_default_keymap(cx);

    cx.spawn(async move |cx| {
        let _user_keymap_watcher = user_keymap_watcher;
        let mut user_keymap_content = String::new();

        loop {
            futures::select_biased! {
                _ = keyboard_layout_rx.next() => {},
                content = user_keymap_file_rx.next() => {
                    if let Some(content) = content {
                        user_keymap_content = content;
                    }
                }
            }

            cx.update(|cx| match KeymapFile::load(&user_keymap_content, cx) {
                KeymapFileLoadResult::Success { key_bindings } => {
                    reload_keymaps(cx, key_bindings);
                }
                KeymapFileLoadResult::SomeFailedToLoad {
                    key_bindings,
                    error_message,
                } => {
                    if !key_bindings.is_empty() {
                        reload_keymaps(cx, key_bindings);
                    }
                    log::error!("Failed to load user keymap: {error_message}");
                }
                KeymapFileLoadResult::JsonParseFailure { error } => {
                    log::error!("Failed to parse user keymap: {error}");
                }
            });
        }
    })
    .detach();
}

fn reload_keymaps(cx: &mut App, user_key_bindings: Vec<KeyBinding>) {
    cx.clear_key_bindings();
    load_default_keymap(cx);
    cx.bind_keys(user_key_bindings);
}

fn load_default_keymap(cx: &mut App) {
    #[cfg(target_os = "macos")]
    let asset_path = "keymaps/default_macos.json";

    #[cfg(target_os = "windows")]
    let asset_path = "keymaps/default_windows.json";

    #[cfg(target_os = "linux")]
    let asset_path = "keymaps/default_linux.json";

    let key_bindings = match KeymapFile::load_asset(asset_path, cx) {
        Ok(key_bindings) => key_bindings,
        Err(error) => panic!("Failed to load default keymap: {error}"),
    };
    cx.bind_keys(key_bindings);
}
