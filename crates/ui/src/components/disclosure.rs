use gpui::{App, ClickEvent, CursorStyle, ElementId, SharedString, Window, prelude::*};
use std::sync::Arc;

use crate::{Color, IconButton, IconButtonShape, IconName, IconSize, prelude::*};

#[derive(IntoElement)]
pub struct Disclosure {
    id: ElementId,
    is_open: bool,
    selected: bool,
    disabled: bool,
    on_toggle_expanded: Option<Arc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    cursor_style: CursorStyle,
    opened_icon: IconName,
    closed_icon: IconName,
    visible_on_hover: Option<SharedString>,
}

impl Disclosure {
    pub fn new(id: impl Into<ElementId>, is_open: bool) -> Self {
        Self {
            id: id.into(),
            is_open,
            selected: false,
            disabled: false,
            on_toggle_expanded: None,
            cursor_style: CursorStyle::PointingHand,
            opened_icon: IconName::CaretDown,
            closed_icon: IconName::CaretRight,
            visible_on_hover: None,
        }
    }

    pub fn on_toggle_expanded(
        mut self,
        handler: impl Into<Option<Arc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>>,
    ) -> Self {
        self.on_toggle_expanded = handler.into();
        self
    }

    pub fn opened_icon(mut self, icon: IconName) -> Self {
        self.opened_icon = icon;
        self
    }

    pub fn closed_icon(mut self, icon: IconName) -> Self {
        self.closed_icon = icon;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Toggleable for Disclosure {
    fn toggle_state(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl Clickable for Disclosure {
    fn on_click(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_toggle_expanded = Some(Arc::new(handler));
        self
    }

    fn cursor_style(mut self, cursor_style: gpui::CursorStyle) -> Self {
        self.cursor_style = cursor_style;
        self
    }
}

impl VisibleOnHover for Disclosure {
    fn visible_on_hover(mut self, group_name: impl Into<SharedString>) -> Self {
        self.visible_on_hover = Some(group_name.into());
        self
    }
}

impl RenderOnce for Disclosure {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        IconButton::new(
            self.id,
            match self.is_open {
                true => self.opened_icon,
                false => self.closed_icon,
            },
        )
        .shape(IconButtonShape::Square)
        .icon_color(Color::Muted)
        .icon_size(IconSize::Small)
        .disabled(self.disabled)
        .toggle_state(self.selected)
        .when_some(self.visible_on_hover.clone(), |this, group_name| {
            this.visible_on_hover(group_name)
        })
        .when_some(self.on_toggle_expanded, move |this, on_toggle| {
            this.on_click(move |event, window, cx| on_toggle(event, window, cx))
        })
    }
}
