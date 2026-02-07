use gpui::{App, Pixels, Rems, Styled};

use theme::ThemeSettings;

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
            Self::Large => gpui::px(16.0),
            Self::Default => gpui::px(14.0),
            Self::Small => gpui::px(12.0),
            Self::XSmall => gpui::px(10.0),
            Self::Ui => theme_settings.ui_font_size(cx),
            Self::Editor => theme_settings.buffer_font_size(cx),
        }
    }
}
