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
pub enum ButtonShape {
    Square,
    #[default]
    Wide,
}

#[derive(IntoElement, RegisterComponent)]
pub struct IconButton {
    base: ButtonLike,
    id: ElementId,
    size: ButtonSize,
    shape: ButtonShape,
    disabled: bool,
    selected: bool,
    icon: IconName,
    selected_icon: Option<IconName>,
    icon_color: Color,
    selected_icon_color: Option<Color>,
}

impl IconButton {
    pub fn new(id: impl Into<ElementId>, icon: IconName) -> Self {
        let id = id.into();
        Self {
            base: ButtonLike::new(id.clone()),
            id,
            size: ButtonSize::default(),
            shape: ButtonShape::default(),
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

    pub fn height(mut self, height: DefiniteLength) -> Self {
        self.base = self.base.height(height);
        self
    }

    pub fn shape(mut self, shape: ButtonShape) -> Self {
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
        &self.id
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
        self.size = size;
        self.base = self.base.size(size);
        self
    }
}

impl RenderOnce for IconButton {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let icon_size = match self.size {
            ButtonSize::Large => IconSize::Medium,
            ButtonSize::Medium => IconSize::Small,
            ButtonSize::Default => IconSize::Small,
            ButtonSize::Compact => IconSize::XSmall,
            ButtonSize::None => IconSize::XSmall,
        };

        self.base
            .map(|this| match self.shape {
                ButtonShape::Square => {
                    let size = self.size.rems();
                    this.width(size).height(size.into())
                }
                ButtonShape::Wide => this,
            })
            .child(
                StyledIcon::new(self.icon)
                    .size(icon_size)
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
