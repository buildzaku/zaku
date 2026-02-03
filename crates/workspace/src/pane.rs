use gpui::{Entity, Window, div, prelude::*, rgb};

use input::InputField;
use ui::{Button, ButtonCommon, ButtonSize, ButtonVariant, FixedWidth, rems_from_px};

pub struct Pane {
    input: Option<Entity<InputField>>,
}

impl Pane {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self { input: None }
    }
}

impl Render for Pane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.input.is_none() {
            let input = cx.new(|cx| InputField::new(window, cx, "https://example.com"));
            self.input = Some(input);
        }

        let input = self
            .input
            .clone()
            .expect("InputField should be initialized");

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x1a1a1a))
            .p_3()
            .child("HTTP Request")
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .w_full()
                    .py_2()
                    .gap_2()
                    .child(div().flex_1().child(input))
                    .child(
                        Button::new("request-send", "Send")
                            .variant(ButtonVariant::Accent)
                            .size(ButtonSize::Large)
                            .width(rems_from_px(68.))
                            .font_weight(gpui::FontWeight::SEMIBOLD),
                    ),
            )
            .child(div().flex_1())
    }
}
