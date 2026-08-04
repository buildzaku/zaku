use gpui::{
    App, Context, Entity, FocusHandle, Focusable, Image, ImageFormat, Pixels, Size, Subscription,
    Tiling, Window, WindowBounds, prelude::*,
};
use std::sync::Arc;

use metadata::{ZAKU_BUILD_ID, ZAKU_COMMIT_SHA, ZAKU_DESCRIPTION, ZAKU_NAME, ZAKU_REPOSITORY};
use platform_title_bar::PlatformTitleBar;
use settings::SettingsStore;
use theme::ThemeSettings;
use ui::{
    ActiveTheme, Button, ButtonCommon, ButtonVariant, Clickable, Headline, HeadlineSize, Link,
    StyledTypography, Text,
};

const MIN_WINDOW_SIZE: Size<Pixels> = gpui::size(gpui::px(300.0), gpui::px(440.0));

struct AboutWindow {
    title_bar: Entity<PlatformTitleBar>,
    focus_handle: FocusHandle,
    app_icon: Arc<Image>,
    _settings_subscription: Subscription,
}

impl AboutWindow {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let app_icon = Arc::new(Image::from_bytes(
            ImageFormat::Png,
            include_bytes!("../resources/app-icon-no-padding.png").to_vec(),
        ));
        let settings_subscription =
            cx.observe_global_in::<SettingsStore>(window, |_, window, cx| {
                let display_size = window
                    .display(cx)
                    .map(|display| display.visible_bounds().size);
                window.resize(Self::window_size(display_size, cx));
                cx.notify();
            });

        Self {
            title_bar: cx.new(|cx| PlatformTitleBar::new("about-title-bar", cx)),
            focus_handle: cx.focus_handle(),
            app_icon,
            _settings_subscription: settings_subscription,
        }
    }

    fn window_size(display_size: Option<Size<Pixels>>, cx: &App) -> Size<Pixels> {
        let ui_font_size = f32::from(ThemeSettings::get_global(cx).ui_font_size(cx));
        let default_ui_font_size = f32::from(ThemeSettings::default().ui_font_size(cx));
        let scale_factor = (ui_font_size / default_ui_font_size).max(1.0);
        let window_size = MIN_WINDOW_SIZE.map(|axis| axis * scale_factor);

        display_size.map_or(window_size, |display_size| window_size.min(&display_size))
    }
}

impl Render for AboutWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui_font = theme::setup_ui_font(window, cx);
        let theme_colors = cx.theme().colors();
        let content = gpui::div()
            .flex()
            .flex_col()
            .w_full()
            .gap_1()
            .items_center()
            .child(
                gpui::img(self.app_icon.clone())
                    .size(gpui::rems(7.0))
                    .flex_none()
                    .mb_6(),
            )
            .child(Headline::new(ZAKU_NAME).size(HeadlineSize::Large))
            .child(Text::new(ZAKU_DESCRIPTION))
            .child(gpui::div().h_5())
            .child(
                gpui::div()
                    .grid()
                    .grid_cols(2)
                    .self_center()
                    .gap_x_2()
                    .child(gpui::div().text_right().child(Text::new("Version")))
                    .child(
                        gpui::div()
                            .text_left()
                            .font_buffer(cx)
                            .child(Text::new(metadata::version(cx).to_string())),
                    )
                    .when_some(ZAKU_BUILD_ID, |this, build_id| {
                        this.child(gpui::div().text_right().child(Text::new("Build")))
                            .child(
                                gpui::div()
                                    .text_left()
                                    .font_buffer(cx)
                                    .child(Text::new(build_id)),
                            )
                    })
                    .child(gpui::div().text_right().child(Text::new("Commit")))
                    .child(
                        gpui::div().flex().flex_shrink_1().child(
                            Link::new(
                                ZAKU_COMMIT_SHA,
                                format!("{ZAKU_REPOSITORY}/commits/{ZAKU_COMMIT_SHA}"),
                            )
                            .font_buffer(),
                        ),
                    ),
            )
            .child(gpui::div().h_5())
            .child(
                gpui::div()
                    .flex()
                    .items_center()
                    .w_full()
                    .justify_center()
                    .px_6()
                    .child(
                        Button::new("about-github-repository", "GitHub")
                            .variant(ButtonVariant::Solid)
                            .on_click(|_, _, cx| cx.open_url(ZAKU_REPOSITORY)),
                    ),
            );

        workspace::client_side_decorations(
            gpui::div()
                .flex()
                .flex_col()
                .size_full()
                .overflow_hidden()
                .font(ui_font)
                .text_color(theme_colors.text)
                .on_action(|_: &actions::workspace::CloseWindow, window, _| {
                    window.remove_window();
                })
                .on_action(
                    cx.listener(|_, _: &actions::menu::Cancel, window, _| window.remove_window()),
                )
                .child(self.title_bar.clone())
                .child(
                    gpui::div()
                        .id("about-window")
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .bg(theme_colors.background)
                        .border_y_1()
                        .border_color(theme_colors.border)
                        .child(
                            gpui::div()
                                .flex()
                                .flex_col()
                                .flex_none()
                                .w_full()
                                .min_h_full()
                                .track_focus(&self.focus_handle)
                                .px_3p5()
                                .py_16()
                                .justify_center()
                                .child(content),
                        ),
                ),
            window,
            cx,
            Tiling::default(),
        )
    }
}

impl Focusable for AboutWindow {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

pub(crate) fn open_window(cx: &mut App) {
    if let Some(existing) = cx
        .windows()
        .into_iter()
        .find_map(|window| window.downcast::<AboutWindow>())
    {
        if let Err(error) = existing.update(cx, |about_window, window, cx| {
            window.activate_window();
            about_window.focus_handle.focus(window, cx);
        }) {
            log::error!("Failed to activate About window: {error}");
        }
        return;
    }

    let display_size = cx
        .primary_display()
        .map(|display| display.visible_bounds().size);
    let window_size = AboutWindow::window_size(display_size, cx);

    let mut window_options = workspace::build_window_options(None, cx);
    if let Some(titlebar) = window_options.titlebar.as_mut() {
        titlebar.title = Some(format!("About {ZAKU_NAME}").into());
    }
    window_options.window_bounds = Some(WindowBounds::centered(window_size, cx));
    window_options.is_resizable = false;
    window_options.is_minimizable = false;

    if let Err(error) = cx.open_window(window_options, |window, cx| {
        let about_window = cx.new(|cx| AboutWindow::new(window, cx));
        let focus_handle = about_window.read(cx).focus_handle.clone();
        window.activate_window();
        focus_handle.focus(window, cx);
        about_window
    }) {
        log::error!("Failed to open about window: {error}");
    }
}
