use gpui::{
    AnyView, App, ClickEvent, CursorStyle, DefiniteLength, Div, ElementId, FontWeight, Hsla,
    MouseButton, Rems, SharedString, Window, prelude::*,
};

use icons::IconName;
use theme::ActiveTheme;

use crate::{
    ButtonCommon, Clickable, Color, Disableable, DynamicSpacing, FixedWidth, Icon, IconSize,
    LabelSize, SelectableButton, StyledTypography, Toggleable,
};

#[derive(Debug, Clone)]
pub struct ButtonStyle {
    pub background: Hsla,
    pub border_color: Hsla,
    pub label_color: Hsla,
    pub icon_color: Hsla,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy, Default)]
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
                label_color: cx.theme().colors().text,
                icon_color: cx.theme().colors().text,
            },
            TintColor::Error => ButtonStyle {
                background: cx.theme().status().error_background,
                border_color: cx.theme().status().error_border,
                label_color: cx.theme().colors().text,
                icon_color: cx.theme().colors().text,
            },
            TintColor::Warning => ButtonStyle {
                background: cx.theme().status().warning_background,
                border_color: cx.theme().status().warning_border,
                label_color: cx.theme().colors().text,
                icon_color: cx.theme().colors().text,
            },
            TintColor::Success => ButtonStyle {
                background: cx.theme().status().success_background,
                border_color: cx.theme().status().success_border,
                label_color: cx.theme().colors().text,
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

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    #[default]
    Subtle,
    Solid,
    Accent,
    Outline,
    Ghost,
    Tinted(TintColor),
}

impl From<ButtonVariant> for Color {
    fn from(variant: ButtonVariant) -> Self {
        match variant {
            ButtonVariant::Subtle
            | ButtonVariant::Solid
            | ButtonVariant::Accent
            | ButtonVariant::Outline
            | ButtonVariant::Ghost => Color::Default,
            ButtonVariant::Tinted(tint) => tint.into(),
        }
    }
}

impl ButtonVariant {
    pub fn enabled(self, cx: &mut App) -> ButtonStyle {
        match self {
            ButtonVariant::Subtle => {
                let colors = cx.theme().colors();
                ButtonStyle {
                    background: colors.ghost_element_background,
                    border_color: gpui::transparent_black(),
                    label_color: colors.text,
                    icon_color: colors.text,
                }
            }
            ButtonVariant::Solid => {
                let colors = cx.theme().colors();
                ButtonStyle {
                    background: colors.element_background,
                    border_color: gpui::transparent_black(),
                    label_color: colors.text,
                    icon_color: colors.text,
                }
            }
            ButtonVariant::Accent => {
                let status = cx.theme().status();
                ButtonStyle {
                    background: status.info,
                    border_color: gpui::transparent_black(),
                    label_color: status.info_background,
                    icon_color: status.info_background,
                }
            }
            ButtonVariant::Outline => {
                let colors = cx.theme().colors();
                ButtonStyle {
                    background: colors.ghost_element_background,
                    border_color: colors.border_variant,
                    label_color: colors.text,
                    icon_color: colors.text,
                }
            }
            ButtonVariant::Ghost => {
                let colors = cx.theme().colors();
                ButtonStyle {
                    background: gpui::transparent_black(),
                    border_color: gpui::transparent_black(),
                    label_color: colors.text,
                    icon_color: colors.text,
                }
            }
            ButtonVariant::Tinted(tint) => tint.button_style(cx),
        }
    }

    pub fn hovered(self, cx: &mut App) -> ButtonStyle {
        match self {
            ButtonVariant::Subtle => {
                let colors = cx.theme().colors();
                ButtonStyle {
                    background: colors.ghost_element_hover,
                    border_color: gpui::transparent_black(),
                    label_color: colors.text,
                    icon_color: colors.text,
                }
            }
            ButtonVariant::Solid => {
                let colors = cx.theme().colors();
                ButtonStyle {
                    background: colors.element_hover,
                    border_color: gpui::transparent_black(),
                    label_color: colors.text,
                    icon_color: colors.text,
                }
            }
            ButtonVariant::Accent => {
                let status = cx.theme().status();
                ButtonStyle {
                    background: status.info,
                    border_color: gpui::transparent_black(),
                    label_color: status.info_background,
                    icon_color: status.info_background,
                }
            }
            ButtonVariant::Outline => {
                let colors = cx.theme().colors();
                ButtonStyle {
                    background: colors.ghost_element_hover,
                    border_color: colors.border,
                    label_color: colors.text,
                    icon_color: colors.text,
                }
            }
            ButtonVariant::Ghost => {
                let colors = cx.theme().colors();
                ButtonStyle {
                    background: gpui::transparent_black(),
                    border_color: gpui::transparent_black(),
                    label_color: colors.text,
                    icon_color: colors.text,
                }
            }
            ButtonVariant::Tinted(tint) => {
                let mut styles = tint.button_style(cx);
                let theme = cx.theme();
                styles.background = theme.darken(styles.background, 0.05, 0.2);
                styles
            }
        }
    }

    pub fn active(self, cx: &mut App) -> ButtonStyle {
        match self {
            ButtonVariant::Subtle => {
                let colors = cx.theme().colors();
                ButtonStyle {
                    background: colors.ghost_element_active,
                    border_color: gpui::transparent_black(),
                    label_color: colors.text,
                    icon_color: colors.text,
                }
            }
            ButtonVariant::Solid => {
                let colors = cx.theme().colors();
                ButtonStyle {
                    background: colors.element_active,
                    border_color: gpui::transparent_black(),
                    label_color: colors.text,
                    icon_color: colors.text,
                }
            }
            ButtonVariant::Accent => {
                let status = cx.theme().status();
                ButtonStyle {
                    background: status.info,
                    border_color: gpui::transparent_black(),
                    label_color: status.info_background,
                    icon_color: status.info_background,
                }
            }
            ButtonVariant::Outline => {
                let colors = cx.theme().colors();
                ButtonStyle {
                    background: colors.ghost_element_active,
                    border_color: colors.border_variant,
                    label_color: colors.text,
                    icon_color: colors.text,
                }
            }
            ButtonVariant::Ghost => {
                let colors = cx.theme().colors();
                ButtonStyle {
                    background: gpui::transparent_black(),
                    border_color: gpui::transparent_black(),
                    label_color: colors.text,
                    icon_color: colors.text,
                }
            }
            ButtonVariant::Tinted(tint) => tint.button_style(cx),
        }
    }

    pub fn disabled(self, cx: &mut App) -> ButtonStyle {
        match self {
            ButtonVariant::Subtle => {
                let colors = cx.theme().colors();
                ButtonStyle {
                    background: colors.ghost_element_disabled,
                    border_color: colors.border_disabled,
                    label_color: colors.text_disabled,
                    icon_color: colors.text_disabled,
                }
            }
            ButtonVariant::Solid | ButtonVariant::Accent | ButtonVariant::Outline => {
                let colors = cx.theme().colors();
                ButtonStyle {
                    background: colors.element_disabled,
                    border_color: colors.border_disabled,
                    label_color: colors.text_disabled,
                    icon_color: colors.text_disabled,
                }
            }
            ButtonVariant::Ghost => {
                let colors = cx.theme().colors();
                ButtonStyle {
                    background: gpui::transparent_black(),
                    border_color: gpui::transparent_black(),
                    label_color: colors.text_disabled,
                    icon_color: colors.text_disabled,
                }
            }
            ButtonVariant::Tinted(tint) => tint.button_style(cx),
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
            ButtonSize::Large => crate::rems_from_px(32.0),
            ButtonSize::Medium => crate::rems_from_px(28.0),
            ButtonSize::Default => crate::rems_from_px(22.0),
            ButtonSize::Compact => crate::rems_from_px(18.0),
            ButtonSize::None => crate::rems_from_px(16.0),
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
    selected: bool,
    selected_style: Option<ButtonVariant>,
    label: SharedString,
    label_color: Option<Color>,
    label_size: Option<LabelSize>,
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
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            variant: ButtonVariant::default(),
            selected: false,
            selected_style: None,
            label: label.into(),
            label_color: None,
            label_size: None,
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

    pub fn color(mut self, label_color: impl Into<Option<Color>>) -> Self {
        self.label_color = label_color.into();
        self
    }

    pub fn label_size(mut self, label_size: impl Into<Option<LabelSize>>) -> Self {
        self.label_size = label_size.into();
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
    fn selected_style(mut self, style: ButtonVariant) -> Self {
        self.selected_style = Some(style);
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
        self.width = Some(gpui::relative(1.));
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
        let disabled = self.disabled;
        let variant = self
            .selected_style
            .filter(|_| self.selected)
            .unwrap_or(self.variant);
        let style = if self.disabled {
            variant.disabled(cx)
        } else {
            variant.enabled(cx)
        };
        let hovered_style = variant.hovered(cx);
        let active_style = variant.active(cx);
        let text_accent = cx.theme().colors().text_accent;
        let icon_size = self.icon_size.unwrap_or(match self.size {
            ButtonSize::Large => IconSize::Medium,
            ButtonSize::Medium => IconSize::Small,
            ButtonSize::Default => IconSize::Small,
            ButtonSize::Compact => IconSize::XSmall,
            ButtonSize::None => IconSize::XSmall,
        });
        let icon_position = self.icon_position.unwrap_or(IconPosition::Start);

        let text_color = if self.disabled {
            style.label_color
        } else if self.selected {
            text_accent
        } else {
            style.label_color
        };

        let label_text_color = if self.disabled {
            style.label_color
        } else if self.selected {
            text_accent
        } else {
            self.label_color
                .map(|color| color.color(cx))
                .unwrap_or(style.label_color)
        };

        let icon_color = if self.disabled || self.selected {
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

        let label_size = self.label_size.unwrap_or_default();

        self.base
            .id(self.id)
            .when_some(self.tooltip, |this, tooltip| {
                this.tooltip(move |window, cx| tooltip(window, cx))
            })
            .when_some(self.tab_index, |this, tab_index| this.tab_index(tab_index))
            .flex()
            .justify_center()
            .items_center()
            .gap(DynamicSpacing::Base04.rems(cx))
            .h(self.height.unwrap_or(self.size.rems().into()))
            .when_some(self.width, |this, width| this.w(width).justify_center())
            .map(|this| match label_size {
                LabelSize::Large => this.text_ui_lg(cx),
                LabelSize::Default => this.text_ui(cx),
                LabelSize::Small => this.text_ui_sm(cx),
                LabelSize::XSmall => this.text_ui_xs(cx),
            })
            .map(|this| match self.size {
                ButtonSize::Large | ButtonSize::Medium => this.px(DynamicSpacing::Base12.rems(cx)),
                ButtonSize::Default | ButtonSize::Compact => {
                    this.px(DynamicSpacing::Base08.rems(cx))
                }
                ButtonSize::None => this.px_px(),
            })
            .rounded_md()
            .border_color(style.border_color)
            .bg(style.background)
            .text_color(label_text_color)
            .when_some(self.font_weight, |this, weight| this.font_weight(weight))
            .when(self.disabled, |this| {
                if self.cursor_style == CursorStyle::PointingHand {
                    this.cursor_not_allowed()
                } else {
                    this.cursor(self.cursor_style)
                }
            })
            .when(!self.disabled, |this| {
                this.cursor(self.cursor_style)
                    .hover(|style| style.bg(hovered_style.background))
                    .active(|style| style.bg(active_style.background))
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
            .when(self.variant == ButtonVariant::Outline, |this| {
                this.border_1().border_color(style.border_color)
            })
            .when_some(start_icon, |this, icon| {
                this.child(if disabled {
                    icon.color(Color::Disabled)
                } else {
                    icon
                })
            })
            .child(self.label)
            .when_some(end_icon, |this, icon| {
                this.child(if disabled {
                    icon.color(Color::Disabled)
                } else {
                    icon
                })
            })
    }
}
