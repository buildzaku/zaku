use gpui::{
    App, ClipboardItem, Context, ElementId, Entity, IntoElement, RenderOnce, SharedString, Window,
};
use std::time::{Duration, Instant};

use crate::{
    ButtonCommon, Clickable, Color, IconAsset, IconButton, IconButtonShape, IconSize, Tooltip,
};

const COPIED_STATE_DURATION: Duration = Duration::from_secs(2);

struct CopyButtonState {
    copied_at: Option<Instant>,
}

impl CopyButtonState {
    fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self { copied_at: None }
    }

    fn is_copied(&self) -> bool {
        self.copied_at
            .is_some_and(|copied_at| copied_at.elapsed() < COPIED_STATE_DURATION)
    }

    fn mark_copied(&mut self) {
        self.copied_at = Some(Instant::now());
    }
}

#[derive(IntoElement)]
pub struct CopyButton {
    id: ElementId,
    message: SharedString,
}

impl CopyButton {
    pub fn new(id: impl Into<ElementId>, message: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            message: message.into(),
        }
    }
}

impl RenderOnce for CopyButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let message = self.message;
        let state: Entity<CopyButtonState> =
            window.use_keyed_state(self.id.clone(), cx, CopyButtonState::new);

        let (icon, color, tooltip) = if state.read(cx).is_copied() {
            (IconAsset::Check, Color::Success, "Copied!")
        } else {
            (IconAsset::Copy, Color::Muted, "Copy")
        };

        IconButton::new(self.id, icon)
            .shape(IconButtonShape::Square)
            .icon_color(color)
            .icon_size(IconSize::Small)
            .tooltip(Tooltip::text(tooltip))
            .on_click(move |_, _, cx| {
                state.update(cx, |state, _| {
                    state.mark_copied();
                });
                cx.stop_propagation();
                cx.write_to_clipboard(ClipboardItem::new_string(message.to_string()));

                let state_id = state.entity_id();
                cx.spawn(async move |cx| {
                    cx.background_executor().timer(COPIED_STATE_DURATION).await;
                    cx.update(|cx| {
                        cx.notify(state_id);
                    });
                })
                .detach();
            })
    }
}
