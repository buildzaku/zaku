pub(crate) mod migrate;

use futures::{StreamExt, channel::mpsc::UnboundedReceiver};
use gpui::{Action, App, DismissEvent, KeyBinding, Task, prelude::*};

use ::settings::{
    KeymapFile, KeymapLoadResult, MigrationStatus, SettingsLoadResult, SettingsLoadStatus,
    SettingsStore,
};
use migrator::migrate_keymap;
use workspace::notifications::{
    NotificationId, dismiss_app_notification, show_app_notification,
    simple_message_notification::MessageNotification,
};

use crate::settings::migrate::{MigrationEvent, MigrationNotification, MigrationType};

pub fn handle_settings_file_changes(
    mut user_settings_file_rx: UnboundedReceiver<String>,
    user_settings_watcher: Task<()>,
    cx: &mut App,
) {
    MigrationNotification::set_global(cx.new(|_| MigrationNotification), cx);

    let user_content = cx
        .foreground_executor()
        .block_on(user_settings_file_rx.next())
        .expect("user settings file should be loaded");

    cx.update_global::<SettingsStore, _>(|store, cx| {
        let result = store.set_user_settings(&user_content, cx);
        let did_show_settings_error_notification = sync_settings_error_notification(&result, cx);
        if let Some(notifier) = MigrationNotification::try_global(cx) {
            notifier.update(cx, |_, cx| {
                cx.emit(MigrationEvent::ContentChanged {
                    migration_type: MigrationType::Settings,
                    using_in_memory_migration: matches!(
                        &result.migration_status,
                        MigrationStatus::Succeeded
                    ),
                });
            });
        }
        sync_settings_migration_notification(
            &result.migration_status,
            did_show_settings_error_notification,
            cx,
        );
    });

    cx.spawn(async move |cx| {
        let _user_settings_watcher = user_settings_watcher;
        while let Some(content) = user_settings_file_rx.next().await {
            cx.update_global(|store: &mut SettingsStore, cx| {
                let result = store.set_user_settings(&content, cx);
                let did_show_settings_error_notification =
                    sync_settings_error_notification(&result, cx);
                if let Some(notifier) = MigrationNotification::try_global(cx) {
                    notifier.update(cx, |_, cx| {
                        cx.emit(MigrationEvent::ContentChanged {
                            migration_type: MigrationType::Settings,
                            using_in_memory_migration: matches!(
                                &result.migration_status,
                                MigrationStatus::Succeeded
                            ),
                        });
                    });
                }
                sync_settings_migration_notification(
                    &result.migration_status,
                    did_show_settings_error_notification,
                    cx,
                );
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
    struct KeymapErrorNotification;
    struct KeymapMigrationErrorNotification;

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

    let error_notification_id = NotificationId::unique::<KeymapErrorNotification>();
    let migration_notification_id = NotificationId::unique::<KeymapMigrationErrorNotification>();

    cx.spawn(async move |cx| {
        let _user_keymap_watcher = user_keymap_watcher;
        let mut user_keymap_content = String::new();
        let mut user_keymap_migration_status = MigrationStatus::NotNeeded;

        loop {
            let mut did_check_keymap_migration_status = false;

            futures::select_biased! {
                _ = keyboard_layout_rx.next() => {},
                content = user_keymap_file_rx.next() => {
                    if let Some(content) = content {
                        did_check_keymap_migration_status = true;
                        match migrate_keymap(&content) {
                            Ok(Some(migrated_content)) => {
                                user_keymap_content = migrated_content;
                                user_keymap_migration_status = MigrationStatus::Succeeded;
                            }
                            Ok(None) => {
                                user_keymap_content = content;
                                user_keymap_migration_status = MigrationStatus::NotNeeded;
                            }
                            Err(error) => {
                                log::error!("Failed to migrate user keymap: {error}");
                                user_keymap_content = content;
                                user_keymap_migration_status = MigrationStatus::Failed {
                                    error: error.to_string(),
                                };
                            }
                        }
                    }
                }
            }

            cx.update(|cx| {
                let load_result = KeymapFile::load(&user_keymap_content, cx);
                if let Some(notifier) = MigrationNotification::try_global(cx) {
                    notifier.update(cx, |_, cx| {
                        cx.emit(MigrationEvent::ContentChanged {
                            migration_type: MigrationType::Keymap,
                            using_in_memory_migration: matches!(
                                user_keymap_migration_status,
                                MigrationStatus::Succeeded
                            ),
                        });
                    });
                }
                let did_show_keymap_error_notification =
                    sync_keymap_error_notification(load_result, error_notification_id.clone(), cx);

                if did_check_keymap_migration_status {
                    sync_keymap_migration_notification(
                        &user_keymap_migration_status,
                        migration_notification_id.clone(),
                        did_show_keymap_error_notification,
                        cx,
                    );
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

    let menus = crate::app_menu(cx);
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

fn sync_settings_error_notification(result: &SettingsLoadResult, cx: &mut App) -> bool {
    let notification_id = NotificationId::named("failed-to-load-settings".into());

    let error_message = match &result.status {
        SettingsLoadStatus::Loaded => {
            dismiss_app_notification(&notification_id, cx);
            return false;
        }
        SettingsLoadStatus::PartiallyLoaded { error_message } => {
            log::error!("Failed to load user settings: {error_message}");
            error_message.as_str()
        }
        SettingsLoadStatus::FailedToParseJsonc { error } => {
            log::error!("Failed to parse user settings: {error}");
            error.as_str()
        }
        SettingsLoadStatus::FailedToLoad { error } => {
            log::error!("Failed to load user settings: {error}");
            error.as_str()
        }
    };

    let message = format!("Invalid user settings file\n{error_message}");
    show_app_notification(notification_id, cx, move |cx| {
        cx.new(|cx| {
            MessageNotification::new(message.clone(), cx)
                .primary_message("Open Settings File")
                .primary_on_click(|window, cx| {
                    window.dispatch_action(actions::zaku::OpenSettingsFile.boxed_clone(), cx);
                    cx.emit(DismissEvent);
                })
        })
    });
    true
}

fn sync_settings_migration_notification(
    migration_status: &MigrationStatus,
    did_show_settings_error_notification: bool,
    cx: &mut App,
) {
    let notification_id = NotificationId::named("failed-to-migrate-settings".into());

    match migration_status {
        MigrationStatus::Failed { error } if !did_show_settings_error_notification => {
            log::error!("Failed to migrate user settings: {error}");
            let message = format!("Failed to migrate user settings\n{error}");
            show_app_notification(notification_id, cx, move |cx| {
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
        MigrationStatus::Failed { .. }
        | MigrationStatus::NotNeeded
        | MigrationStatus::Succeeded => {
            dismiss_app_notification(&notification_id, cx);
        }
    }
}

fn sync_keymap_error_notification(
    load_result: KeymapLoadResult,
    notification_id: NotificationId,
    cx: &mut App,
) -> bool {
    let error_message = match load_result {
        KeymapLoadResult::Loaded { key_bindings } => {
            reload_keymaps(cx, key_bindings);
            dismiss_app_notification(&notification_id, cx);
            return false;
        }
        KeymapLoadResult::PartiallyLoaded {
            key_bindings,
            error_message,
        } => {
            if !key_bindings.is_empty() {
                reload_keymaps(cx, key_bindings);
            }
            log::error!("Failed to load user keymap: {error_message}");
            let error_message = error_message
                .strip_prefix("Errors in user keymap file.")
                .unwrap_or(error_message.as_str())
                .trim_start();
            error_message.to_string()
        }
        KeymapLoadResult::FailedToParseJsonc { error } => {
            log::error!("Failed to parse user keymap: {error}");
            error
        }
        KeymapLoadResult::FailedToLoad { error } => {
            log::error!("Failed to load user keymap: {error}");
            error
        }
    };

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
    true
}

fn sync_keymap_migration_notification(
    migration_status: &MigrationStatus,
    notification_id: NotificationId,
    did_show_keymap_error_notification: bool,
    cx: &mut App,
) {
    match migration_status {
        MigrationStatus::Failed { error } if !did_show_keymap_error_notification => {
            let message = format!("Failed to migrate user keymap\n{error}");
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
        MigrationStatus::Failed { .. }
        | MigrationStatus::NotNeeded
        | MigrationStatus::Succeeded => {
            dismiss_app_notification(&notification_id, cx);
        }
    }
}
