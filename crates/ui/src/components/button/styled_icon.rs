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
}

impl StyledIcon {
    pub fn new(icon: IconName) -> Self {
        Self {
            icon,
            size: IconSize::default(),
            color: Color::Default,
            disabled: false,
            selected: false,
            selected_icon: None,
            selected_icon_color: None,
        }
    }

    pub fn size(mut self, size: IconSize) -> Self {
        self.size = size;
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn toggle_state(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn selected_icon(mut self, icon: impl Into<Option<IconName>>) -> Self {
        self.selected_icon = icon.into();
        self
    }

    pub fn selected_icon_color(mut self, color: impl Into<Option<Color>>) -> Self {
        self.selected_icon_color = color.into();
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
        let icon = self
            .selected_icon
            .filter(|_| self.selected)
            .unwrap_or(self.icon);

        let icon_color = if self.disabled {
            Color::Disabled
        } else if self.selected {
            self.selected_icon_color.unwrap_or(Color::Selected)
        } else {
            self.color
        };

        Icon::new(icon).size(self.size).color(icon_color)
    }
}
