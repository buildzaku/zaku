use futures::StreamExt;
use gpui::{
    App, Application, Bounds, KeyBinding, WindowBounds, WindowOptions, actions, prelude::*,
};

use settings::{LoadSettings, SettingsStore};
use theme::LoadThemes;
use workspace::Workspace;

actions!(comet, [Quit]);

fn main() {
    Application::new()
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            settings::init(LoadSettings::None, cx);
            let (user_settings_file_rx, user_settings_watcher) = settings::watch_config_file(
                cx.background_executor(),
                settings::settings_file().clone(),
            );
            handle_settings_file_changes(user_settings_file_rx, user_settings_watcher, cx);
            theme::init(LoadThemes::All(Box::new(assets::Assets)), cx);
            editor::init(cx);
            workspace::init(cx);

            cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
            cx.on_action(|_: &Quit, cx: &mut App| {
                cx.quit();
            });
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
                |window, cx| cx.new(|cx| Workspace::new(window, cx)),
            )
            .unwrap();
        });
}

fn handle_settings_file_changes(
    mut user_settings_file_rx: futures::channel::mpsc::UnboundedReceiver<String>,
    user_settings_watcher: gpui::Task<()>,
    cx: &mut App,
) {
    let user_content = match cx
        .foreground_executor()
        .block_on(user_settings_file_rx.next())
    {
        Some(content) => content,
        None => {
            eprintln!("failed to load settings file: settings channel closed");
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
