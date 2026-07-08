mod about;
mod app_menu;
mod logs;

pub use app_menu::app_menu;

use futures::{StreamExt, channel::mpsc::UnboundedReceiver};
use gpui::{
    Action, App, AsyncApp, ClipboardItem, Context, DismissEvent, KeyBinding, PromptLevel, Task,
    Window, prelude::*,
};
use std::{borrow::Cow, path::Path, sync::Arc};

use migrator::migrate_keymap;
use project_panel::ProjectPanel;
use response_panel::ResponsePanel;
use settings::{KeymapFile, KeymapFileLoadResult, SettingsParseResult, SettingsStore};
use system_specs::SystemSpecs;
use workspace::{
    AppState, CloseIntent, DockPosition, OpenMode, Panel, Root, SessionWorkspace, Toast, Workspace,
    WorkspaceDb, create_and_open_file,
    notifications::{
        NotificationId, dismiss_app_notification, show_app_notification,
        simple_message_notification::MessageNotification,
    },
    with_active_or_new_workspace,
};

use crate::logs::open_log_file;

pub fn init(cx: &mut App) {
    register_actions(cx);

    cx.observe_new(|workspace: &mut Workspace, window, cx| {
        let Some(window) = window else {
            return;
        };

        let project_panel = ProjectPanel::new(workspace, window, cx);
        let project_panel_should_start_open = project_panel.read(cx).starts_open(window, cx);
        workspace.add_panel(project_panel, DockPosition::Left, window, cx);
        if !project_panel_should_start_open {
            workspace.left_dock().update(cx, |dock, cx| {
                dock.set_open(false, window, cx);
            });
        }

        let response_panel = cx.new(|cx| ResponsePanel::new(window, cx));
        workspace.add_panel(response_panel, DockPosition::Bottom, window, cx);

        workspace.register_action(
            |_, _: &actions::zaku::CopySystemSpecsIntoClipboard, window, cx| {
                let specs = SystemSpecs::new(
                    window,
                    cx,
                    system_specs::os_name(),
                    system_specs::os_version(),
                );

                cx.spawn_in(window, async move |_, cx| {
                    let specs = specs.await.to_string();

                    cx.update(|_, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(specs.clone()));
                    })?;

                    if let Err(error) = cx
                        .prompt(
                            PromptLevel::Info,
                            "Copied into clipboard",
                            Some(&specs),
                            &["OK"],
                        )
                        .await
                    {
                        log::debug!("Failed to show copied system specs prompt: {error}");
                    }

                    anyhow::Ok(())
                })
                .detach_and_log_err(cx);
            },
        );
    })
    .detach();

    cx.observe_new(|_root: &mut Root, window, cx| {
        let Some(window) = window else {
            return;
        };

        let root_handle = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            root_handle
                .update(cx, |root, cx| {
                    root.close_window(&actions::workspace::CloseWindow, window, cx);
                    false
                })
                .unwrap_or(true)
        });
    })
    .detach();
    cx.on_window_closed(|cx, _| {
        if cx.windows().is_empty() {
            cx.quit();
        }
    })
    .detach();
}

fn register_actions(cx: &mut App) {
    #[cfg(target_os = "macos")]
    {
        cx.on_action(|_: &actions::zaku::Hide, cx| cx.hide());
        cx.on_action(|_: &actions::zaku::HideOthers, cx| cx.hide_other_apps());
        cx.on_action(|_: &actions::zaku::ShowAll, cx| cx.unhide_other_apps());
    }

    cx.on_action(|_: &actions::zaku::Quit, cx| {
        cx.spawn(async move |cx| {
            let workspace_windows = cx.update(|cx| {
                cx.windows()
                    .into_iter()
                    .filter_map(|window| window.downcast::<Root>())
                    .collect::<Vec<_>>()
            });

            for window in &workspace_windows {
                let prepare_task = match window.update(cx, |root, window, cx| {
                    root.workspace().update(cx, |workspace, cx| {
                        workspace.prepare_to_close(CloseIntent::Quit, window, cx)
                    })
                }) {
                    Ok(prepare_task) => prepare_task,
                    Err(error) => {
                        log::error!("Failed to prepare workspace for quit: {error}");
                        return anyhow::Ok(());
                    }
                };

                let should_quit = match prepare_task.await {
                    Ok(should_quit) => should_quit,
                    Err(error) => {
                        log::error!("Failed to prepare workspace for quit: {error}");
                        return anyhow::Ok(());
                    }
                };

                if !should_quit {
                    return anyhow::Ok(());
                }
            }

            cx.update(|cx| cx.quit());

            anyhow::Ok(())
        })
        .detach();
    })
    .on_action(|_: &actions::zaku::About, cx| about::open_window(cx))
    .on_action(|_: &actions::zaku::OpenSettingsFile, cx| {
        with_active_or_new_workspace(cx, |_, window, cx| {
            open_settings_file(
                path::settings_file(),
                settings::initial_user_settings,
                window,
                cx,
            );
        });
    })
    .on_action(|_: &actions::zaku::OpenKeymapFile, cx| {
        with_active_or_new_workspace(cx, |_, window, cx| {
            open_settings_file(
                path::keymap_file(),
                settings::initial_user_keymap,
                window,
                cx,
            );
        });
    })
    .on_action(|_: &actions::zaku::OpenLogs, cx| {
        with_active_or_new_workspace(cx, |workspace, window, cx| {
            open_log_file(workspace, window, cx);
        });
    })
    .on_action(|_: &actions::workspace::CloseWindow, cx| Workspace::close_window(cx));
}

fn open_settings_file(
    abs_path: &'static Path,
    default_content: impl FnOnce() -> Cow<'static, str> + Send + 'static,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let config_dir = path::config_dir().clone();
    cx.spawn_in(window, async move |workspace, cx| {
        let project = workspace.read_with(cx, |workspace, _| workspace.project().clone())?;
        let (_worktree, _) = project
            .update(cx, |project, cx| {
                project.find_or_create_worktree(&config_dir, false, cx)
            })
            .await?;

        workspace
            .update_in(cx, |_, window, cx| {
                create_and_open_file(abs_path, window, cx, default_content)
            })?
            .await?;

        anyhow::Ok(())
    })
    .detach_and_log_err(cx);
}

pub fn handle_settings_file_changes(
    mut user_settings_file_rx: UnboundedReceiver<String>,
    user_settings_watcher: Task<()>,
    cx: &mut App,
) {
    let user_content = cx
        .foreground_executor()
        .block_on(user_settings_file_rx.next())
        .expect("user settings file should be loaded");

    cx.update_global::<SettingsStore, _>(|store, cx| {
        let result = store.set_user_settings(&user_content, cx);
        notify_settings_file_errors(&result, cx);
    });

    cx.spawn(async move |cx| {
        let _user_settings_watcher = user_settings_watcher;
        while let Some(content) = user_settings_file_rx.next().await {
            cx.update_global(|store: &mut SettingsStore, cx| {
                let result = store.set_user_settings(&content, cx);
                notify_settings_file_errors(&result, cx);
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
    struct KeymapParseErrorNotification;

    let (keyboard_layout_tx, mut keyboard_layout_rx) = futures::channel::mpsc::unbounded();

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let mut current_mapping = cx.keyboard_mapper().get_key_equivalents().cloned();
        cx.on_keyboard_layout_change(move |cx| {
            let next_mapping = cx.keyboard_mapper().get_key_equivalents();
            if current_mapping.as_ref() != next_mapping {
                current_mapping = next_mapping.cloned();
                if keyboard_layout_tx.unbounded_send(()).is_err() {
                    log::trace!("Keyboard layout update receiver dropped");
                }
            }
        })
        .detach();
    }

    #[cfg(target_os = "windows")]
    {
        let mut current_layout_id = cx.keyboard_layout().id().to_string();
        cx.on_keyboard_layout_change(move |cx| {
            let next_layout_id = cx.keyboard_layout().id();
            if next_layout_id != current_layout_id {
                current_layout_id = next_layout_id.to_string();
                if keyboard_layout_tx.unbounded_send(()).is_err() {
                    log::trace!("Keyboard layout update receiver dropped");
                }
            }
        })
        .detach();
    }

    load_default_keymap(cx);

    let notification_id = NotificationId::unique::<KeymapParseErrorNotification>();

    cx.spawn(async move |cx| {
        let _user_keymap_watcher = user_keymap_watcher;
        let mut user_keymap_content = String::new();

        loop {
            futures::select_biased! {
                _ = keyboard_layout_rx.next() => {},
                content = user_keymap_file_rx.next() => {
                    if let Some(content) = content {
                        if let Ok(Some(migrated_content)) = migrate_keymap(&content) {
                            user_keymap_content = migrated_content;
                        } else {
                            user_keymap_content = content;
                        }
                    }
                }
            }

            cx.update(|cx| match KeymapFile::load(&user_keymap_content, cx) {
                KeymapFileLoadResult::Success { key_bindings } => {
                    reload_keymaps(cx, key_bindings);
                    dismiss_app_notification(&notification_id.clone(), cx);
                }
                KeymapFileLoadResult::SomeFailedToLoad {
                    key_bindings,
                    error_message,
                } => {
                    if !key_bindings.is_empty() {
                        reload_keymaps(cx, key_bindings);
                    }
                    log::error!("Failed to load user keymap: {error_message}");
                    show_keymap_file_load_error(notification_id.clone(), &error_message, cx);
                }
                KeymapFileLoadResult::JsoncParseFailure { error } => {
                    log::error!("Failed to parse user keymap: {error}");
                    show_keymap_file_jsonc_error(notification_id.clone(), &error, cx);
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

    let menus = app_menu(cx);
    cx.set_menus(menus);
}

fn load_default_keymap(cx: &mut App) {
    #[cfg(target_os = "linux")]
    let asset_path = "keymaps/default_linux.jsonc";

    #[cfg(target_os = "macos")]
    let asset_path = "keymaps/default_macos.jsonc";

    #[cfg(target_os = "windows")]
    let asset_path = "keymaps/default_windows.jsonc";

    let key_bindings = KeymapFile::load_asset(asset_path, cx).expect("default keymap should load");
    cx.bind_keys(key_bindings);
}

fn notify_settings_file_errors(result: &SettingsParseResult, cx: &mut App) {
    let id = NotificationId::named("failed-to-parse-settings".into());
    match result.parse_error() {
        Some(error) => {
            log::error!("Failed to load user settings: {error}");
            let message = format!("Invalid user settings file\n{error}");
            show_app_notification(id, cx, move |cx| {
                cx.new(|cx| {
                    MessageNotification::new(message.clone(), cx)
                        .primary_message("Open Settings File")
                        .primary_on_click(|window, cx| {
                            window
                                .dispatch_action(actions::zaku::OpenSettingsFile.boxed_clone(), cx);
                            cx.emit(DismissEvent);
                        })
                })
            });
        }
        None => {
            dismiss_app_notification(&id, cx);
        }
    }
}

fn show_keymap_file_jsonc_error(
    notification_id: NotificationId,
    error: &anyhow::Error,
    cx: &mut App,
) {
    let message = format!("Invalid user keymap file\n{error}");
    show_app_notification(notification_id, cx, move |cx| {
        cx.new(|cx| {
            MessageNotification::new(message.clone(), cx)
                .primary_message("Open Keymap File")
                .primary_on_click(|window, cx| {
                    window.dispatch_action(actions::zaku::OpenKeymapFile.boxed_clone(), cx);
                    cx.emit(DismissEvent);
                })
        })
    });
}

fn show_keymap_file_load_error(notification_id: NotificationId, error_message: &str, cx: &mut App) {
    let error_message = error_message
        .strip_prefix("Errors in user keymap file.")
        .unwrap_or(error_message)
        .trim_start();
    let message = format!("Invalid user keymap file\n{error_message}");
    show_app_notification(notification_id, cx, move |cx| {
        cx.new(|cx| {
            MessageNotification::new(message.clone(), cx)
                .primary_message("Open Keymap File")
                .primary_on_click(|window, cx| {
                    window.dispatch_action(actions::zaku::OpenKeymapFile.boxed_clone(), cx);
                    cx.emit(DismissEvent);
                })
        })
    });
}

pub async fn restore_or_create_workspace(
    app_state: Arc<AppState>,
    cx: &mut AsyncApp,
) -> anyhow::Result<()> {
    if let Some(workspaces) = restorable_workspace_locations(cx, &app_state).await {
        let mut error_count = 0;

        for session_workspace in workspaces {
            let result = cx
                .update(|cx| {
                    Workspace::open(
                        session_workspace.location,
                        app_state.clone(),
                        None,
                        OpenMode::NewWindow,
                        cx,
                    )
                })
                .await;

            if let Err(error) = result {
                log::error!("Failed to restore workspace: {error:#}");
                error_count += 1;
            }
        }

        if error_count > 0 {
            let message = if error_count == 1 {
                "Failed to restore 1 workspace. Check logs for details.".to_string()
            } else {
                format!("Failed to restore {error_count} workspaces. Check logs for details.")
            };

            let toast_shown = cx.update(|cx| {
                if let Some(window) = cx.active_window()
                    && let Some(root) = window.downcast::<Root>()
                {
                    if let Err(error) = root.update(cx, |root, _, cx| {
                        root.workspace().update(cx, |workspace, cx| {
                            workspace.show_toast(
                                Toast::new(NotificationId::unique::<()>(), message.clone()),
                                cx,
                            );
                        });
                    }) {
                        log::trace!("Failed to show workspace restore toast: {error:?}");
                    }
                    return true;
                }

                false
            });

            if !toast_shown {
                log::error!("All workspace restorations failed. Opening fallback empty workspace.");

                let workspace_db = cx.update(|cx| WorkspaceDb::global(cx));
                let workspace_id = workspace_db.next_id().await?;
                let app_state = app_state.clone();
                cx.update(|cx| {
                    let window_options = workspace::default_window_options(cx);
                    cx.open_window(window_options, move |window, cx| {
                        cx.new(|cx| {
                            let workspace = Workspace::create(workspace_id, app_state, window, cx);

                            workspace.update(cx, |workspace, cx| {
                                workspace.show_toast(
                                    Toast::new(NotificationId::unique::<()>(), message),
                                    cx,
                                );
                            });

                            Root::new(workspace)
                        })
                    })
                })?;
            }
        }

        return Ok(());
    }

    let workspace_db = cx.update(|cx| WorkspaceDb::global(cx));
    let workspace_id = workspace_db.next_id().await?;
    cx.update(|cx| {
        let window_options = workspace::default_window_options(cx);
        cx.open_window(window_options, move |window, cx| {
            cx.new(|cx| Root::new(Workspace::create(workspace_id, app_state, window, cx)))
        })
    })?;

    Ok(())
}

async fn restorable_workspace_locations(
    cx: &mut AsyncApp,
    app_state: &Arc<AppState>,
) -> Option<Vec<SessionWorkspace>> {
    let session_handle = app_state.session.clone();
    let (last_session_id, last_session_window_stack) = cx.update(|cx| {
        let session = session_handle.read(cx);

        (
            session.last_session_id().map(|id| id.to_string()),
            session.last_session_window_stack(),
        )
    });

    let last_session_id = last_session_id?;
    let has_window_stack = last_session_window_stack.is_some();
    let workspace_db = cx.update(|cx| WorkspaceDb::global(cx));

    let mut locations = workspace::last_session_workspace_locations(
        &workspace_db,
        &last_session_id,
        last_session_window_stack,
        app_state.fs.as_ref(),
    )
    .await
    .filter(|locations| !locations.is_empty())?;

    if has_window_stack {
        locations.reverse();
    }

    Some(locations)
}
