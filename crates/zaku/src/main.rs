#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use anyhow::anyhow;
#[cfg(target_os = "linux")]
use ashpd::desktop::notification::{Notification, NotificationProxy, Priority};
use clap::Parser;
use gpui::{App, Application, PromptLevel, QuitMode, prelude::*};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use indoc::indoc;
use smol::future;
use std::{
    collections::HashMap,
    fmt,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::Arc,
};
use uuid::Uuid;
#[cfg(target_os = "windows")]
use windows::{Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID, core::HSTRING};

use assets::Assets;
use client::Client;
use crash_diagnostics::InitCrashHandler;
use db::{AppDatabase, kv::KeyValueStore};
use fs::{Fs, NativeFs};
use language::LanguageRegistry;
#[cfg(target_os = "windows")]
use metadata::ZAKU_IDENTIFIER;
use reqwest_client::ReqwestClient;
use session::{AppSession, Session};
use theme::{ActiveTheme, GlobalTheme, LoadThemes};
use workspace::AppState;
use zaku::{CrashHandler, EmptyRoot};

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    let args = Args::parse();
    if let Some(socket) = &args.crash_handler {
        crash_diagnostics::crash_server(socket, path::logs_dir().clone());
        return;
    }

    let app_version = metadata::load_version();
    let release_channel = app_version
        .release_channel()
        .expect("application version should have a release channel");
    let should_install_crash_handler =
        client::telemetry::should_install_crash_handler(release_channel);
    if !should_install_crash_handler {
        // SAFETY: No executors or platform threads have been created yet.
        unsafe { crash_diagnostics::force_backtrace() };
    }

    let file_errors = init_paths();
    if !file_errors.is_empty() {
        files_not_created_on_launch(file_errors);
        return;
    }

    logger::init();
    if zaku::stdout_is_terminal() {
        logger::init_output_stdout();
    } else {
        let result =
            logger::init_output_file(path::log_file().clone(), Some(path::old_log_file().clone()));
        if let Err(error) = result {
            eprintln!("Could not open log file: {error}... Defaulting to stdout");
            logger::init_output_stdout();
        }
    }

    #[cfg(target_os = "windows")]
    {
        // SAFETY: `HSTRING::from(ZAKU_IDENTIFIER)` provides a valid UTF-16 buffer for the duration
        // of this call.
        let result =
            unsafe { SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(ZAKU_IDENTIFIER)) };
        if let Err(error) = result {
            log::error!("Failed to set Windows application user model ID: {error}");
        }
    }

    let app =
        Application::with_platform(gpui_platform::current_platform(false)).with_assets(Assets);
    let app_db = AppDatabase::new();
    let kv_store = KeyValueStore::open(&app_db);
    let session_id = Uuid::new_v4().to_string();
    let session = app
        .background_executor()
        .spawn(Session::new(session_id.clone(), kv_store.clone()));
    let system_id = app.background_executor().spawn(system_id(kv_store.clone()));
    let installation_id = app.background_executor().spawn(installation_id(kv_store));
    let background_executor = app.background_executor();
    let crash_handler = if should_install_crash_handler {
        Some(app.background_executor().spawn(crash_diagnostics::init(
            InitCrashHandler {
                session_id,
                app_version: app_version.base().to_string(),
                binary: "zaku".to_string(),
                release_channel: release_channel.to_string(),
                commit_sha: metadata::ZAKU_COMMIT_SHA.to_string(),
            },
            {
                let background_executor = app.background_executor();
                move |task| {
                    background_executor.spawn(task).detach();
                }
            },
            |pid| path::cache_dir().join(format!("zaku-crash-handler-{pid}")),
            move |duration| background_executor.timer(duration),
        )))
    } else {
        None
    };

    app.run(move |cx: &mut App| {
        metadata::init(app_version, cx);
        cx.set_global(app_db);
        settings::init(cx);
        settings::log_settings::init(cx);
        let fs: Arc<dyn Fs> = Arc::new(NativeFs::new(cx.background_executor().clone()));
        let (user_settings_file_rx, user_settings_watcher) = settings::watch_config_file(
            cx.background_executor(),
            fs.clone(),
            path::settings_file().clone(),
        );
        let (user_keymap_file_rx, user_keymap_watcher) = settings::watch_config_file(
            cx.background_executor(),
            fs.clone(),
            path::keymap_file().clone(),
        );
        zaku::handle_settings_file_changes(user_settings_file_rx, user_settings_watcher, cx);
        zaku::handle_keymap_file_changes(user_keymap_file_rx, user_keymap_watcher, cx);
        theme_settings::init(LoadThemes::All(Box::new(Assets)), cx);
        register_embedded_fonts(cx);
        let system_id = match cx.foreground_executor().block_on(system_id) {
            Ok(system_id) => Some(system_id),
            Err(error) => {
                log::error!("Failed to initialize system ID: {error:#}");
                None
            }
        };
        let installation_id = match cx.foreground_executor().block_on(installation_id) {
            Ok(installation_id) => Some(installation_id),
            Err(error) => {
                log::error!("Failed to initialize installation ID: {error:#}");
                None
            }
        };
        let session = cx.foreground_executor().block_on(session);
        let http_client = Arc::new(ReqwestClient::new());
        let client = Client::new(http_client, cx);
        let telemetry = client.telemetry().clone();
        telemetry.start(
            system_id.as_ref().map(ToString::to_string),
            installation_id.as_ref().map(ToString::to_string),
            session.id().to_owned(),
        );
        if let (Some(system_id), Some(installation_id)) = (&system_id, &installation_id) {
            if matches!(
                (system_id, installation_id),
                (IdType::New(_), IdType::New(_))
            ) {
                telemetry::event!("App First Opened");
            } else {
                telemetry::event!("App Opened");
            }
        }
        let app_session = cx.new(|cx| AppSession::new(session, cx));
        let languages = Arc::new(LanguageRegistry::new(cx.background_executor().clone()));
        languages::init(languages.as_ref());
        languages.set_theme(cx.theme().clone());
        cx.observe_global::<GlobalTheme>({
            let languages = languages.clone();
            move |cx| {
                languages.set_theme(cx.theme().clone());
            }
        })
        .detach();
        let app_state = Arc::new(AppState::new(fs, client.clone(), app_session, languages));
        updater::init(client, path::cache_dir().clone(), cx);
        workspace::init(app_state.clone(), cx);
        project_panel::init(cx);
        editor::init(cx);
        request_editor::init(cx);
        response_panel::init(cx);
        title_bar::init(cx);
        zaku::init(cx);
        command_palette::init(cx);
        let menus = zaku::app_menu(cx);
        cx.set_menus(menus);
        telemetry.flush_events().detach();

        if let Some(mut crash_handler) = crash_handler {
            match gpui::block_on(future::poll_once(&mut crash_handler)) {
                Some(client) => cx.set_global(CrashHandler(client)),
                None => {
                    cx.spawn(async move |cx| {
                        let client = crash_handler.await;
                        cx.update(|cx| {
                            cx.set_global(CrashHandler(client));
                        });
                    })
                    .detach();
                }
            }
        }

        cx.activate(true);
        cx.spawn(
            async move |cx| match zaku::restore_or_create_workspace(app_state, cx).await {
                Ok(()) => {
                    cx.update(|cx| {
                        let menus = zaku::app_menu(cx);
                        cx.set_menus(menus);
                    });
                }
                Err(error) => {
                    log::error!("Failed to restore or create workspace: {error:#}");
                    cx.update(|cx| {
                        fail_to_open_window(
                            error,
                            &format!(
                                "Unable to open a window. Check the logs for more details:\n\n{}",
                                path::log_file().display()
                            ),
                            cx,
                        );
                    });
                }
            },
        )
        .detach();
    });
}

#[derive(Debug, Parser)]
#[command(name = "zaku")]
struct Args {
    /// Run the minidump crash server at the provided socket path.
    #[arg(long, hide = true)]
    crash_handler: Option<PathBuf>,
}

async fn system_id(kv_store: KeyValueStore) -> anyhow::Result<IdType> {
    let key_name = "system_id";
    if let Some(system_id) = kv_store.read_kv(key_name)? {
        return Ok(IdType::Existing(system_id));
    }

    let system_id = Uuid::new_v4().to_string();
    kv_store
        .write_kv(key_name.to_string(), system_id.clone())
        .await?;

    Ok(IdType::New(system_id))
}

async fn installation_id(kv_store: KeyValueStore) -> anyhow::Result<IdType> {
    let key_name = "installation_id";
    if let Some(installation_id) = kv_store.read_kv(key_name)? {
        return Ok(IdType::Existing(installation_id));
    }

    let installation_id = Uuid::new_v4().to_string();
    kv_store
        .write_kv(key_name.to_string(), installation_id.clone())
        .await?;

    Ok(IdType::New(installation_id))
}

#[derive(Debug, Clone)]
enum IdType {
    New(String),
    Existing(String),
}

impl fmt::Display for IdType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::New(id) | Self::Existing(id) => formatter.write_str(id),
        }
    }
}

fn init_paths() -> HashMap<ErrorKind, Vec<&'static Path>> {
    [
        path::config_dir(),
        path::data_dir(),
        path::logs_dir(),
        path::cache_dir(),
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
    let error_message = errors
        .into_iter()
        .filter_map(|(kind, paths)| {
            let error_kind_details = match paths.as_slice() {
                [] => return None,
                [path] => format!(
                    "{kind} when creating directory {}",
                    path.display()
                ),
                [_, ..] => format!("{kind} when creating directories {paths:?}"),
            };

            #[cfg(any(target_os = "linux", target_os = "macos"))]
            {
                if kind == ErrorKind::PermissionDenied {
                    let permission_hint = indoc! {"
                        Consider using chown and chmod tools for altering the directories permissions if your user has corresponding rights.

                        For example, `sudo chown $(whoami):staff ~/.config` and `chmod +uwrx ~/.config`
                    "};

                    return Some(format!("{error_kind_details}\n\n{permission_hint}"));
                }
            }

            Some(error_kind_details)
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    eprintln!("{message}: {error_message}");
    Application::with_platform(gpui_platform::current_platform(false))
        .with_assets(Assets)
        .run(move |cx| {
            settings::init(cx);
            theme_settings::init(LoadThemes::JustBase, cx);
            fail_to_open_window(anyhow!("{message}: {error_message}"), &error_message, cx);
        });
}

fn fail_to_open_window(error: anyhow::Error, error_message: &str, cx: &mut App) {
    let message = "Zaku failed to launch";
    let menus = zaku::app_menu(cx);
    cx.set_menus(menus);
    cx.set_quit_mode(QuitMode::LastWindowClosed);
    let mut window_options = workspace::build_window_options(None, cx);
    window_options.window_bounds = Some(workspace::default_window_bounds(cx));
    let error = match cx.open_window(window_options, |window, cx| {
        window.activate_window();
        cx.new(|cx| EmptyRoot::new(window, cx))
    }) {
        Ok(window) => match window.update(cx, |_, window, cx| {
            let response = window.prompt(
                PromptLevel::Critical,
                message,
                Some(error_message),
                &["Exit"],
                cx,
            );

            cx.spawn_in(window, async move |_, cx| {
                response.await?;
                cx.update(|_, cx| cx.quit())
            })
            .detach_and_log_err(cx);
        }) {
            Ok(()) => return,
            Err(prompt_error) => error.context(format!(
                "failed to show launch failure prompt: {prompt_error:?}"
            )),
        },
        Err(window_error) => error.context(format!(
            "failed to open launch failure prompt: {window_error:?}"
        )),
    };

    eprintln!("Zaku failed to open a window: {error:?}.");

    #[cfg(target_os = "linux")]
    {
        let notification_body = error_message.to_string();
        cx.spawn(async move |_| {
            let Ok(proxy) = NotificationProxy::new().await else {
                std::process::exit(1);
            };

            let notification_id = "dev.zaku.Oops";
            if let Err(error) = proxy
                .add_notification(
                    notification_id,
                    Notification::new("Zaku failed to launch")
                        .body(Some(notification_body.as_str()))
                        .priority(Priority::High)
                        .icon(ashpd::desktop::Icon::with_names([
                            "dialog-question-symbolic",
                        ])),
                )
                .await
            {
                eprintln!("Failed to show launch failure notification: {error:?}.");
            }

            std::process::exit(1);
        })
        .detach();
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    std::process::exit(1);
}
