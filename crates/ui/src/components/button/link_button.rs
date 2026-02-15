use gpui::{App, IntoElement, SharedString, Window, prelude::*};

use crate::{ButtonLike, prelude::*};

#[derive(IntoElement)]
pub struct LinkButton {
    label: SharedString,
    label_size: LabelSize,
    label_color: Color,
    link: String,
    no_icon: bool,
}

impl LinkButton {
    pub fn new(label: impl Into<SharedString>, link: impl Into<String>) -> Self {
        Self {
            link: link.into(),
            label: label.into(),
            label_size: LabelSize::Default,
            label_color: Color::Default,
            no_icon: false,
        }
    }

    pub fn no_icon(mut self, no_icon: bool) -> Self {
        self.no_icon = no_icon;
        self
    }

    pub fn label_size(mut self, label_size: LabelSize) -> Self {
        self.label_size = label_size;
        self
    }

    pub fn label_color(mut self, label_color: Color) -> Self {
        self.label_color = label_color;
        self
    }
}

impl RenderOnce for LinkButton {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let id = format!("{}-{}", self.label, self.link);

        ButtonLike::new(id)
            .size(ButtonSize::None)
            .child(
                h_flex()
                    .gap_0p5()
                    .child(
                        Label::new(self.label)
                            .size(self.label_size)
                            .color(self.label_color)
                            .underline(),
                    )
                    .when(!self.no_icon, |this| {
                        this.child(
                            Icon::new(IconName::ArrowUpRight)
                                .size(IconSize::Small)
                                .color(Color::Muted),
                        )
                    }),
            )
            .on_click(move |_, _, cx| cx.open_url(&self.link))
            .into_any_element()
    }
}
