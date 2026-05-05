use gpui::{
    App, Bounds, Context, FocusHandle, Focusable, Image, ImageFormat, Size, TitlebarOptions,
    Window, WindowBounds, WindowKind, WindowOptions, prelude::*,
};
use std::sync::Arc;

use actions::menu;
use metadata::{
    ZAKU_COMMIT_SHA, ZAKU_DESCRIPTION, ZAKU_IDENTIFIER, ZAKU_NAME, ZAKU_REPOSITORY, ZAKU_VERSION,
};
use theme::ActiveTheme;
use ui::{Headline, Label, LabelCommon, LabelSize, Link, TextSize, prelude::*};

struct AboutWindow {
    focus_handle: FocusHandle,
    app_icon: Arc<Image>,
}

impl AboutWindow {
    fn new(cx: &mut Context<Self>) -> Self {
        let app_icon = Arc::new(Image::from_bytes(
            ImageFormat::Png,
            include_bytes!("../resources/app-icon.png").to_vec(),
        ));

        Self {
            focus_handle: cx.focus_handle(),
            app_icon,
        }
    }
}

impl Render for AboutWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("about-window")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|_, _: &menu::Cancel, window, _cx| window.remove_window()))
            .size_full()
            .bg(cx.theme().colors().background)
            .text_color(cx.theme().colors().text)
            .p_4()
            .when(cfg!(target_os = "macos"), |this| this.pt_10())
            .gap_3()
            .justify_center()
            .child(
                v_flex()
                    .w_full()
                    .gap_1()
                    .items_center()
                    .child(gpui::img(self.app_icon.clone()).size_32().flex_none())
                    .child(Headline::new(ZAKU_NAME))
                    .child(Label::new(ZAKU_DESCRIPTION).size(LabelSize::XSmall))
                    .child(gpui::div().h_5())
                    .child(
                        gpui::div()
                            .grid()
                            .grid_cols(2)
                            .self_center()
                            .gap_x_2()
                            .child(
                                gpui::div()
                                    .text_right()
                                    .child(Label::new("Version").size(LabelSize::Small)),
                            )
                            .child(
                                gpui::div()
                                    .text_left()
                                    .font_buffer(cx)
                                    .child(Label::new(ZAKU_VERSION).size(LabelSize::Small)),
                            )
                            .child(
                                gpui::div()
                                    .text_right()
                                    .child(Label::new("Commit").size(LabelSize::Small)),
                            )
                            .child(
                                gpui::div().flex().flex_shrink().child(
                                    Link::new(
                                        ZAKU_COMMIT_SHA,
                                        format!("{ZAKU_REPOSITORY}/commits/{ZAKU_COMMIT_SHA}"),
                                    )
                                    .font_buffer()
                                    .text_size(TextSize::Small),
                                ),
                            ),
                    )
                    .child(gpui::div().h_5())
                    .child(
                        h_flex().w_full().justify_center().px_6().child(
                            Button::new("about-github-repository", "GitHub")
                                .variant(ButtonVariant::Solid)
                                .label_size(LabelSize::Small)
                                .on_click(|_, _, cx| cx.open_url(ZAKU_REPOSITORY)),
                        ),
                    ),
            )
    }
}

impl Focusable for AboutWindow {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

pub(crate) fn open_window(cx: &mut App) {
    let window_size = Size {
        width: gpui::px(300.0),
        height: gpui::px(436.0),
    };
    let mut bounds = Bounds::centered(None, window_size, cx);
    bounds.origin.y -= gpui::px(36.0);

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

    if let Err(error) = cx.open_window(
        WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some(format!("About {ZAKU_NAME}").into()),
                appears_transparent: true,
                traffic_light_position: Some(gpui::point(gpui::px(12.0), gpui::px(12.0))),
            }),
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            app_id: Some(ZAKU_IDENTIFIER.to_owned()),
            is_resizable: false,
            is_minimizable: false,
            kind: WindowKind::Normal,
            ..WindowOptions::default()
        },
        |window, cx| {
            let about_window = cx.new(AboutWindow::new);
            let focus_handle = about_window.read(cx).focus_handle.clone();
            window.activate_window();
            focus_handle.focus(window, cx);
            about_window
        },
    ) {
        log::error!("Failed to open about window: {error}");
    }
}
