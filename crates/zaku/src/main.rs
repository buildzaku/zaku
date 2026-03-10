use futures::{StreamExt, channel::mpsc::UnboundedReceiver};
use gpui::{
    App, Application, Bounds, Context, Entity, FocusHandle, Focusable, KeyBinding, Render, Task,
    Window, WindowBounds, WindowOptions, prelude::*,
};
use gpui_platform;
use std::sync::Arc;
use uuid::Uuid;

use fs::NativeFs;
use settings::SettingsStore;
use theme::LoadThemes;
use workspace::{CloseProject, OpenRecent, SharedState, Workspace};

gpui::actions!(zaku, [Quit]);

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

struct Root {
    workspace: Entity<Workspace>,
}

impl Root {
    fn new(workspace: Entity<Workspace>) -> Self {
        Self { workspace }
    }

    fn replace_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let shared_state = self.workspace.read(cx).shared_state().clone();
        self.workspace = Workspace::create(shared_state, window, cx);
        cx.notify();
    }

    fn open_recent_project(&mut self, _: &OpenRecent, window: &mut Window, cx: &mut Context<Self>) {
        self.replace_workspace(window, cx);
    }

    fn close_project(&mut self, _: &CloseProject, window: &mut Window, cx: &mut Context<Self>) {
        self.replace_workspace(window, cx);
    }
}

impl Focusable for Root {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.workspace.read(cx).focus_handle(cx)
    }
}

impl Render for Root {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        gpui::div()
            .size_full()
            .on_action(cx.listener(Self::open_recent_project))
            .on_action(cx.listener(Self::close_project))
            .child(self.workspace.clone())
    }
}

fn main() {
    Application::with_platform(gpui_platform::current_platform(false))
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            settings::init(cx);
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

fn register_embedded_fonts(cx: &App) {
    let asset_source = cx.asset_source();
    let font_paths = match asset_source.list("fonts") {
        Ok(font_paths) => font_paths,
        Err(error) => {
            eprintln!("failed to list bundled fonts: {error:?}");
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
                eprintln!("asset source returned None for {font_path:?}");
            }
            Err(error) => {
                eprintln!("failed to load bundled font {font_path:?}: {error:?}");
            }
        }
    }

    if let Err(error) = cx.text_system().add_fonts(embedded_fonts) {
        eprintln!("failed to add bundled fonts: {error:?}");
    }
}
