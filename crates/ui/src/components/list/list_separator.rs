use gpui::{App, Window, prelude::*};

use crate::prelude::*;

#[derive(IntoElement)]
pub struct ListSeparator;

impl RenderOnce for ListSeparator {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        gpui::div()
            .h_px()
            .w_full()
            .my(DynamicSpacing::Base06.rems(cx))
            .bg(cx.theme().colors().border_variant)
    }
}
