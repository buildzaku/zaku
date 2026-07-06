use gpui::{App, IntoElement, SharedString, Window, prelude::*};

use super::{ButtonCommon, ButtonLike, ButtonSize};

use crate::{Clickable, Color, Icon, IconAsset, IconSize, Text, TextCommon, TextSize};

#[derive(IntoElement)]
pub struct LinkButton {
    text: SharedString,
    text_size: TextSize,
    text_color: Color,
    link: String,
    no_icon: bool,
}

impl LinkButton {
    pub fn new(text: impl Into<SharedString>, link: impl Into<String>) -> Self {
        Self {
            link: link.into(),
            text: text.into(),
            text_size: TextSize::Default,
            text_color: Color::Default,
            no_icon: false,
        }
    }

    pub fn no_icon(mut self, no_icon: bool) -> Self {
        self.no_icon = no_icon;
        self
    }

    pub fn text_size(mut self, text_size: TextSize) -> Self {
        self.text_size = text_size;
        self
    }

    pub fn text_color(mut self, text_color: Color) -> Self {
        self.text_color = text_color;
        self
    }
}

impl RenderOnce for LinkButton {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let id = format!("{}-{}", self.text, self.link);

        ButtonLike::new(id)
            .size(ButtonSize::None)
            .child(
                gpui::div()
                    .flex()
                    .items_center()
                    .gap_0p5()
                    .child(
                        Text::new(self.text)
                            .size(self.text_size)
                            .color(self.text_color)
                            .underline(),
                    )
                    .when(!self.no_icon, |this| {
                        this.child(
                            Icon::new(IconAsset::ArrowUpRight)
                                .size(IconSize::Small)
                                .color(Color::Muted),
                        )
                    }),
            )
            .on_click(move |_, _, cx| cx.open_url(&self.link))
            .into_any_element()
    }
}
