use gpui::{Entity, Window, div, prelude::*, px, rgb};

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
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = self.workspace.clone();

        div()
            .flex()
            .flex_row()
            .items_center()
            .w_full()
            .h(px(26.))
            .px_1p5()
            .gap_2()
            .bg(rgb(0x141414))
            .border_t_1()
            .border_color(rgb(0x2a2a2a))
            .child(
                IconButton::new("toggle-dock", IconName::Dock)
                    .size(ButtonSize::Compact)
                    .shape(ButtonShape::Square)
                    .on_click(move |_, _, cx| workspace.update(cx, |w, cx| w.toggle_dock(cx))),
            )
    }
}
