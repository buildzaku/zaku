use gpui::{App, Hsla};

use theme::ActiveTheme;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
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
    PaletteBlue,
    PaletteGreen,
    PaletteOrange,
    PalettePurple,
    PaletteRed,
    PaletteYellow,
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
            Color::Accent | Color::Selected => cx.theme().colors().text_accent,
            Color::Error => cx.theme().status().error,
            Color::Success => cx.theme().status().success,
            Color::Warning => cx.theme().status().warning,
            Color::PaletteRed => cx.theme().colors().palette_red,
            Color::PaletteOrange => cx.theme().colors().palette_orange,
            Color::PaletteYellow => cx.theme().colors().palette_yellow,
            Color::PaletteGreen => cx.theme().colors().palette_green,
            Color::PaletteBlue => cx.theme().colors().palette_blue,
            Color::PalettePurple => cx.theme().colors().palette_purple,
            Color::Custom(color) => *color,
        }
    }
}

impl From<Hsla> for Color {
    fn from(color: Hsla) -> Self {
        Color::Custom(color)
    }
}
