mod app_menu;

pub use app_menu::app_menu;

use futures::{StreamExt, channel::mpsc::UnboundedReceiver};
use gpui::{
    App, AsyncApp, Bounds, Context, FocusHandle, Focusable, Image, ImageFormat, KeyBinding, Size,
    Task, TitlebarOptions, Window, WindowBounds, WindowKind, WindowOptions, prelude::*,
};
use std::sync::Arc;

#[cfg(target_os = "macos")]
use actions::zaku::{Hide, HideOthers, ShowAll};
use actions::{
    menu,
    workspace::CloseWindow,
    zaku::{About, Quit},
};
use metadata::{
    ZAKU_COMMIT_SHA, ZAKU_DESCRIPTION, ZAKU_IDENTIFIER, ZAKU_NAME, ZAKU_REPOSITORY, ZAKU_VERSION,
};
use settings::{KeymapFile, KeymapFileLoadResult, SettingsStore};
use theme::ActiveTheme;
use ui::{Headline, Label, LabelCommon, LabelSize, Link, TextSize, prelude::*};
use workspace::{
    CloseIntent, OpenMode, Root, SerializedWorkspaceLocation, SessionWorkspace, SharedState, Toast,
    Workspace, WorkspaceDb, notifications::NotificationId,
};

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
        cx.on_action(|_: &Hide, cx| cx.hide());
        cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
        cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    }

    cx.on_action(|_: &Quit, cx| {
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
    .on_action(|_: &About, cx| open_about_window(cx))
    .on_action(|_: &CloseWindow, cx| Workspace::close_window(cx));
}

fn open_about_window(cx: &mut App) {
    let window_size = Size {
        width: gpui::px(300.0),
        height: gpui::px(436.0),
    };
    let mut bounds = Bounds::centered(None, window_size, cx);
    bounds.origin.y -= gpui::px(36.0);

    struct AboutWindow {
        focus_handle: FocusHandle,
        app_icon: Arc<Image>,
    }

    impl AboutWindow {
        fn new(cx: &mut Context<Self>) -> Self {
            let app_icon = Arc::new(Image::from_bytes(
                ImageFormat::Png,
                include_bytes!("../resources/app-icon.png").to_vec(),
            ));

            Self {
                focus_handle: cx.focus_handle(),
                app_icon,
            }
        }
    }

    impl Render for AboutWindow {
        fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            v_flex()
                .id("about-window")
                .track_focus(&self.focus_handle)
                .on_action(cx.listener(|_, _: &menu::Cancel, window, _cx| window.remove_window()))
                .size_full()
                .bg(cx.theme().colors().background)
                .text_color(cx.theme().colors().text)
                .p_4()
                .when(cfg!(target_os = "macos"), |this| this.pt_10())
                .gap_3()
                .justify_center()
                .child(
                    v_flex()
                        .w_full()
                        .gap_1()
                        .items_center()
                        .child(gpui::img(self.app_icon.clone()).size_32().flex_none())
                        .child(Headline::new(ZAKU_NAME))
                        .child(Label::new(ZAKU_DESCRIPTION).size(LabelSize::XSmall))
                        .child(gpui::div().h_5())
                        .child(
                            gpui::div()
                                .grid()
                                .grid_cols(2)
                                .self_center()
                                .gap_x_2()
                                .child(
                                    gpui::div()
                                        .text_right()
                                        .child(Label::new("Version").size(LabelSize::Small)),
                                )
                                .child(
                                    gpui::div()
                                        .text_left()
                                        .font_buffer(cx)
                                        .child(Label::new(ZAKU_VERSION).size(LabelSize::Small)),
                                )
                                .child(
                                    gpui::div()
                                        .text_right()
                                        .child(Label::new("Commit").size(LabelSize::Small)),
                                )
                                .child(
                                    gpui::div().flex().flex_shrink().child(
                                        Link::new(
                                            ZAKU_COMMIT_SHA,
                                            format!("{ZAKU_REPOSITORY}/commits/{ZAKU_COMMIT_SHA}"),
                                        )
                                        .font_buffer()
                                        .text_size(TextSize::Small),
                                    ),
                                ),
                        )
                        .child(gpui::div().h_5())
                        .child(
                            h_flex().w_full().justify_center().px_6().child(
                                Button::new("about-github-repository", "GitHub")
                                    .variant(ButtonVariant::Solid)
                                    .label_size(LabelSize::Small)
                                    .on_click(|_, _, cx| cx.open_url(ZAKU_REPOSITORY)),
                            ),
                        ),
                )
        }
    }

    impl Focusable for AboutWindow {
        fn focus_handle(&self, _cx: &App) -> FocusHandle {
            self.focus_handle.clone()
        }
    }

    if let Some(existing) = cx
        .windows()
        .into_iter()
        .find_map(|window| window.downcast::<AboutWindow>())
    {
        if let Err(error) = existing.update(cx, |about_window, window, cx| {
            window.activate_window();
            about_window.focus_handle.focus(window, cx);
        }) {
            log::error!("Failed to activate About window: {error}");
        }
        return;
    }

    if let Err(error) = cx.open_window(
        WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some(format!("About {ZAKU_NAME}").into()),
                appears_transparent: true,
                traffic_light_position: Some(gpui::point(gpui::px(12.0), gpui::px(12.0))),
            }),
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            app_id: Some(ZAKU_IDENTIFIER.to_owned()),
            is_resizable: false,
            is_minimizable: false,
            kind: WindowKind::Normal,
            ..Default::default()
        },
        |window, cx| {
            let about_window = cx.new(AboutWindow::new);
            let focus_handle = about_window.read(cx).focus_handle.clone();
            window.activate_window();
            focus_handle.focus(window, cx);
            about_window
        },
    ) {
        log::error!("Failed to open about window: {error}");
    }
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

    let menus = app_menu(cx);
    cx.set_menus(menus);
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

pub async fn restore_or_create_workspace(
    shared_state: Arc<SharedState>,
    cx: &mut AsyncApp,
) -> anyhow::Result<()> {
    if let Some(workspaces) = restorable_workspace_locations(cx, &shared_state).await {
        let mut error_count = 0;

        for session_workspace in workspaces {
            let SerializedWorkspaceLocation::Local(path) = session_workspace.location;
            let result = cx
                .update(|cx| {
                    Workspace::open_local(path, shared_state.clone(), None, OpenMode::NewWindow, cx)
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
                    root.update(cx, |root, _, cx| {
                        root.workspace().update(cx, |workspace, cx| {
                            workspace.show_toast(
                                Toast::new(NotificationId::unique::<()>(), message.clone()),
                                cx,
                            );
                        });
                    })
                    .ok();
                    return true;
                }

                false
            });

            if !toast_shown {
                log::error!("All workspace restorations failed. Opening fallback empty workspace.");

                let workspace_db = cx.update(|cx| WorkspaceDb::global(cx));
                let workspace_id = workspace_db.next_id().await?;
                let shared_state = shared_state.clone();
                cx.update(|cx| {
                    let window_options = workspace::default_window_options(cx);
                    cx.open_window(window_options, move |window, cx| {
                        cx.new(|cx| {
                            let workspace =
                                Workspace::create(workspace_id, shared_state, window, cx);

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
            cx.new(|cx| Root::new(Workspace::create(workspace_id, shared_state, window, cx)))
        })
    })?;

    Ok(())
}

async fn restorable_workspace_locations(
    cx: &mut AsyncApp,
    shared_state: &Arc<SharedState>,
) -> Option<Vec<SessionWorkspace>> {
    let session_handle = shared_state.session.clone();
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
        shared_state.fs.as_ref(),
    )
    .await
    .filter(|locations| !locations.is_empty())?;

    if has_window_stack {
        locations.reverse();
    }

    Some(locations)
}
