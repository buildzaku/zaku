use gpui::{App, IntoElement, RenderOnce, Window};

use icons::IconName;

use crate::{Color, Icon, IconSize};

use crate::Disableable;

#[derive(IntoElement)]
pub(super) struct StyledIcon {
    icon: IconName,
    size: IconSize,
    color: Color,
    disabled: bool,
    selected: bool,
    selected_icon: Option<IconName>,
    selected_icon_color: Option<Color>,
    hover_icon_color: Option<Color>,
}

impl StyledIcon {
    pub(super) fn new(icon: IconName) -> Self {
        Self {
            icon,
            size: IconSize::default(),
            color: Color::Default,
            disabled: false,
            selected: false,
            selected_icon: None,
            selected_icon_color: None,
            hover_icon_color: None,
        }
    }

    pub(super) fn size(mut self, size: IconSize) -> Self {
        self.size = size;
        self
    }

    pub(super) fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub(super) fn toggle_state(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub(super) fn selected_icon(mut self, icon: impl Into<Option<IconName>>) -> Self {
        self.selected_icon = icon.into();
        self
    }

    pub(super) fn selected_icon_color(mut self, color: impl Into<Option<Color>>) -> Self {
        self.selected_icon_color = color.into();
        self
    }

    pub(super) fn hover_icon_color(mut self, color: impl Into<Option<Color>>) -> Self {
        self.hover_icon_color = color.into();
        self
    }
}

impl Disableable for StyledIcon {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl RenderOnce for StyledIcon {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let selected = self.selected && !self.disabled;
        let icon = self.selected_icon.filter(|_| selected).unwrap_or(self.icon);

        let icon_color = if selected {
            self.selected_icon_color.unwrap_or(Color::Selected)
        } else {
            self.color
        };

        let hover_icon_color = if self.disabled {
            None
        } else {
            self.hover_icon_color
        };

        Icon::new(icon)
            .size(self.size)
            .color(icon_color)
            .group_hover_color("button-like", hover_icon_color)
    }
}
