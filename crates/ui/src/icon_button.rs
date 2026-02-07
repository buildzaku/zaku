use gpui::{App, ClickEvent, DefiniteLength, Div, ElementId, Window, prelude::*};

use component::{Component, ComponentScope};
use icons::IconName;
use theme::ActiveTheme;
use ui_macros::RegisterComponent;

use crate::{
    ButtonCommon, ButtonSize, ButtonVariant, Clickable, Disableable, DynamicSpacing, FixedWidth,
    Icon, IconSize,
};

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub enum ButtonShape {
    Square,
    #[default]
    Wide,
}

#[derive(IntoElement, RegisterComponent)]
pub struct IconButton {
    id: ElementId,
    variant: ButtonVariant,
    base: Div,
    width: Option<DefiniteLength>,
    height: Option<DefiniteLength>,
    size: ButtonSize,
    shape: ButtonShape,
    disabled: bool,
    icon: IconName,
    on_click: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
}

impl IconButton {
    pub fn new(id: impl Into<ElementId>, icon: IconName) -> Self {
        Self {
            id: id.into(),
            variant: ButtonVariant::default(),
            base: gpui::div(),
            width: None,
            height: None,
            size: ButtonSize::default(),
            shape: ButtonShape::default(),
            disabled: false,
            icon,
            on_click: None,
        }
    }

    pub fn height(mut self, height: DefiniteLength) -> Self {
        self.height = Some(height);
        self
    }

    pub fn shape(mut self, shape: ButtonShape) -> Self {
        self.shape = shape;
        self
    }
}

impl Disableable for IconButton {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Clickable for IconButton {
    fn on_click(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl FixedWidth for IconButton {
    fn width(mut self, width: impl Into<DefiniteLength>) -> Self {
        self.width = Some(width.into());
        self
    }

    fn full_width(mut self) -> Self {
        self.width = Some(gpui::relative(1.));
        self
    }
}

impl ButtonCommon for IconButton {
    fn id(&self) -> &ElementId {
        &self.id
    }

    fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }
}

impl RenderOnce for IconButton {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme_colors = cx.theme().colors();
        let colors = self.variant.colors(cx);
        let icon_size = match self.size {
            ButtonSize::Large => IconSize::Medium,
            ButtonSize::Medium => IconSize::Small,
            ButtonSize::Default => IconSize::Small,
            ButtonSize::Compact => IconSize::XSmall,
            ButtonSize::None => IconSize::XSmall,
        };

        let icon_color = if self.disabled {
            theme_colors.icon_disabled
        } else {
            theme_colors.icon
        };

        self.base
            .id(self.id)
            .flex()
            .justify_center()
            .items_center()
            .gap(DynamicSpacing::Base04.rems(cx))
            .when(self.shape == ButtonShape::Square, |this| {
                let size = self.size.rems();
                this.w(size).h(size)
            })
            .when(self.shape == ButtonShape::Wide, |this| {
                this.h(self.height.unwrap_or(self.size.rems().into()))
                    .when_some(self.width, |this, width| this.w(width).justify_center())
            })
            .map(|this| match self.size {
                ButtonSize::Large | ButtonSize::Medium => this.px(DynamicSpacing::Base08.rems(cx)),
                ButtonSize::Default | ButtonSize::Compact => {
                    this.px(DynamicSpacing::Base04.rems(cx))
                }
                ButtonSize::None => this.px_px(),
            })
            .rounded_sm()
            .bg(colors.bg)
            .text_color(colors.text)
            .when(self.disabled, |this| this.cursor_not_allowed())
            .when(!self.disabled, |this| {
                this.cursor_pointer()
                    .hover(|style| style.bg(colors.hover_bg))
                    .active(|style| style.bg(colors.active_bg))
            })
            .when(self.variant == ButtonVariant::Outline, |this| {
                this.border_1().border_color(theme_colors.border_variant)
            })
            .when_some(
                self.on_click.filter(|_| !self.disabled),
                |this, on_click| {
                    this.on_click(move |event, window, cx| on_click(event, window, cx))
                },
            )
            .child(Icon::new(self.icon).size(icon_size).color(icon_color))
    }
}

impl Component for IconButton {
    fn scope() -> ComponentScope {
        ComponentScope::Input
    }

    fn sort_name() -> &'static str {
        "ButtonIcon"
    }
}
