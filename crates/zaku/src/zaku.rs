mod about;
mod app_menu;
mod logs;
mod settings;

pub use app_menu::app_menu;
pub use settings::{handle_keymap_file_changes, handle_settings_file_changes};

use gpui::{
    App, AsyncApp, ClipboardItem, Context, Entity, PromptLevel, Tiling, Window, prelude::*,
};
use std::{borrow::Cow, io::IsTerminal, path::Path, sync::Arc};

use ::settings::{initial_user_keymap, initial_user_settings};
use project_panel::ProjectPanel;
use response_panel::ResponsePanel;
use system_specs::SystemSpecs;
use theme::ActiveTheme;
use title_bar::TitleBar;
use ui::StyledTypography;
use workspace::{
    AppState, Breadcrumbs, CloseIntent, DockPosition, OpenMode, Panel, Root, SessionWorkspace,
    Toast, Workspace, WorkspaceDb, WorkspaceEvent, create_and_open_file,
    notifications::NotificationId, pane::Pane, with_active_or_new_workspace,
};

use crate::{logs::open_log_file, settings::migrate::MigrationBanner};

pub struct EmptyRoot {
    title_bar: Entity<TitleBar>,
}

impl EmptyRoot {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            title_bar: cx.new(|cx| TitleBar::new("title-bar", None, window, cx)),
        }
    }
}

impl Render for EmptyRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui_font = theme::setup_ui_font(window, cx);
        let theme_colors = cx.theme().colors();

        workspace::client_side_decorations(
            gpui::div()
                .relative()
                .size_full()
                .on_action(|_: &actions::workspace::CloseWindow, window, _| window.remove_window())
                .child(
                    gpui::div()
                        .flex()
                        .flex_1()
                        .size_full()
                        .overflow_hidden()
                        .child(
                            gpui::div()
                                .relative()
                                .flex()
                                .flex_col()
                                .text_color(theme_colors.text)
                                .font(ui_font)
                                .text_ui(cx)
                                .size_full()
                                .overflow_hidden()
                                .child(self.title_bar.clone())
                                .child(
                                    gpui::div()
                                        .id("workspace")
                                        .bg(theme_colors.background)
                                        .relative()
                                        .flex()
                                        .flex_1()
                                        .overflow_hidden()
                                        .border_y_1()
                                        .border_color(theme_colors.border),
                                ),
                        ),
                ),
            window,
            cx,
            Tiling::default(),
        )
    }
}

pub fn init(cx: &mut App) {
    register_actions(cx);

    cx.observe_new(|workspace: &mut Workspace, window, cx| {
        let Some(window) = window else {
            return;
        };

        let workspace_handle = cx.entity();
        let center_pane = workspace.pane().clone();
        initialize_pane_toolbar(&center_pane, window, cx);

        cx.subscribe_in(&workspace_handle, window, move |_, _, event, window, cx| {
            if let WorkspaceEvent::PaneAdded(pane) = event {
                initialize_pane_toolbar(pane, window, cx);
            }
        })
        .detach();

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

fn initialize_pane_toolbar(pane: &Entity<Pane>, window: &mut Window, cx: &mut Context<Workspace>) {
    let workspace_handle = cx.weak_entity();
    pane.update(cx, |pane, cx| {
        pane.toolbar().update(cx, |toolbar, cx| {
            let breadcrumbs = cx.new(|_| Breadcrumbs::new());
            toolbar.add_item(breadcrumbs, window, cx);

            let migration_banner =
                cx.new(move |inner_cx| MigrationBanner::new(workspace_handle, inner_cx));
            toolbar.add_item(migration_banner, window, cx);
        });
    });
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
            open_settings_file(path::settings_file(), initial_user_settings, window, cx);
        });
    })
    .on_action(|_: &actions::zaku::OpenKeymapFile, cx| {
        with_active_or_new_workspace(cx, |_, window, cx| {
            open_settings_file(path::keymap_file(), initial_user_keymap, window, cx);
        });
    })
    .on_action(|_: &actions::workspace::CloseWindow, cx| Workspace::close_window(cx));

    if !stdout_is_terminal() {
        cx.on_action(|_: &actions::zaku::OpenLogs, cx| {
            with_active_or_new_workspace(cx, |workspace, window, cx| {
                open_log_file(workspace, window, cx);
            });
        });
    }
}

pub fn stdout_is_terminal() -> bool {
    std::io::stdout().is_terminal()
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
