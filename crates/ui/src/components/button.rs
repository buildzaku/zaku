mod base;
mod icon_button;
mod link_button;
mod styled_icon;

pub use base::*;
pub use icon_button::*;
pub use link_button::*;

use gpui::{
    AnyView, App, ClickEvent, CursorStyle, DefiniteLength, Div, ElementId, FontWeight, Hsla,
    MouseButton, Rems, SharedString, Window, prelude::*,
};

use icons::IconName;
use theme::ActiveTheme;

use crate::{
    Clickable, Color, Disableable, DynamicSpacing, FixedWidth, Icon, IconSize, StyledTypography,
    TOOLTIP_SHOW_DELAY, TextSize, Toggleable,
};

#[derive(Debug, Clone)]
pub struct ButtonStyle {
    pub background: Hsla,
    pub border_color: Hsla,
    pub text_color: Hsla,
    pub icon_color: Hsla,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TintColor {
    #[default]
    Info,
    Error,
    Warning,
    Success,
}

impl TintColor {
    fn button_style(self, cx: &mut App) -> ButtonStyle {
        match self {
            TintColor::Info => ButtonStyle {
                background: cx.theme().status().info_background,
                border_color: cx.theme().status().info_border,
                text_color: cx.theme().colors().text,
                icon_color: cx.theme().colors().text,
            },
            TintColor::Error => ButtonStyle {
                background: cx.theme().status().error_background,
                border_color: cx.theme().status().error_border,
                text_color: cx.theme().colors().text,
                icon_color: cx.theme().colors().text,
            },
            TintColor::Warning => ButtonStyle {
                background: cx.theme().status().warning_background,
                border_color: cx.theme().status().warning_border,
                text_color: cx.theme().colors().text,
                icon_color: cx.theme().colors().text,
            },
            TintColor::Success => ButtonStyle {
                background: cx.theme().status().success_background,
                border_color: cx.theme().status().success_border,
                text_color: cx.theme().colors().text,
                icon_color: cx.theme().colors().text,
            },
        }
    }
}

impl From<TintColor> for Color {
    fn from(tint: TintColor) -> Self {
        match tint {
            TintColor::Info => Color::Info,
            TintColor::Error => Color::Error,
            TintColor::Warning => Color::Warning,
            TintColor::Success => Color::Success,
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    #[default]
    Subtle,
    Solid,
    Outline,
    OutlinedGhost,
    Ghost,
    Custom {
        background: Hsla,
        foreground: Hsla,
        hover_background: Hsla,
        border: Hsla,
    },
    Tinted(TintColor),
}

impl ButtonVariant {
    pub fn enabled(self, cx: &mut App) -> ButtonStyle {
        match self {
            ButtonVariant::Subtle => {
                let colors = cx.theme().colors();
                ButtonStyle {
                    background: colors.button_secondary_background,
                    border_color: gpui::transparent_black(),
                    text_color: colors.button_secondary_foreground,
                    icon_color: colors.button_secondary_foreground,
                }
            }
            ButtonVariant::Solid => {
                let colors = cx.theme().colors();
                ButtonStyle {
                    background: colors.button_background,
                    border_color: colors.button_border,
                    text_color: colors.button_foreground,
                    icon_color: colors.button_foreground,
                }
            }
            ButtonVariant::Outline => {
                let colors = cx.theme().colors();
                ButtonStyle {
                    background: colors.button_secondary_background,
                    border_color: colors.button_secondary_border,
                    text_color: colors.button_secondary_foreground,
                    icon_color: colors.button_secondary_foreground,
                }
            }
            ButtonVariant::OutlinedGhost => {
                let colors = cx.theme().colors();
                ButtonStyle {
                    background: gpui::transparent_black(),
                    border_color: colors.button_secondary_border,
                    text_color: colors.button_secondary_foreground,
                    icon_color: colors.button_secondary_foreground,
                }
            }
            ButtonVariant::Ghost => {
                let colors = cx.theme().colors();
                ButtonStyle {
                    background: gpui::transparent_black(),
                    border_color: gpui::transparent_black(),
                    text_color: colors.button_secondary_foreground,
                    icon_color: colors.button_secondary_foreground,
                }
            }
            ButtonVariant::Custom {
                background,
                foreground,
                border,
                ..
            } => ButtonStyle {
                background,
                border_color: border,
                text_color: foreground,
                icon_color: foreground,
            },
            ButtonVariant::Tinted(tint) => tint.button_style(cx),
        }
    }

    pub fn hovered(self, cx: &mut App) -> ButtonStyle {
        match self {
            ButtonVariant::Subtle => {
                let colors = cx.theme().colors();
                ButtonStyle {
                    background: colors.button_secondary_hover_background,
                    border_color: gpui::transparent_black(),
                    text_color: colors.button_secondary_foreground,
                    icon_color: colors.button_secondary_foreground,
                }
            }
            ButtonVariant::Solid => {
                let colors = cx.theme().colors();
                ButtonStyle {
                    background: colors.button_hover_background,
                    border_color: colors.button_border,
                    text_color: colors.button_foreground,
                    icon_color: colors.button_foreground,
                }
            }
            ButtonVariant::Outline => {
                let colors = cx.theme().colors();
                ButtonStyle {
                    background: colors.button_secondary_hover_background,
                    border_color: colors.button_secondary_border,
                    text_color: colors.button_secondary_foreground,
                    icon_color: colors.button_secondary_foreground,
                }
            }
            ButtonVariant::OutlinedGhost => {
                let colors = cx.theme().colors();
                ButtonStyle {
                    background: gpui::transparent_black(),
                    border_color: colors.button_secondary_border,
                    text_color: colors.button_secondary_foreground,
                    icon_color: colors.button_secondary_foreground,
                }
            }
            ButtonVariant::Ghost => {
                let colors = cx.theme().colors();
                ButtonStyle {
                    background: gpui::transparent_black(),
                    border_color: gpui::transparent_black(),
                    text_color: colors.button_secondary_foreground,
                    icon_color: colors.button_secondary_foreground,
                }
            }
            ButtonVariant::Custom {
                foreground,
                hover_background,
                border,
                ..
            } => ButtonStyle {
                background: hover_background,
                border_color: border,
                text_color: foreground,
                icon_color: foreground,
            },
            ButtonVariant::Tinted(tint) => {
                let mut styles = tint.button_style(cx);
                let theme = cx.theme();
                styles.background = theme.darken(styles.background, 0.05, 0.2);
                styles
            }
        }
    }
}

impl From<ButtonVariant> for Color {
    fn from(variant: ButtonVariant) -> Self {
        match variant {
            ButtonVariant::Subtle
            | ButtonVariant::Solid
            | ButtonVariant::Outline
            | ButtonVariant::OutlinedGhost
            | ButtonVariant::Ghost => Color::Default,
            ButtonVariant::Custom { foreground, .. } => foreground.into(),
            ButtonVariant::Tinted(tint) => tint.into(),
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq)]
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
            ButtonSize::Large => crate::rems_from_px(32.0),
            ButtonSize::Medium => crate::rems_from_px(28.0),
            ButtonSize::Default => crate::rems_from_px(22.0),
            ButtonSize::Compact => crate::rems_from_px(18.0),
            ButtonSize::None => crate::rems_from_px(16.0),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum IconPosition {
    #[default]
    Start,
    End,
}

#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    variant: ButtonVariant,
    selected: bool,
    selected_background: Option<Hsla>,
    text: SharedString,
    text_color: Option<Color>,
    text_size: Option<TextSize>,
    base: Div,
    cursor_style: CursorStyle,
    width: Option<DefiniteLength>,
    height: Option<DefiniteLength>,
    size: ButtonSize,
    disabled: bool,
    icon: Option<IconName>,
    icon_position: Option<IconPosition>,
    icon_size: Option<IconSize>,
    icon_color: Option<Color>,
    start_icon: Option<Icon>,
    end_icon: Option<Icon>,
    font_weight: Option<FontWeight>,
    tooltip: Option<Box<dyn Fn(&mut Window, &mut App) -> AnyView + 'static>>,
    on_click: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    tab_index: Option<isize>,
}

impl Button {
    pub fn new(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            variant: ButtonVariant::default(),
            selected: false,
            selected_background: None,
            text: text.into(),
            text_color: None,
            text_size: None,
            base: gpui::div(),
            cursor_style: CursorStyle::PointingHand,
            width: None,
            height: None,
            size: ButtonSize::default(),
            disabled: false,
            icon: None,
            icon_position: None,
            icon_size: None,
            icon_color: None,
            start_icon: None,
            end_icon: None,
            font_weight: None,
            tooltip: None,
            on_click: None,
            tab_index: None,
        }
    }

    pub fn color(mut self, text_color: impl Into<Option<Color>>) -> Self {
        self.text_color = text_color.into();
        self
    }

    pub fn text_size(mut self, text_size: impl Into<Option<TextSize>>) -> Self {
        self.text_size = text_size.into();
        self
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

    pub fn icon_size(mut self, icon_size: IconSize) -> Self {
        self.icon_size = Some(icon_size);
        self
    }

    pub fn icon_color(mut self, icon_color: Color) -> Self {
        self.icon_color = Some(icon_color);
        self
    }

    pub fn start_icon(mut self, icon: impl Into<Option<Icon>>) -> Self {
        self.start_icon = icon.into();
        self
    }

    pub fn end_icon(mut self, icon: impl Into<Option<Icon>>) -> Self {
        self.end_icon = icon.into();
        self
    }

    pub fn font_weight(mut self, font_weight: FontWeight) -> Self {
        self.font_weight = Some(font_weight);
        self
    }

    pub fn tab_index(mut self, tab_index: isize) -> Self {
        self.tab_index = Some(tab_index);
        self
    }
}

impl Disableable for Button {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Toggleable for Button {
    fn toggle_state(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl SelectableButton for Button {
    fn selected_background(mut self, background: Hsla) -> Self {
        self.selected_background = Some(background);
        self
    }
}

impl Clickable for Button {
    fn on_click(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }

    fn cursor_style(mut self, cursor_style: CursorStyle) -> Self {
        self.cursor_style = cursor_style;
        self
    }
}

impl FixedWidth for Button {
    fn width(mut self, width: impl Into<DefiniteLength>) -> Self {
        self.width = Some(width.into());
        self
    }

    fn full_width(mut self) -> Self {
        self.width = Some(gpui::relative(1.0));
        self
    }
}

impl ButtonCommon for Button {
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

impl RenderOnce for Button {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let variant = self.variant;
        let style = variant.enabled(cx);
        let hovered_style = variant.hovered(cx);
        let selected = self.selected && !self.disabled;
        let background = if selected {
            self.selected_background.unwrap_or(style.background)
        } else {
            style.background
        };
        let is_outlined = matches!(
            self.variant,
            ButtonVariant::Outline | ButtonVariant::OutlinedGhost
        );
        let text_accent = cx.theme().colors().text_accent;
        let icon_size = self.icon_size.unwrap_or(match self.size {
            ButtonSize::Large => IconSize::Medium,
            ButtonSize::Medium | ButtonSize::Default => IconSize::Small,
            ButtonSize::Compact | ButtonSize::None => IconSize::XSmall,
        });
        let icon_position = self.icon_position.unwrap_or(IconPosition::Start);

        let text_color = if selected {
            text_accent
        } else {
            self.text_color
                .map_or(style.text_color, |color| color.color(cx))
        };

        let icon_color = if selected {
            text_color.into()
        } else {
            self.icon_color.unwrap_or_else(|| style.icon_color.into())
        };

        let mut start_icon = self.start_icon;
        let mut end_icon = self.end_icon;
        if let Some(icon) = self.icon {
            let icon = Icon::new(icon).size(icon_size).color(icon_color);
            match icon_position {
                IconPosition::Start => {
                    if start_icon.is_none() {
                        start_icon = Some(icon);
                    }
                }
                IconPosition::End => {
                    if end_icon.is_none() {
                        end_icon = Some(icon);
                    }
                }
            }
        }

        let text_size = self.text_size.unwrap_or_default();

        self.base
            .id(self.id)
            .when_some(self.tooltip, |this, tooltip| {
                this.tooltip_show_delay(TOOLTIP_SHOW_DELAY)
                    .tooltip(move |window, cx| tooltip(window, cx))
            })
            .when_some(self.tab_index, |this, tab_index| this.tab_index(tab_index))
            .flex()
            .justify_center()
            .items_center()
            .gap(DynamicSpacing::Base04.rems(cx))
            .h(self.height.unwrap_or(self.size.rems().into()))
            .when_some(self.width, |this, width| this.w(width).justify_center())
            .text_ui_size(text_size, cx)
            .map(|this| match self.size {
                ButtonSize::Large | ButtonSize::Medium => this.px(DynamicSpacing::Base12.rems(cx)),
                ButtonSize::Default | ButtonSize::Compact => {
                    this.px(DynamicSpacing::Base08.rems(cx))
                }
                ButtonSize::None => this.px_px(),
            })
            .rounded_md()
            .border_color(style.border_color)
            .bg(background)
            .text_color(text_color)
            .when_some(self.font_weight, |this, weight| this.font_weight(weight))
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
            .when(is_outlined, |this| {
                this.border_1().border_color(style.border_color)
            })
            .when_some(start_icon, |this, icon| this.child(icon))
            .child(self.text)
            .when_some(end_icon, |this, icon| this.child(icon))
    }
}
