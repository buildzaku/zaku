use futures::{StreamExt, channel::mpsc::UnboundedReceiver};
use gpui::{App, Application, Bounds, KeyBinding, Task, WindowBounds, WindowOptions, prelude::*};
use gpui_platform;
use std::{collections::HashMap, io::IsTerminal, path::Path, sync::Arc};
use uuid::Uuid;

use fs::NativeFs;
use settings::SettingsStore;
use theme::LoadThemes;
use workspace::{CloseWindow, Root, SharedState, Workspace};

gpui::actions!(zaku, [Quit]);

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    let file_errors = init_paths();
    if !file_errors.is_empty() {
        eprintln!("Zaku failed to launch: {file_errors:?}");
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
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            settings::init(cx);
            settings::log_settings::init(cx);
            let (user_settings_file_rx, user_settings_watcher) = settings::watch_config_file(
                cx.background_executor(),
                settings::settings_file().clone(),
            );
            handle_settings_file_changes(user_settings_file_rx, user_settings_watcher, cx);
            theme::init(LoadThemes::All(Box::new(assets::Assets)), cx);
            register_embedded_fonts(cx);
            menu::init(cx);
            editor::init(cx);
            let shared_state = Arc::new(SharedState::new(
                Arc::new(NativeFs::new()),
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

            let window_size = gpui::size(gpui::px(1180.0), gpui::px(760.0));
            let mut bounds = Bounds::centered(None, window_size, cx);
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

fn init_paths() -> HashMap<std::io::ErrorKind, Vec<&'static Path>> {
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
