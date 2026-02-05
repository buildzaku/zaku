use gpui::{Entity, Window, div, prelude::*, px};

use theme::ActiveTheme;
use ui::{ButtonCommon, ButtonShape, ButtonSize, Clickable, IconButton, IconName};

use crate::Workspace;

pub struct StatusBar {
    workspace: Entity<Workspace>,
}

impl StatusBar {
    pub fn new(workspace: Entity<Workspace>, _cx: &mut Context<Self>) -> Self {
        Self { workspace }
    }
}

impl Render for StatusBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = self.workspace.clone();
        let theme_colors = cx.theme().colors();

        div()
            .flex()
            .flex_row()
            .items_center()
            .w_full()
            .h(px(26.))
            .px_1p5()
            .gap_2()
            .bg(theme_colors.status_bar_background)
            .border_t_1()
            .border_color(theme_colors.border_variant)
            .child(
                IconButton::new("toggle-dock", IconName::Dock)
                    .size(ButtonSize::Compact)
                    .shape(ButtonShape::Square)
                    .on_click(move |_, _, cx| workspace.update(cx, |w, cx| w.toggle_dock(cx))),
            )
    }
}
