mod base;
mod icon_button;
mod link_button;
mod styled_icon;

pub use base::*;
pub use icon_button::*;
pub use link_button::*;

use gpui::{
    AnyView, App, ClickEvent, CursorStyle, DefiniteLength, ElementId, FontWeight, Hsla, Rems,
    SharedString, Window, prelude::*,
};

use ::svg::IconAsset;
use theme::ActiveTheme;

use crate::{
    Clickable, Color, Disableable, DynamicSpacing, FixedWidth, Icon, IconSize, StyledTypography,
    TextSize, Toggleable,
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
                let theme_colors = cx.theme().colors();
                ButtonStyle {
                    background: theme_colors.button_secondary_background,
                    border_color: gpui::transparent_black(),
                    text_color: theme_colors.button_secondary_foreground,
                    icon_color: theme_colors.button_secondary_foreground,
                }
            }
            ButtonVariant::Solid => {
                let theme_colors = cx.theme().colors();
                ButtonStyle {
                    background: theme_colors.button_background,
                    border_color: theme_colors.button_border,
                    text_color: theme_colors.button_foreground,
                    icon_color: theme_colors.button_foreground,
                }
            }
            ButtonVariant::Outline => {
                let theme_colors = cx.theme().colors();
                ButtonStyle {
                    background: theme_colors.button_secondary_background,
                    border_color: theme_colors.button_secondary_border,
                    text_color: theme_colors.button_secondary_foreground,
                    icon_color: theme_colors.button_secondary_foreground,
                }
            }
            ButtonVariant::OutlinedGhost => {
                let theme_colors = cx.theme().colors();
                ButtonStyle {
                    background: gpui::transparent_black(),
                    border_color: theme_colors.button_secondary_border,
                    text_color: theme_colors.button_secondary_foreground,
                    icon_color: theme_colors.button_secondary_foreground,
                }
            }
            ButtonVariant::Ghost => {
                let theme_colors = cx.theme().colors();
                ButtonStyle {
                    background: gpui::transparent_black(),
                    border_color: gpui::transparent_black(),
                    text_color: theme_colors.button_secondary_foreground,
                    icon_color: theme_colors.button_secondary_foreground,
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
                let theme_colors = cx.theme().colors();
                ButtonStyle {
                    background: theme_colors.button_secondary_hover_background,
                    border_color: gpui::transparent_black(),
                    text_color: theme_colors.button_secondary_foreground,
                    icon_color: theme_colors.button_secondary_foreground,
                }
            }
            ButtonVariant::Solid => {
                let theme_colors = cx.theme().colors();
                ButtonStyle {
                    background: theme_colors.button_hover_background,
                    border_color: theme_colors.button_border,
                    text_color: theme_colors.button_foreground,
                    icon_color: theme_colors.button_foreground,
                }
            }
            ButtonVariant::Outline => {
                let theme_colors = cx.theme().colors();
                ButtonStyle {
                    background: theme_colors.button_secondary_hover_background,
                    border_color: theme_colors.button_secondary_border,
                    text_color: theme_colors.button_secondary_foreground,
                    icon_color: theme_colors.button_secondary_foreground,
                }
            }
            ButtonVariant::OutlinedGhost => {
                let theme_colors = cx.theme().colors();
                ButtonStyle {
                    background: gpui::transparent_black(),
                    border_color: theme_colors.button_secondary_border,
                    text_color: theme_colors.button_secondary_foreground,
                    icon_color: theme_colors.button_secondary_foreground,
                }
            }
            ButtonVariant::Ghost => {
                let theme_colors = cx.theme().colors();
                ButtonStyle {
                    background: gpui::transparent_black(),
                    border_color: gpui::transparent_black(),
                    text_color: theme_colors.button_secondary_foreground,
                    icon_color: theme_colors.button_secondary_foreground,
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
            ButtonSize::Large => crate::rems_from_px(32.0_f32),
            ButtonSize::Medium => crate::rems_from_px(28.0_f32),
            ButtonSize::Default => crate::rems_from_px(22.0_f32),
            ButtonSize::Compact => crate::rems_from_px(18.0_f32),
            ButtonSize::None => crate::rems_from_px(16.0_f32),
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
    base: ButtonLike,
    text: SharedString,
    text_color: Option<Color>,
    text_size: Option<TextSize>,
    icon: Option<IconAsset>,
    icon_position: Option<IconPosition>,
    icon_size: Option<IconSize>,
    icon_color: Option<Color>,
    start_icon: Option<Icon>,
    end_icon: Option<Icon>,
    font_weight: Option<FontWeight>,
}

impl Button {
    pub fn new(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Self {
        Self {
            base: ButtonLike::new(id),
            text: text.into(),
            text_color: None,
            text_size: None,
            icon: None,
            icon_position: None,
            icon_size: None,
            icon_color: None,
            start_icon: None,
            end_icon: None,
            font_weight: None,
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

    pub fn height(mut self, height: impl Into<DefiniteLength>) -> Self {
        self.base = self.base.height(height);
        self
    }

    pub fn icon(mut self, icon: impl Into<Option<IconAsset>>) -> Self {
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
        self.base = self.base.tab_index(tab_index);
        self
    }
}

impl Disableable for Button {
    fn disabled(mut self, disabled: bool) -> Self {
        self.base = self.base.disabled(disabled);
        self
    }
}

impl Toggleable for Button {
    fn toggle_state(mut self, selected: bool) -> Self {
        self.base = self.base.toggle_state(selected);
        self
    }
}

impl SelectableButton for Button {
    fn selected_background(mut self, background: Hsla) -> Self {
        self.base = self.base.selected_background(background);
        self
    }
}

impl Clickable for Button {
    fn on_click(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.base = self.base.on_click(handler);
        self
    }

    fn cursor_style(mut self, cursor_style: CursorStyle) -> Self {
        self.base = self.base.cursor_style(cursor_style);
        self
    }
}

impl FixedWidth for Button {
    fn width(mut self, width: impl Into<DefiniteLength>) -> Self {
        self.base = self.base.width(width);
        self
    }

    fn full_width(mut self) -> Self {
        self.base = self.base.full_width();
        self
    }
}

impl ButtonCommon for Button {
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

impl RenderOnce for Button {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let style = self.base.variant.enabled(cx);
        let selected = self.base.selected && !self.base.disabled;
        let size = self.base.size;
        let text_accent = cx.theme().colors().text_accent;
        let icon_size = self.icon_size.unwrap_or(match size {
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

        self.base.child(
            gpui::div()
                .flex()
                .items_center()
                .justify_center()
                .gap(DynamicSpacing::Base04.rems(cx))
                .text_ui_size(text_size, cx)
                .map(|this| match size {
                    ButtonSize::Large | ButtonSize::Medium => {
                        this.px(DynamicSpacing::Base12.rems(cx) - DynamicSpacing::Base08.rems(cx))
                    }
                    ButtonSize::Default | ButtonSize::Compact => {
                        this.px(DynamicSpacing::Base08.rems(cx) - DynamicSpacing::Base04.rems(cx))
                    }
                    ButtonSize::None => this,
                })
                .text_color(text_color)
                .when_some(self.font_weight, |this, weight| this.font_weight(weight))
                .when_some(start_icon, |this, icon| this.child(icon))
                .child(self.text)
                .when_some(end_icon, |this, icon| this.child(icon)),
        )
    }
}
