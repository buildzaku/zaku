use gpui::{App, Hsla};

use theme::ActiveTheme;

#[derive(Debug, Default, Eq, PartialEq, Copy, Clone)]
pub enum Color {
    #[default]
    Default,
    Accent,
    Conflict,
    Created,
    Custom(Hsla),
    Deleted,
    Disabled,
    Error,
    Hidden,
    Hint,
    Ignored,
    Info,
    Modified,
    Muted,
    Placeholder,
    Selected,
    Success,
    Warning,
}

impl Color {
    pub fn color(&self, cx: &App) -> Hsla {
        match self {
            Color::Default => cx.theme().colors().text,
            Color::Muted => cx.theme().colors().text_muted,
            Color::Created => cx.theme().status().created,
            Color::Modified => cx.theme().status().modified,
            Color::Conflict => cx.theme().status().conflict,
            Color::Ignored => cx.theme().status().ignored,
            Color::Deleted => cx.theme().status().deleted,
            Color::Disabled => cx.theme().colors().text_disabled,
            Color::Hidden => cx.theme().status().hidden,
            Color::Hint => cx.theme().status().hint,
            Color::Info => cx.theme().status().info,
            Color::Placeholder => cx.theme().colors().text_placeholder,
            Color::Accent => cx.theme().colors().text_accent,
            Color::Selected => cx.theme().colors().text_accent,
            Color::Error => cx.theme().status().error,
            Color::Success => cx.theme().status().success,
            Color::Warning => cx.theme().status().warning,
            Color::Custom(color) => *color,
        }
    }
}

impl From<Hsla> for Color {
    fn from(color: Hsla) -> Self {
        Color::Custom(color)
    }
}
