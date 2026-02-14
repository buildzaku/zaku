use gpui::{App, ClickEvent, CursorStyle, Window};

pub trait Clickable {
    fn on_click(self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self;
    fn cursor_style(self, cursor_style: CursorStyle) -> Self;
}
