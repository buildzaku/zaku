use gpui::{
    AnyElement, Context, ElementId, Hsla, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Render, StatefulInteractiveElement, Styled, Window, WindowControlArea,
    prelude::*,
};
use smallvec::SmallVec;
use std::mem;

use ui::prelude::*;
#[cfg(target_os = "macos")]
use ui::utils::MACOS_TRAFFIC_LIGHT_PADDING;

pub struct PlatformTitleBar {
    id: ElementId,
    platform_style: PlatformStyle,
    children: SmallVec<[AnyElement; 2]>,
    should_move: bool,
}

impl PlatformTitleBar {
    pub fn new(id: impl Into<ElementId>, _: &mut Context<Self>) -> Self {
        let platform_style = PlatformStyle::platform();

        Self {
            id: id.into(),
            platform_style,
            children: SmallVec::new(),
            should_move: false,
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
}

impl Render for PlatformTitleBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let height = ui::utils::title_bar_height(window.rem_size());
        let titlebar_color = self.title_bar_color(window, cx);
        let children = mem::take(&mut self.children);

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
                } else if self.platform_style == PlatformStyle::Mac {
                    #[cfg(target_os = "macos")]
                    {
                        this.pl(gpui::px(MACOS_TRAFFIC_LIGHT_PADDING))
                    }
                    #[cfg(any(target_os = "linux", target_os = "windows"))]
                    {
                        this
                    }
                } else {
                    this.pl_2()
                }
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
    }
}

impl ParentElement for PlatformTitleBar {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}
