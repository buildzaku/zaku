use gpui::{
    App, ClickEvent, DefiniteLength, Div, ElementId, FontWeight, Hsla, Rems, SharedString, Window,
    div, prelude::*, relative, rgb,
};

use icons::IconName;

use crate::{ButtonCommon, Clickable, Disableable, FixedWidth, Icon, IconSize, rems_from_px};

pub struct ButtonColor {
    pub bg: Hsla,
    pub text: Hsla,
    pub hover_bg: Hsla,
    pub active_bg: Hsla,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    #[default]
    Subtle,
    Solid,
    Accent,
    Outline,
    Ghost,
}

impl ButtonVariant {
    pub fn colors(&self) -> ButtonColor {
        match self {
            ButtonVariant::Subtle => ButtonColor {
                bg: gpui::transparent_black(),
                text: rgb(0xffffff).into(),
                hover_bg: rgb(0x292929).into(),
                active_bg: rgb(0x404040).into(),
            },
            ButtonVariant::Solid => ButtonColor {
                bg: rgb(0x292929).into(),
                text: rgb(0xffffff).into(),
                hover_bg: rgb(0x292929).into(),
                active_bg: rgb(0x404040).into(),
            },
            ButtonVariant::Accent => ButtonColor {
                bg: rgb(0x41d4dc).into(),
                text: rgb(0x043f58).into(),
                hover_bg: rgb(0x3dc9d1).into(),
                active_bg: rgb(0x3dc9d1).into(),
            },
            ButtonVariant::Outline => ButtonColor {
                bg: gpui::transparent_black(),
                text: rgb(0xffffff).into(),
                hover_bg: rgb(0x292929).into(),
                active_bg: rgb(0x404040).into(),
            },
            ButtonVariant::Ghost => ButtonColor {
                bg: gpui::transparent_black(),
                text: rgb(0xffffff).into(),
                hover_bg: gpui::transparent_black(),
                active_bg: gpui::transparent_black(),
            },
        }
    }
}

#[derive(Default, PartialEq, Clone, Copy)]
pub enum ButtonSize {
    Large,
    Medium,
    #[default]
    Default,
    Compact,
    None,
}

impl ButtonSize {
    pub fn rems(self) -> Rems {
        match self {
            ButtonSize::Large => rems_from_px(32.),
            ButtonSize::Medium => rems_from_px(28.),
            ButtonSize::Default => rems_from_px(22.),
            ButtonSize::Compact => rems_from_px(18.),
            ButtonSize::None => rems_from_px(16.),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub enum IconPosition {
    #[default]
    Start,
    End,
}

#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    variant: ButtonVariant,
    label: SharedString,
    base: Div,
    width: Option<DefiniteLength>,
    height: Option<DefiniteLength>,
    size: ButtonSize,
    disabled: bool,
    icon: Option<IconName>,
    icon_position: Option<IconPosition>,
    font_weight: Option<FontWeight>,
    on_click: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
}

impl Button {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            variant: ButtonVariant::default(),
            label: label.into(),
            base: div(),
            width: None,
            height: None,
            size: ButtonSize::default(),
            disabled: false,
            icon: None,
            icon_position: None,
            font_weight: None,
            on_click: None,
        }
    }

    pub fn height(mut self, height: DefiniteLength) -> Self {
        self.height = Some(height);
        self
    }

    pub fn icon(mut self, icon: impl Into<Option<IconName>>) -> Self {
        self.icon = icon.into();
        self
    }

    pub fn icon_position(mut self, icon_position: impl Into<Option<IconPosition>>) -> Self {
        self.icon_position = icon_position.into();
        self
    }

    pub fn font_weight(mut self, font_weight: FontWeight) -> Self {
        self.font_weight = Some(font_weight);
        self
    }
}

impl Disableable for Button {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Clickable for Button {
    fn on_click(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl FixedWidth for Button {
    fn width(mut self, width: impl Into<DefiniteLength>) -> Self {
        self.width = Some(width.into());
        self
    }

    fn full_width(mut self) -> Self {
        self.width = Some(relative(1.));
        self
    }
}

impl ButtonCommon for Button {
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

impl RenderOnce for Button {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let colors = self.variant.colors();
        let icon_size = match self.size {
            ButtonSize::Large => IconSize::Medium,
            ButtonSize::Medium => IconSize::Small,
            ButtonSize::Default => IconSize::Small,
            ButtonSize::Compact => IconSize::XSmall,
            ButtonSize::None => IconSize::XSmall,
        };
        let (padding_x, gap) = match self.size {
            ButtonSize::Large => (rems_from_px(12.), rems_from_px(6.)),
            ButtonSize::Medium => (rems_from_px(10.), rems_from_px(5.)),
            ButtonSize::Default => (rems_from_px(8.), rems_from_px(4.)),
            ButtonSize::Compact => (rems_from_px(6.), rems_from_px(3.)),
            ButtonSize::None => (rems_from_px(4.), rems_from_px(2.)),
        };
        let icon_position = self.icon_position.unwrap_or(IconPosition::Start);

        self.base
            .id(self.id)
            .flex()
            .justify_center()
            .items_center()
            .gap(gap)
            .h(self.height.unwrap_or(self.size.rems().into()))
            .when_some(self.width, |this, width| this.w(width).justify_center())
            .px(padding_x)
            .rounded_md()
            .bg(colors.bg)
            .text_color(colors.text)
            .when_some(self.font_weight, |this, weight| this.font_weight(weight))
            .when(self.disabled, |this| this.opacity(0.4).cursor_not_allowed())
            .when(!self.disabled, |this| {
                this.cursor_pointer()
                    .hover(|style| style.bg(colors.hover_bg))
                    .active(|style| style.bg(colors.active_bg))
            })
            .when(self.variant == ButtonVariant::Outline, |this| {
                this.border_1().border_color(rgb(0x545454))
            })
            .when(
                self.icon.is_some() && icon_position == IconPosition::Start,
                |this| {
                    this.children(
                        self.icon
                            .map(|icon| Icon::new(icon).size(icon_size).color(colors.text)),
                    )
                },
            )
            .child(self.label)
            .when(
                self.icon.is_some() && icon_position == IconPosition::End,
                |this| {
                    this.children(
                        self.icon
                            .map(|icon| Icon::new(icon).size(icon_size).color(colors.text)),
                    )
                },
            )
    }
}
