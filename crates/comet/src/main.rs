use gpui::{
    App, Application, Bounds, Context, FontWeight, KeyBinding, Window, WindowBounds, WindowOptions,
    actions, div, prelude::*, px, rgb, size,
};

actions!(comet, [Quit]);

struct Comet {}

impl Render for Comet {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .bg(rgb(0x141414))
            .size_full()
            .justify_center()
            .items_center()
            .text_2xl()
            .text_color(rgb(0xffffff))
            .font_weight(FontWeight::MEDIUM)
            .child("Welcome to Comet!".to_string())
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
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
            |_, cx| cx.new(|_| Comet {}),
        )
        .unwrap();
    });
}
