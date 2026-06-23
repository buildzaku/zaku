pub mod platform;

use gpui::{
    Action, AnyElement, App, Context, Decorations, ElementId, Hsla, InteractiveElement,
    IntoElement, MouseButton, ParentElement, Pixels, Render, StatefulInteractiveElement, Styled,
    Window, WindowButtonLayout, WindowControlArea, prelude::*,
};
use smallvec::SmallVec;
use std::mem;

use actions::workspace::CloseWindow;
use ui::prelude::*;

use crate::platform::{linux, windows};

pub struct PlatformTitleBar {
    id: ElementId,
    platform_style: PlatformStyle,
    children: SmallVec<[AnyElement; 2]>,
    should_move: bool,
    button_layout: Option<WindowButtonLayout>,
}

impl PlatformTitleBar {
    pub fn new(id: impl Into<ElementId>, _: &mut Context<Self>) -> Self {
        let platform_style = PlatformStyle::platform();

        Self {
            id: id.into(),
            platform_style,
            children: SmallVec::new(),
            should_move: false,
            button_layout: None,
        }
    }

    pub fn title_bar_color(&self, window: &mut Window, cx: &mut Context<Self>) -> Hsla {
        if cfg!(target_os = "linux") {
            if window.is_window_active() && !self.should_move {
                cx.theme().colors().title_bar_background
            } else {
                cx.theme().colors().title_bar_inactive_background
            }
        } else {
            cx.theme().colors().title_bar_background
        }
    }

    pub fn set_children<T>(&mut self, children: T)
    where
        T: IntoIterator<Item = AnyElement>,
    {
        self.children = children.into_iter().collect();
    }

    pub fn set_button_layout(&mut self, button_layout: Option<WindowButtonLayout>) {
        self.button_layout = button_layout;
    }

    fn effective_button_layout(
        &self,
        decorations: Decorations,
        cx: &App,
    ) -> Option<WindowButtonLayout> {
        if self.platform_style == PlatformStyle::Linux
            && matches!(decorations, Decorations::Client { .. })
        {
            self.button_layout.or_else(|| cx.button_layout())
        } else {
            None
        }
    }
}

pub fn render_left_window_controls(
    button_layout: Option<WindowButtonLayout>,
    close_action: Box<dyn Action>,
    window: &Window,
) -> Option<AnyElement> {
    match PlatformStyle::platform() {
        PlatformStyle::Linux => {
            if !matches!(window.window_decorations(), Decorations::Client { .. }) {
                return None;
            }

            let button_layout = button_layout?;
            button_layout.left[0]?;

            Some(
                linux::LinuxWindowControls::new(
                    "left-window-controls",
                    button_layout.left,
                    close_action,
                )
                .into_any_element(),
            )
        }
        PlatformStyle::Mac | PlatformStyle::Windows => None,
    }
}

pub fn render_right_window_controls(
    button_layout: Option<WindowButtonLayout>,
    close_action: Box<dyn Action>,
    window: &Window,
) -> Option<AnyElement> {
    let decorations = window.window_decorations();
    let height = ui::utils::title_bar_height(window.rem_size());

    match PlatformStyle::platform() {
        PlatformStyle::Linux => {
            if !matches!(decorations, Decorations::Client { .. }) {
                return None;
            }

            let button_layout = button_layout?;
            button_layout.right[0]?;

            Some(
                linux::LinuxWindowControls::new(
                    "right-window-controls",
                    button_layout.right,
                    close_action,
                )
                .into_any_element(),
            )
        }
        PlatformStyle::Windows => {
            Some(windows::WindowsWindowControls::new(height).into_any_element())
        }
        PlatformStyle::Mac => None,
    }
}

impl Render for PlatformTitleBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let height = ui::utils::title_bar_height(window.rem_size());
        let titlebar_color = self.title_bar_color(window, cx);
        let children = mem::take(&mut self.children);
        let decorations = window.window_decorations();
        let button_layout = self.effective_button_layout(decorations, cx);
        let close_action = Box::new(CloseWindow);
        let traffic_light_padding: Option<Pixels> = {
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            {
                None
            }
            #[cfg(target_os = "macos")]
            {
                Some(ui::utils::traffic_light_padding(height, cx))
            }
        };

        #[cfg(target_os = "macos")]
        {
            let (x_inset, y_inset) = ui::utils::traffic_light_inset(height, cx);
            window.set_traffic_light_position(gpui::point(x_inset, y_inset));
        }

        h_flex()
            .window_control_area(WindowControlArea::Drag)
            .w_full()
            .h(height)
            .map(|this| {
                this.on_mouse_down_out(cx.listener(move |this, _event, _window, _cx| {
                    this.should_move = false;
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, _cx| {
                        this.should_move = false;
                    }),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, _cx| {
                        this.should_move = true;
                    }),
                )
                .on_mouse_move(cx.listener(move |this, _event, window, _cx| {
                    if this.should_move {
                        this.should_move = false;
                        window.start_window_move();
                    }
                }))
            })
            .map(|this| {
                this.id(self.id.clone())
                    .when(self.platform_style == PlatformStyle::Mac, |this| {
                        this.on_click(|event, window, _cx| {
                            if event.click_count() == 2 {
                                window.titlebar_double_click();
                            }
                        })
                    })
                    .when(self.platform_style == PlatformStyle::Linux, |this| {
                        this.on_click(|event, window, _cx| {
                            if event.click_count() == 2 {
                                window.zoom_window();
                            }
                        })
                    })
            })
            .map(|this| {
                if window.is_fullscreen() {
                    this.pl_2()
                } else if let Some(traffic_light_padding) = traffic_light_padding {
                    this.pl(traffic_light_padding)
                } else if let Some(controls) = render_left_window_controls(
                    button_layout,
                    close_action.as_ref().boxed_clone(),
                    window,
                ) {
                    this.child(controls)
                } else {
                    this.pl_2()
                }
            })
            .map(|this| match decorations {
                Decorations::Server => this,
                Decorations::Client { tiling, .. } => this
                    .when(!(tiling.top || tiling.right), |this| {
                        this.rounded_tr(theme::CLIENT_SIDE_DECORATION_ROUNDING)
                    })
                    .when(!(tiling.top || tiling.left), |this| {
                        this.rounded_tl(theme::CLIENT_SIDE_DECORATION_ROUNDING)
                    })
                    .mt(gpui::px(-1.0))
                    .mb(gpui::px(-1.0))
                    .border(gpui::px(1.0))
                    .border_color(titlebar_color),
            })
            .bg(titlebar_color)
            .content_stretch()
            .child(
                gpui::div()
                    .id(self.id.clone())
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .overflow_x_hidden()
                    .w_full()
                    .children(children),
            )
            .when(!window.is_fullscreen(), |this| {
                let title_bar = this.children(render_right_window_controls(
                    button_layout,
                    close_action.as_ref().boxed_clone(),
                    window,
                ));

                if self.platform_style == PlatformStyle::Linux
                    && matches!(decorations, Decorations::Client { .. })
                {
                    title_bar.when(window.window_controls().window_menu, |titlebar| {
                        titlebar.on_mouse_down(MouseButton::Right, move |event, window, _cx| {
                            window.show_window_menu(event.position);
                        })
                    })
                } else {
                    title_bar
                }
            })
    }
}

impl ParentElement for PlatformTitleBar {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}
