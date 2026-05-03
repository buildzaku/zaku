#[cfg(target_os = "linux")]
use ashpd::desktop::notification::{Notification, NotificationProxy, Priority};

use gpui::{App, Application, Empty, PromptLevel, QuitMode, prelude::*};
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
use db::{AppDatabase, kv::KeyValueStore};
use fs::{Fs, NativeFs};
use session::{AppSession, Session};
use theme::LoadThemes;
use workspace::SharedState;

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

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

    let app =
        Application::with_platform(gpui_platform::current_platform(false)).with_assets(Assets);
    let app_db = AppDatabase::new();
    let session = app.background_executor().spawn(Session::new(
        Uuid::new_v4().to_string(),
        KeyValueStore::from_app_db(&app_db),
    ));

    app.run(move |cx: &mut App| {
        cx.set_global(app_db);
        settings::init(cx);
        settings::log_settings::init(cx);
        let fs: Arc<dyn Fs> = Arc::new(NativeFs::new(cx.background_executor().clone()));
        let (user_settings_file_rx, user_settings_watcher) = settings::watch_config_file(
            cx.background_executor(),
            fs.clone(),
            settings::settings_file().clone(),
        );
        let (user_keymap_file_rx, user_keymap_watcher) = settings::watch_config_file(
            cx.background_executor(),
            fs.clone(),
            settings::keymap_file().clone(),
        );
        zaku::handle_settings_file_changes(user_settings_file_rx, user_settings_watcher, cx);
        zaku::handle_keymap_file_changes(user_keymap_file_rx, user_keymap_watcher, cx);
        theme::init(LoadThemes::All(Box::new(Assets)), cx);
        register_embedded_fonts(cx);
        editor::init(cx);
        let session = cx.foreground_executor().block_on(session);
        let app_session = cx.new(|cx| AppSession::new(session, cx));
        let shared_state = Arc::new(SharedState::new(fs, app_session));
        workspace::init(shared_state.clone(), cx);
        workspace::panel::project::init(cx);
        workspace::panel::response::init(cx);
        zaku::init(cx);
        let menus = zaku::app_menu(cx);
        cx.set_menus(menus);

        cx.activate(true);
        cx.spawn(async move |cx| {
            if let Err(error) = zaku::restore_or_create_workspace(shared_state, cx).await {
                log::error!("Failed to restore or create workspace: {error:#}");
            }
        })
        .detach();
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
        .filter_map(|(kind, paths)| {
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
            let window_options = workspace::default_window_options(cx);
            if let Ok(window) = cx.open_window(window_options, |_, cx| cx.new(|_| Empty)) {
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
                fail_to_open_window(anyhow::anyhow!("{message}: {error_details}"), cx);
            }
        });
}

fn fail_to_open_window(error: anyhow::Error, _cx: &mut App) {
    eprintln!("Zaku failed to open a window: {error:?}.");

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        std::process::exit(1);
    }

    #[cfg(target_os = "linux")]
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
