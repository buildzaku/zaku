use gpui::{
    AnyElement, AnyView, App, ClickEvent, DefiniteLength, Div, ElementId, MouseButton, Window,
    prelude::*,
};
use smallvec::SmallVec;
use theme::ActiveTheme;

use crate::{ButtonColor, ButtonSize, ButtonVariant, DynamicSpacing};

pub trait Clickable: Sized {
    fn on_click(self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self;
}

pub trait Disableable: Sized {
    fn disabled(self, disabled: bool) -> Self;
}

pub trait ButtonCommon: Clickable + Disableable {
    fn id(&self) -> &ElementId;

    fn tooltip(self, tooltip: impl Fn(&mut Window, &mut App) -> AnyView + 'static) -> Self;

    fn variant(self, variant: ButtonVariant) -> Self;

    fn size(self, size: ButtonSize) -> Self;
}

pub trait FixedWidth: Sized {
    fn width(self, width: impl Into<DefiniteLength>) -> Self;

    fn full_width(self) -> Self;
}

#[derive(IntoElement)]
pub struct ButtonLike {
    base: Div,
    id: ElementId,
    variant: ButtonVariant,
    disabled: bool,
    width: Option<DefiniteLength>,
    height: Option<DefiniteLength>,
    size: ButtonSize,
    tooltip: Option<Box<dyn Fn(&mut Window, &mut App) -> AnyView + 'static>>,
    on_click: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    children: SmallVec<[AnyElement; 2]>,
}

impl ButtonLike {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            base: gpui::div(),
            id: id.into(),
            variant: ButtonVariant::default(),
            disabled: false,
            width: None,
            height: None,
            size: ButtonSize::Default,
            tooltip: None,
            on_click: None,
            children: SmallVec::new(),
        }
    }

    pub fn height(mut self, height: DefiniteLength) -> Self {
        self.height = Some(height);
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.base = self.base.opacity(opacity);
        self
    }
}

impl Disableable for ButtonLike {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Clickable for ButtonLike {
    fn on_click(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl FixedWidth for ButtonLike {
    fn width(mut self, width: impl Into<DefiniteLength>) -> Self {
        self.width = Some(width.into());
        self
    }

    fn full_width(mut self) -> Self {
        self.width = Some(gpui::relative(1.));
        self
    }
}

impl ButtonCommon for ButtonLike {
    fn id(&self) -> &ElementId {
        &self.id
    }

    fn tooltip(mut self, tooltip: impl Fn(&mut Window, &mut App) -> AnyView + 'static) -> Self {
        self.tooltip = Some(Box::new(tooltip));
        self
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

impl ParentElement for ButtonLike {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

impl RenderOnce for ButtonLike {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme_colors = cx.theme().colors();
        let mut colors = self.variant.colors(cx);
        let is_outlined = matches!(self.variant, ButtonVariant::Outline);

        if self.disabled {
            colors = match self.variant {
                ButtonVariant::Subtle => ButtonColor {
                    bg: theme_colors.ghost_element_disabled,
                    text: theme_colors.text_disabled,
                    hover_bg: theme_colors.ghost_element_disabled,
                    active_bg: theme_colors.ghost_element_disabled,
                },
                ButtonVariant::Solid | ButtonVariant::Outline | ButtonVariant::Accent => {
                    ButtonColor {
                        bg: theme_colors.element_disabled,
                        text: theme_colors.text_disabled,
                        hover_bg: theme_colors.element_disabled,
                        active_bg: theme_colors.element_disabled,
                    }
                }
                ButtonVariant::Ghost => ButtonColor {
                    bg: gpui::transparent_black(),
                    text: theme_colors.text_disabled,
                    hover_bg: gpui::transparent_black(),
                    active_bg: gpui::transparent_black(),
                },
            };
        }

        self.base
            .id(self.id.clone())
            .when_some(self.tooltip, |this, tooltip| {
                this.tooltip(move |window, cx| tooltip(window, cx))
            })
            .flex()
            .flex_row()
            .items_center()
            .justify_center()
            .h(self.height.unwrap_or(self.size.rems().into()))
            .when_some(self.width, |this, width| this.w(width))
            .text_center()
            .gap(DynamicSpacing::Base04.rems(cx))
            .map(|this| match self.size {
                ButtonSize::Large | ButtonSize::Medium => this.px(DynamicSpacing::Base08.rems(cx)),
                ButtonSize::Default | ButtonSize::Compact => {
                    this.px(DynamicSpacing::Base04.rems(cx))
                }
                ButtonSize::None => this.px_px(),
            })
            .rounded_sm()
            .when(is_outlined, |this| {
                this.border_1().border_color(theme_colors.border_variant)
            })
            .bg(colors.bg)
            .text_color(colors.text)
            .when(self.disabled, |this| this.cursor_not_allowed())
            .when(!self.disabled, |this| {
                this.cursor_pointer()
                    .hover(|style| style.bg(colors.hover_bg))
                    .active(|style| style.bg(colors.active_bg))
            })
            .when_some(
                self.on_click.filter(|_| !self.disabled),
                |this, on_click| {
                    this.on_mouse_down(MouseButton::Left, |_, window, _cx| {
                        window.prevent_default();
                    })
                    .on_click(move |event, window, cx| {
                        cx.stop_propagation();
                        on_click(event, window, cx)
                    })
                },
            )
            .children(self.children)
    }
}
