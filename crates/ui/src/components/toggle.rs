use gpui::{AnyView, App, ClickEvent, ElementId, Hsla, Pixels, SharedString, Window, prelude::*};

use theme::ActiveTheme;

use crate::{
    Color, DynamicSpacing, Icon, IconName, IconSize, Label, LabelCommon, LabelSize, ToggleState,
    h_flex,
};

pub fn checkbox(id: impl Into<ElementId>, toggle_state: ToggleState) -> Checkbox {
    Checkbox::new(id, toggle_state)
}

#[derive(IntoElement)]
pub struct Checkbox {
    id: ElementId,
    toggle_state: ToggleState,
    disabled: bool,
    label: Option<SharedString>,
    label_size: LabelSize,
    label_color: Color,
    tooltip: Option<Box<dyn Fn(&mut Window, &mut App) -> AnyView>>,
    on_click: Option<Box<dyn Fn(&ToggleState, &ClickEvent, &mut Window, &mut App) + 'static>>,
}

impl Checkbox {
    pub fn new(id: impl Into<ElementId>, checked: ToggleState) -> Self {
        Self {
            id: id.into(),
            toggle_state: checked,
            disabled: false,
            label: None,
            label_size: LabelSize::Default,
            label_color: Color::Muted,
            tooltip: None,
            on_click: None,
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ToggleState, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(move |state, _, window, cx| {
            handler(state, window, cx)
        }));
        self
    }

    pub fn on_click_ext(
        mut self,
        handler: impl Fn(&ToggleState, &ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }

    pub fn tooltip(mut self, tooltip: impl Fn(&mut Window, &mut App) -> AnyView + 'static) -> Self {
        self.tooltip = Some(Box::new(tooltip));
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn label_size(mut self, size: LabelSize) -> Self {
        self.label_size = size;
        self
    }

    pub fn label_color(mut self, color: Color) -> Self {
        self.label_color = color;
        self
    }

    fn bg_color(&self, cx: &App) -> Hsla {
        if self.disabled {
            cx.theme().colors().element_disabled
        } else if self.toggle_state == ToggleState::Unselected {
            cx.theme().colors().ghost_element_background
        } else {
            cx.theme().colors().text_accent.opacity(0.8)
        }
    }

    fn border_color(&self, cx: &App) -> Hsla {
        if self.disabled {
            return cx.theme().colors().border_disabled;
        }

        if self.toggle_state == ToggleState::Unselected {
            cx.theme().colors().border
        } else {
            gpui::transparent_black()
        }
    }

    pub fn container_size() -> Pixels {
        gpui::px(20.0)
    }
}

impl RenderOnce for Checkbox {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let color = if self.disabled {
            Color::Disabled
        } else {
            Color::Custom(cx.theme().colors().panel_background)
        };

        let icon = match self.toggle_state {
            ToggleState::Selected => Some(
                Icon::new(IconName::Check)
                    .size(IconSize::Small)
                    .color(color),
            ),
            ToggleState::Indeterminate => {
                Some(Icon::new(IconName::Dash).size(IconSize::Small).color(color))
            }
            ToggleState::Unselected => None,
        };

        let bg_color = self.bg_color(cx);
        let border_color = self.border_color(cx);
        let size = Self::container_size();

        let checkbox = h_flex()
            .id(self.id.clone())
            .size(size)
            .justify_center()
            .child(
                gpui::div()
                    .flex()
                    .flex_none()
                    .justify_center()
                    .items_center()
                    .m_1()
                    .size_4()
                    .rounded_sm()
                    .bg(bg_color)
                    .border_1()
                    .border_color(border_color)
                    .when(self.disabled, |this| this.cursor_not_allowed())
                    .children(icon),
            );

        h_flex()
            .id(self.id)
            .map(|this| {
                if self.disabled {
                    this.cursor_not_allowed()
                } else {
                    this.cursor_pointer()
                }
            })
            .gap(DynamicSpacing::Base06.rems(cx))
            .child(checkbox)
            .when_some(self.label, |this, label| {
                this.child(
                    Label::new(label)
                        .color(self.label_color)
                        .size(self.label_size),
                )
            })
            .when_some(self.tooltip, |this, tooltip| {
                this.tooltip(move |window, cx| tooltip(window, cx))
            })
            .when_some(
                self.on_click.filter(|_| !self.disabled),
                |this, on_click| {
                    this.on_click(move |click, window, cx| {
                        on_click(&self.toggle_state.inverse(), click, window, cx)
                    })
                },
            )
    }
}
