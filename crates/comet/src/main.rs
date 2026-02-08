use gpui::{
    App, Application, Bounds, KeyBinding, WindowBounds, WindowOptions, actions, prelude::*,
};

use settings::LoadSettings;
use theme::{LoadThemes, ThemeSettings};
use workspace::Workspace;

actions!(comet, [Quit]);

fn main() {
    Application::new()
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            settings::init(LoadSettings::User, cx);
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
                |window, cx| {
                    window.set_rem_size(ThemeSettings::get_global(cx).ui_font_size(cx));
                    cx.new(|cx| Workspace::new(window, cx))
                },
            )
            .unwrap();
        });
}
