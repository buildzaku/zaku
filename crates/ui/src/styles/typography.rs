use gpui::{App, Pixels, Rems, SharedString, Window, prelude::*};

use theme::{ActiveTheme, ThemeSettings};

pub trait StyledTypography: Styled + Sized {
    fn font_buffer(self, cx: &App) -> Self {
        let settings = ThemeSettings::get_global(cx);
        self.font_family(settings.buffer_font.family.clone())
    }

    fn font_ui(self, cx: &App) -> Self {
        let settings = ThemeSettings::get_global(cx);
        self.font_family(settings.ui_font.family.clone())
    }

    fn text_ui_size(self, size: TextSize, cx: &App) -> Self {
        self.text_size(size.rems(cx))
    }

    fn text_ui_lg(self, cx: &App) -> Self {
        self.text_size(TextSize::Large.rems(cx))
    }

    fn text_ui(self, cx: &App) -> Self {
        self.text_size(TextSize::default().rems(cx))
    }

    fn text_ui_sm(self, cx: &App) -> Self {
        self.text_size(TextSize::Small.rems(cx))
    }

    fn text_ui_xs(self, cx: &App) -> Self {
        self.text_size(TextSize::XSmall.rems(cx))
    }

    fn text_buffer(self, cx: &App) -> Self {
        let settings = ThemeSettings::get_global(cx);
        self.text_size(settings.buffer_font_size(cx))
    }
}

impl<E: Styled> StyledTypography for E {}

#[derive(Debug, Default, Clone, Copy)]
pub enum TextSize {
    #[default]
    Default,
    Large,
    Small,
    XSmall,
    Ui,
    Editor,
}

impl TextSize {
    pub fn rems(self, cx: &App) -> Rems {
        let theme_settings = ThemeSettings::get_global(cx);
        match self {
            Self::Large => crate::rems_from_px(16.0),
            Self::Default => crate::rems_from_px(14.0),
            Self::Small => crate::rems_from_px(12.0),
            Self::XSmall => crate::rems_from_px(10.0),
            Self::Ui => crate::rems_from_px(theme_settings.ui_font_size(cx)),
            Self::Editor => crate::rems_from_px(theme_settings.buffer_font_size(cx)),
        }
    }

    pub fn pixels(self, cx: &App) -> Pixels {
        let theme_settings = ThemeSettings::get_global(cx);
        match self {
            Self::Large => gpui::px(16.),
            Self::Default => gpui::px(14.),
            Self::Small => gpui::px(12.),
            Self::XSmall => gpui::px(10.),
            Self::Ui => theme_settings.ui_font_size(cx),
            Self::Editor => theme_settings.buffer_font_size(cx),
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy, Default)]
pub enum HeadlineSize {
    XSmall,
    Small,
    #[default]
    Medium,
    Large,
    XLarge,
}

impl HeadlineSize {
    pub fn rems(self) -> Rems {
        match self {
            Self::XSmall => gpui::rems(0.88),
            Self::Small => gpui::rems(1.0),
            Self::Medium => gpui::rems(1.125),
            Self::Large => gpui::rems(1.27),
            Self::XLarge => gpui::rems(1.43),
        }
    }

    pub fn line_height(self) -> Rems {
        match self {
            Self::XSmall | Self::Small | Self::Medium | Self::Large | Self::XLarge => {
                gpui::rems(1.6)
            }
        }
    }
}

#[derive(IntoElement)]
pub struct Headline {
    size: HeadlineSize,
    text: SharedString,
}

impl Headline {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            size: HeadlineSize::default(),
            text: text.into(),
        }
    }

    pub fn size(mut self, size: HeadlineSize) -> Self {
        self.size = size;
        self
    }
}

impl RenderOnce for Headline {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        gpui::div()
            .font(ThemeSettings::get_global(cx).ui_font.clone())
            .line_height(self.size.line_height())
            .text_size(self.size.rems())
            .text_color(cx.theme().colors().text)
            .child(self.text)
    }
}
