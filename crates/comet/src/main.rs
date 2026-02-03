use gpui::{
    App, Application, Bounds, KeyBinding, WindowBounds, WindowOptions, actions, prelude::*, px,
    size,
};

use workspace::Workspace;

actions!(comet, [Quit]);

fn main() {
    Application::new()
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
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

            let window_size = size(px(1180.0), px(760.0));
            let mut bounds = Bounds::centered(None, window_size, cx);
            bounds.origin.y -= px(36.0);

            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |_, cx| cx.new(Workspace::new),
            )
            .unwrap();
        });
}
