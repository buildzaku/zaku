use gpui::{
    AnyElement, AnyView, App, ClickEvent, CursorStyle, DefiniteLength, Div, ElementId, Hsla,
    MouseButton, SharedString, Window, prelude::*,
};
use smallvec::SmallVec;

use super::{ButtonSize, ButtonVariant};

use crate::{
    Clickable, Disableable, DynamicSpacing, FixedWidth, TOOLTIP_SHOW_DELAY, Toggleable,
    VisibleOnHover,
};

pub trait SelectableButton: Toggleable {
    fn selected_background(self, background: Hsla) -> Self;
}

pub trait ButtonCommon: Clickable + Disableable {
    fn id(&self) -> &ElementId;

    fn tooltip(self, tooltip: impl Fn(&mut Window, &mut App) -> AnyView + 'static) -> Self;

    fn variant(self, variant: ButtonVariant) -> Self;

    fn size(self, size: ButtonSize) -> Self;
}

#[derive(IntoElement)]
pub struct ButtonLike {
    base: Div,
    id: ElementId,
    variant: ButtonVariant,
    pub(super) disabled: bool,
    pub(super) selected: bool,
    selected_background: Option<Hsla>,
    cursor_style: CursorStyle,
    width: Option<DefiniteLength>,
    height: Option<DefiniteLength>,
    size: ButtonSize,
    tab_index: Option<isize>,
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
            selected: false,
            selected_background: None,
            cursor_style: CursorStyle::PointingHand,
            width: None,
            height: None,
            size: ButtonSize::Default,
            tab_index: None,
            tooltip: None,
            on_click: None,
            children: SmallVec::new(),
        }
    }

    pub fn height(mut self, height: DefiniteLength) -> Self {
        self.height = Some(height);
        self
    }

    pub fn tab_index(mut self, tab_index: isize) -> Self {
        self.tab_index = Some(tab_index);
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

impl Toggleable for ButtonLike {
    fn toggle_state(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl SelectableButton for ButtonLike {
    fn selected_background(mut self, background: Hsla) -> Self {
        self.selected_background = Some(background);
        self
    }
}

impl Clickable for ButtonLike {
    fn on_click(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }

    fn cursor_style(mut self, cursor_style: CursorStyle) -> Self {
        self.cursor_style = cursor_style;
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
        self.children.extend(elements);
    }
}

impl RenderOnce for ButtonLike {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let variant = self.variant;
        let style = variant.enabled(cx);
        let hovered_style = variant.hovered(cx);
        let background = if self.selected {
            self.selected_background.unwrap_or(style.background)
        } else {
            style.background
        };
        let is_outlined = matches!(
            self.variant,
            ButtonVariant::Outline | ButtonVariant::OutlinedGhost
        );

        self.base
            .id(self.id.clone())
            .group("button-like")
            .when_some(self.tooltip, |this, tooltip| {
                this.tooltip_show_delay(TOOLTIP_SHOW_DELAY)
                    .tooltip(move |window, cx| tooltip(window, cx))
            })
            .when_some(self.tab_index, |this, tab_index| this.tab_index(tab_index))
            .flex_none()
            .flex()
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
                this.border_1().border_color(style.border_color)
            })
            .border_color(style.border_color)
            .bg(background)
            .text_color(style.text_color)
            .when(self.disabled, |this| {
                this.cursor(CursorStyle::Arrow).opacity(0.4)
            })
            .when(!self.disabled, |this| {
                this.cursor(self.cursor_style)
                    .hover(|style| style.bg(hovered_style.background))
            })
            .when_some(
                self.on_click.filter(|_| !self.disabled),
                |this, on_click| {
                    this.on_mouse_down(MouseButton::Left, |_, window, _cx| {
                        window.prevent_default();
                    })
                    .on_click(move |event, window, cx| {
                        cx.stop_propagation();
                        on_click(event, window, cx);
                    })
                },
            )
            .children(self.children)
    }
}

impl VisibleOnHover for ButtonLike {
    fn visible_on_hover(mut self, group_name: impl Into<SharedString>) -> Self {
        self.base = self.base.visible_on_hover(group_name);
        self
    }
}
