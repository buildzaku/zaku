use gpui::{AnyView, App, ClickEvent, DefiniteLength, ElementId, Window, prelude::*};

use component::{Component, ComponentScope};
use icons::IconName;
use ui_macros::RegisterComponent;

use crate::{
    ButtonCommon, ButtonLike, ButtonSize, ButtonVariant, Clickable, Color, Disableable, FixedWidth,
    IconSize,
};

use super::styled_icon::StyledIcon;

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub enum IconButtonShape {
    Square,
    #[default]
    Wide,
}

#[derive(IntoElement, RegisterComponent)]
pub struct IconButton {
    base: ButtonLike,
    shape: IconButtonShape,
    icon_size: IconSize,
    disabled: bool,
    selected: bool,
    icon: IconName,
    selected_icon: Option<IconName>,
    icon_color: Color,
    selected_icon_color: Option<Color>,
}

impl IconButton {
    pub fn new(id: impl Into<ElementId>, icon: IconName) -> Self {
        Self {
            base: ButtonLike::new(id),
            shape: IconButtonShape::default(),
            icon_size: IconSize::default(),
            disabled: false,
            selected: false,
            icon,
            selected_icon: None,
            icon_color: Color::Default,
            selected_icon_color: None,
        }
    }

    pub fn toggle_state(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn icon_size(mut self, icon_size: IconSize) -> Self {
        self.icon_size = icon_size;
        self
    }

    pub fn height(mut self, height: DefiniteLength) -> Self {
        self.base = self.base.height(height);
        self
    }

    pub fn shape(mut self, shape: IconButtonShape) -> Self {
        self.shape = shape;
        self
    }

    pub fn icon_color(mut self, icon_color: Color) -> Self {
        self.icon_color = icon_color;
        self
    }

    pub fn selected_icon_color(mut self, selected_icon_color: impl Into<Option<Color>>) -> Self {
        self.selected_icon_color = selected_icon_color.into();
        self
    }

    pub fn selected_icon(mut self, selected_icon: impl Into<Option<IconName>>) -> Self {
        self.selected_icon = selected_icon.into();
        self
    }
}

impl Disableable for IconButton {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self.base = self.base.disabled(disabled);
        self
    }
}

impl Clickable for IconButton {
    fn on_click(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.base = self.base.on_click(handler);
        self
    }
}

impl FixedWidth for IconButton {
    fn width(mut self, width: impl Into<DefiniteLength>) -> Self {
        self.base = self.base.width(width);
        self
    }

    fn full_width(mut self) -> Self {
        self.base = self.base.full_width();
        self
    }
}

impl ButtonCommon for IconButton {
    fn id(&self) -> &ElementId {
        self.base.id()
    }

    fn tooltip(mut self, tooltip: impl Fn(&mut Window, &mut App) -> AnyView + 'static) -> Self {
        self.base = self.base.tooltip(tooltip);
        self
    }

    fn variant(mut self, variant: ButtonVariant) -> Self {
        self.base = self.base.variant(variant);
        self
    }

    fn size(mut self, size: ButtonSize) -> Self {
        self.base = self.base.size(size);
        self
    }
}

impl RenderOnce for IconButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        self.base
            .map(|this| match self.shape {
                IconButtonShape::Square => {
                    let size = self.icon_size.square(window, cx);
                    this.width(size).height(size.into())
                }
                IconButtonShape::Wide => this,
            })
            .child(
                StyledIcon::new(self.icon)
                    .size(self.icon_size)
                    .color(self.icon_color)
                    .selected_icon(self.selected_icon)
                    .selected_icon_color(self.selected_icon_color)
                    .disabled(self.disabled)
                    .toggle_state(self.selected),
            )
    }
}

impl Component for IconButton {
    fn scope() -> ComponentScope {
        ComponentScope::Input
    }
}
