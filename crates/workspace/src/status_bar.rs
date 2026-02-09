use gpui::{Action, Window, prelude::*};

use theme::ActiveTheme;
use ui::{
    ButtonCommon, ButtonShape, ButtonSize, Clickable, IconButton, IconName, StyledTypography,
    Tooltip,
};

use crate::ToggleLeftDock;

pub struct StatusBar {}

impl StatusBar {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {}
    }
}

impl Render for StatusBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme_colors = cx.theme().colors();

        gpui::div()
            .flex()
            .flex_row()
            .items_center()
            .w_full()
            .h(gpui::px(26.))
            .px_1p5()
            .gap_2()
            .font_ui(cx)
            .text_ui_sm(cx)
            .bg(theme_colors.status_bar_background)
            .border_t_1()
            .border_color(theme_colors.border_variant)
            .child(
                IconButton::new("toggle-dock", IconName::Dock)
                    .size(ButtonSize::Compact)
                    .shape(ButtonShape::Square)
                    .tooltip(Tooltip::for_action_title(
                        "Toggle Left Dock",
                        &ToggleLeftDock,
                    ))
                    .on_click(move |_, window, cx| {
                        window.dispatch_action(ToggleLeftDock.boxed_clone(), cx);
                    }),
            )
    }
}
