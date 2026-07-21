use gpui::{
    AnyElement, AnyView, App, ClickEvent, CursorStyle, ElementId, IntoElement, SharedString,
    Window, prelude::*,
};
use std::fmt::Display;

use crate::{
    ActiveTheme, ButtonCommon, ButtonLike, ButtonVariant, CircularProgress, Clickable, Color,
    CommonAnimationExt, FixedWidth, Icon, IconAsset, IconButton, IconSize, Text, TextCommon,
    TextSize, Tooltip,
};

const CIRCLE_NOTCH_GLYPH_VIEWBOX: f32 = 32.0;
const CIRCLE_NOTCH_GLYPH_STROKE_WIDTH: f32 = 2.25;
const CIRCLE_NOTCH_GLYPH_RADIUS: f32 = 12.0;

#[derive(IntoElement)]
pub struct UpdateButton {
    icon: IconAsset,
    icon_animate: bool,
    icon_color: Option<Color>,
    message: SharedString,
    tooltip: Option<Box<dyn Fn(&mut Window, &mut App) -> AnyView + 'static>>,
    disabled: bool,
    show_dismiss: bool,
    progress: Option<f32>,
    on_click: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
    on_dismiss: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
}

impl UpdateButton {
    pub fn new(icon: IconAsset, message: impl Into<SharedString>) -> Self {
        Self {
            icon,
            icon_animate: false,
            icon_color: None,
            message: message.into(),
            tooltip: None,
            disabled: false,
            show_dismiss: false,
            progress: None,
            on_click: None,
            on_dismiss: None,
        }
    }

    pub fn icon_animate(mut self, animate: bool) -> Self {
        self.icon_animate = animate;
        self
    }

    pub fn icon_color(mut self, color: impl Into<Option<Color>>) -> Self {
        self.icon_color = color.into();
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(Box::new(Tooltip::text(tooltip.into())));
        self
    }

    pub fn tooltip_fn(
        mut self,
        tooltip: impl Fn(&mut Window, &mut App) -> AnyView + 'static,
    ) -> Self {
        self.tooltip = Some(Box::new(tooltip));
        self
    }

    pub fn with_dismiss(mut self) -> Self {
        self.show_dismiss = true;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }

    pub fn on_dismiss(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_dismiss = Some(Box::new(handler));
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn progress(mut self, progress: impl Into<Option<f32>>) -> Self {
        self.progress = progress.into();
        self
    }

    pub fn checking() -> Self {
        Self::new(IconAsset::CircleNotch, "Checking for Zaku Updates…")
            .icon_animate(true)
            .disabled(true)
    }

    pub fn downloading(progress: Option<f32>) -> Self {
        Self::new(IconAsset::Download, "Downloading Zaku Update…")
            .progress(progress)
            .disabled(true)
    }

    pub fn installing(version: impl Into<SharedString>) -> Self {
        Self::new(IconAsset::CircleNotch, "Installing Zaku Update…")
            .icon_animate(true)
            .tooltip(version)
            .disabled(true)
    }

    pub fn updated(version: impl Into<SharedString>) -> Self {
        Self::new(IconAsset::Download, "Restart to Update")
            .tooltip(version)
            .with_dismiss()
    }

    pub fn failed() -> Self {
        Self::new(IconAsset::Warning, "Failed to Update")
            .icon_color(Color::Warning)
            .tooltip("Zaku couldn't update. Click to open logs.")
            .with_dismiss()
    }

    pub fn version_tooltip_message(version: impl Display) -> String {
        format!("Update to Zaku {version}")
    }

    pub fn downloading_tooltip_message(version: impl Display, progress: Option<f32>) -> String {
        let message = Self::version_tooltip_message(version);
        match progress {
            Some(progress) => format!(
                "{message} ({:.0}% downloaded)",
                progress.clamp(0.0, 1.0) * 100.0
            ),
            None => message,
        }
    }
}

impl RenderOnce for UpdateButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.theme().colors();
        let background = colors
            .status_bar_background
            .blend(colors.button_background.opacity(0.5));
        let hover_background = colors
            .status_bar_background
            .blend(colors.button_hover_background.opacity(0.7));
        let border_color = colors.text.opacity(0.15);
        let button_variant = ButtonVariant::Custom {
            background: gpui::transparent_black(),
            foreground: colors.button_foreground,
            hover_background: gpui::transparent_black(),
            border: colors.button_border,
        };
        let button_size = IconSize::Small.square(window, cx);

        let icon_element: AnyElement = if let Some(progress) = self.progress {
            let icon_box = IconSize::XSmall.rems().to_pixels(window.rem_size());
            let progress_color = Color::Default.color(cx);
            CircularProgress::new(progress, 1.0, icon_box, cx)
                .stroke_width(
                    icon_box * (CIRCLE_NOTCH_GLYPH_STROKE_WIDTH / CIRCLE_NOTCH_GLYPH_VIEWBOX),
                )
                .radius(icon_box * (CIRCLE_NOTCH_GLYPH_RADIUS / CIRCLE_NOTCH_GLYPH_VIEWBOX))
                .bg_color(progress_color.opacity(0.2))
                .progress_color(progress_color)
                .into_any_element()
        } else {
            let icon = Icon::new(self.icon)
                .size(IconSize::XSmall)
                .when_some(self.icon_color, |icon, color| icon.color(color));
            if self.icon_animate {
                icon.with_rotate_animation(2).into_any_element()
            } else {
                icon.into_any_element()
            }
        };

        let button_id = ElementId::Name(self.message.clone());
        let dismiss_button_id = ElementId::Name(format!("dismiss-{}", self.message).into());
        let label = gpui::div()
            .flex()
            .items_center()
            .w_full()
            .gap_1()
            .child(icon_element)
            .child(Text::new(self.message).size(TextSize::Small));

        gpui::div()
            .flex()
            .items_center()
            .mr_2()
            .h(button_size)
            .rounded_sm()
            .overflow_hidden()
            .border_1()
            .border_color(border_color)
            .bg(background)
            .child(
                gpui::div()
                    .h_full()
                    .rounded_l_sm()
                    .when(!self.disabled, |segment| {
                        segment.hover(move |style| style.bg(hover_background))
                    })
                    .child(
                        ButtonLike::new(button_id)
                            .height(gpui::relative(1.0))
                            .variant(button_variant)
                            .child(label)
                            .when_some(self.tooltip, |button, tooltip| button.tooltip(tooltip))
                            .when(self.disabled, |button| {
                                button.cursor_style(CursorStyle::Arrow)
                            })
                            .when_some(
                                self.on_click.filter(|_| !self.disabled),
                                |button, handler| button.on_click(handler),
                            ),
                    ),
            )
            .when(self.show_dismiss, |this| {
                this.child(
                    gpui::div()
                        .h_full()
                        .w(button_size)
                        .rounded_r_sm()
                        .border_l_1()
                        .border_color(border_color)
                        .hover(move |style| style.bg(hover_background))
                        .child(
                            IconButton::new(dismiss_button_id, IconAsset::Close)
                                .icon_size(IconSize::Indicator)
                                .full_width()
                                .height(gpui::relative(1.0))
                                .variant(button_variant)
                                .when_some(self.on_dismiss, |button, handler| {
                                    button.on_click(handler)
                                })
                                .tooltip(Tooltip::text("Dismiss")),
                        ),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use gpui::{Context, Modifiers, Render, TestAppContext};
    use std::{cell::Cell, rc::Rc};

    use settings::SettingsStore;
    use theme::LoadThemes;

    use crate::TOOLTIP_SHOW_DELAY;

    struct TestTooltip {
        rendered: Rc<Cell<u32>>,
    }

    impl Render for TestTooltip {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.rendered.set(self.rendered.get() + 1);
            gpui::div().child("tooltip")
        }
    }

    struct PreviewLikeButtons {
        tooltip_built: Rc<Cell<bool>>,
        tooltip_rendered: Rc<Cell<u32>>,
    }

    impl Render for PreviewLikeButtons {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let tooltip_built = self.tooltip_built.clone();
            let tooltip_rendered = self.tooltip_rendered.clone();
            gpui::div()
                .flex()
                .flex_col()
                .size_full()
                .child(UpdateButton::checking())
                .child(
                    UpdateButton::downloading(Some(0.5)).tooltip_fn(move |_, cx| {
                        tooltip_built.set(true);
                        let rendered = tooltip_rendered.clone();
                        cx.new(|_| TestTooltip { rendered }).into()
                    }),
                )
                .child(UpdateButton::updated("Update to Zaku 26.1"))
        }
    }

    #[gpui::test]
    fn test_downloading_tooltip_shows_in_preview_like_layout(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test_new(cx);
            cx.set_global(settings_store);
            theme::init(LoadThemes::JustBase, cx);
        });
        let tooltip_built = Rc::new(Cell::new(false));
        let tooltip_rendered = Rc::new(Cell::new(0));
        let (_, cx) = cx.add_window_view({
            let tooltip_built = tooltip_built.clone();
            let tooltip_rendered = tooltip_rendered.clone();
            |_, _| PreviewLikeButtons {
                tooltip_built,
                tooltip_rendered,
            }
        });

        cx.simulate_mouse_move(
            gpui::point(gpui::px(30.0), gpui::px(30.0)),
            None,
            Modifiers::default(),
        );
        cx.run_until_parked();
        cx.simulate_mouse_move(
            gpui::point(gpui::px(31.0), gpui::px(30.0)),
            None,
            Modifiers::default(),
        );
        cx.run_until_parked();

        cx.executor().advance_clock(TOOLTIP_SHOW_DELAY);
        cx.run_until_parked();

        assert!(tooltip_built.get(), "tooltip should have been built");

        tooltip_rendered.set(0);
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        assert!(
            tooltip_rendered.get() > 0,
            "tooltip should still be rendered after another frame"
        );
    }
}
