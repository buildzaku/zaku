#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use ashpd::desktop::notification::{Notification, NotificationProxy, Priority};

use futures::{StreamExt, channel::mpsc::UnboundedReceiver};
use gpui::{
    App, Application, Bounds, Empty, KeyBinding, Pixels, PromptLevel, QuitMode, Size, Task,
    WindowBounds, WindowOptions, prelude::*,
};
use gpui_platform;
use indoc::formatdoc;

#[cfg(unix)]
use indoc::indoc;

use std::{
    collections::HashMap,
    io::{ErrorKind, IsTerminal},
    path::Path,
    sync::Arc,
};
use uuid::Uuid;

use assets::Assets;
use fs::NativeFs;
use settings::SettingsStore;
use theme::LoadThemes;
use workspace::{CloseWindow, Root, SharedState, Workspace};

gpui::actions!(zaku, [Quit]);

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

const DEFAULT_WINDOW_SIZE: Size<Pixels> = gpui::size(gpui::px(1180.0), gpui::px(760.0));

fn main() {
    let file_errors = init_paths();
    if !file_errors.is_empty() {
        files_not_created_on_launch(file_errors);
        return;
    }

    logger::init();
    if std::io::stdout().is_terminal() {
        logger::init_output_stdout();
    } else {
        let result = logger::init_output_file(settings::log_file(), Some(settings::old_log_file()));
        if let Err(error) = result {
            eprintln!("Could not open log file: {error}... Defaulting to stdout");
            logger::init_output_stdout();
        }
    }

    Application::with_platform(gpui_platform::current_platform(false))
        .with_assets(Assets)
        .run(|cx: &mut App| {
            settings::init(cx);
            settings::log_settings::init(cx);
            let (user_settings_file_rx, user_settings_watcher) = settings::watch_config_file(
                cx.background_executor(),
                settings::settings_file().clone(),
            );
            handle_settings_file_changes(user_settings_file_rx, user_settings_watcher, cx);
            theme::init(LoadThemes::All(Box::new(Assets)), cx);
            register_embedded_fonts(cx);
            menu::init(cx);
            editor::init(cx);
            let shared_state = Arc::new(SharedState::new(
                Arc::new(NativeFs::new(cx.background_executor().clone())),
                Uuid::new_v4().to_string(),
            ));
            workspace::init(shared_state.clone(), cx);

            cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
            cx.on_action(quit);
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

            cx.activate(true);

            let mut bounds = Bounds::centered(None, DEFAULT_WINDOW_SIZE, cx);
            bounds.origin.y -= gpui::px(36.0);

            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                move |window, cx| {
                    let shared_state = shared_state.clone();
                    cx.new(|cx| {
                        let workspace = Workspace::create(shared_state, window, cx);
                        Root::new(workspace)
                    })
                },
            )
            .unwrap();
        });
}

fn init_paths() -> HashMap<ErrorKind, Vec<&'static Path>> {
    [
        settings::config_dir(),
        settings::data_dir(),
        settings::logs_dir(),
    ]
    .into_iter()
    .fold(HashMap::default(), |mut errors, path| {
        if let Err(error) = std::fs::create_dir_all(path) {
            errors
                .entry(error.kind())
                .or_insert_with(Vec::new)
                .push(path);
        }
        errors
    })
}

fn handle_settings_file_changes(
    mut user_settings_file_rx: UnboundedReceiver<String>,
    user_settings_watcher: Task<()>,
    cx: &mut App,
) {
    let user_content = match cx
        .foreground_executor()
        .block_on(user_settings_file_rx.next())
    {
        Some(content) => content,
        None => {
            log::error!("Failed to load settings file: settings channel closed");
            settings::default_user_settings().into_owned()
        }
    };

    cx.update_global::<SettingsStore, _>(|store, cx| {
        store.set_user_settings(&user_content, cx);
    });

    cx.spawn(async move |cx| {
        let _user_settings_watcher = user_settings_watcher;
        while let Some(content) = user_settings_file_rx.next().await {
            cx.update_global(|store: &mut SettingsStore, cx| {
                store.set_user_settings(&content, cx);
            });
        }
    })
    .detach();
}

fn register_embedded_fonts(cx: &App) {
    let asset_source = cx.asset_source();
    let font_paths = match asset_source.list("fonts") {
        Ok(font_paths) => font_paths,
        Err(error) => {
            log::error!("Failed to list bundled fonts: {error:?}");
            return;
        }
    };

    let mut embedded_fonts = Vec::new();
    for font_path in &font_paths {
        if !font_path.ends_with(".ttf") {
            continue;
        }

        match asset_source.load(font_path) {
            Ok(Some(font_bytes)) => embedded_fonts.push(font_bytes),
            Ok(None) => {
                log::error!("Asset source returned None for {font_path:?}");
            }
            Err(error) => {
                log::error!("Failed to load bundled font {font_path:?}: {error:?}");
            }
        }
    }

    if let Err(error) = cx.text_system().add_fonts(embedded_fonts) {
        log::error!("Failed to add bundled fonts: {error:?}");
    }
}

fn files_not_created_on_launch(errors: HashMap<ErrorKind, Vec<&Path>>) {
    let message = "Zaku failed to launch";
    let error_details = errors
        .into_iter()
        .flat_map(|(kind, paths)| {
            #[allow(unused_mut)]
            let mut error_kind_details = match paths.len() {
                0 => return None,
                1 => format!(
                    "{kind} when creating directory {:?}",
                    paths.first().expect("match arm checks for a single entry")
                ),
                _many => format!("{kind} when creating directories {paths:?}"),
            };

            #[cfg(unix)]
            {
                if kind == ErrorKind::PermissionDenied {
                    error_kind_details.push_str("\n\n");
                    error_kind_details.push_str(indoc! {"
                        Consider using chown and chmod tools for altering the directories permissions if your user has corresponding rights.

                        For example, `sudo chown $(whoami):staff ~/.config` and `chmod +uwrx ~/.config`
                    "});
                }
            }

            Some(error_kind_details)
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    eprintln!("{message}: {error_details}");
    Application::with_platform(gpui_platform::current_platform(false))
        .with_quit_mode(QuitMode::Explicit)
        .run(move |cx| {
            let mut bounds = Bounds::centered(None, DEFAULT_WINDOW_SIZE, cx);
            bounds.origin.y -= gpui::px(36.0);

            if let Ok(window) = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |_, cx| cx.new(|_| Empty),
            ) {
                if let Err(error) = window.update(cx, |_, window, cx| {
                    let response = window.prompt(
                        PromptLevel::Critical,
                        message,
                        Some(&error_details),
                        &["Exit"],
                        cx,
                    );

                    cx.spawn_in(window, async move |_, cx| {
                        response.await?;
                        cx.update(|_, cx| cx.quit())
                    })
                    .detach_and_log_err(cx);
                }) {
                    fail_to_open_window(
                        anyhow::anyhow!(formatdoc! {"
                            {message}: {error_details}

                            Failed to show launch failure prompt: {error:?}
                        "}),
                        cx,
                    );
                }
            } else {
                fail_to_open_window(anyhow::anyhow!("{message}: {error_details}"), cx)
            }
        })
}

fn fail_to_open_window(error: anyhow::Error, _cx: &mut App) {
    eprintln!("Zaku failed to open a window: {error:?}.");

    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    {
        std::process::exit(1);
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    {
        _cx.spawn(async move |_| {
            let Ok(proxy) = NotificationProxy::new().await else {
                std::process::exit(1);
            };

            let notification_id = "dev.zaku.Oops";
            let notification_body = format!("{error:?}.");
            proxy
                .add_notification(
                    notification_id,
                    Notification::new("Zaku failed to launch")
                        .body(Some(notification_body.as_str()))
                        .priority(Priority::High)
                        .icon(ashpd::desktop::Icon::with_names(&[
                            "dialog-question-symbolic",
                        ])),
                )
                .await
                .ok();

            std::process::exit(1);
        })
        .detach();
    }
}

fn quit(_: &Quit, cx: &mut App) {
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
        anyhow::Ok(())
    })
    .detach_and_log_err(cx);
}
