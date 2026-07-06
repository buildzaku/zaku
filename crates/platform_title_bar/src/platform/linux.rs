use gpui::{
    Action, AnyElement, App, ElementId, Hsla, IntoElement, MAX_BUTTONS_PER_SIDE, MouseButton,
    RenderOnce, Window, WindowButton, prelude::*,
};

use theme::{ActiveTheme, Appearance};
use ui::IconAsset;

#[derive(IntoElement)]
pub struct LinuxWindowControls {
    id: &'static str,
    buttons: [Option<WindowButton>; MAX_BUTTONS_PER_SIDE],
    close_action: Box<dyn Action>,
}

impl LinuxWindowControls {
    pub fn new(
        id: &'static str,
        buttons: [Option<WindowButton>; MAX_BUTTONS_PER_SIDE],
        close_action: Box<dyn Action>,
    ) -> Self {
        Self {
            id,
            buttons,
            close_action,
        }
    }
}

impl RenderOnce for LinuxWindowControls {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let is_maximized = window.is_maximized();
        let supported_controls = window.window_controls();
        let button_elements: Vec<AnyElement> = self
            .buttons
            .iter()
            .filter_map(|button| *button)
            .filter(|button| match button {
                WindowButton::Minimize => supported_controls.minimize,
                WindowButton::Maximize => supported_controls.maximize,
                WindowButton::Close => true,
            })
            .map(|button| {
                create_window_button(button, button.id(), is_maximized, &*self.close_action, cx)
            })
            .collect();

        gpui::div()
            .id(self.id)
            .flex()
            .items_center()
            .when(!button_elements.is_empty(), |this| {
                this.gap_1p5()
                    .px(gpui::rems(0.4375))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .children(button_elements)
            })
    }
}

fn create_window_button(
    button: WindowButton,
    id: &'static str,
    is_maximized: bool,
    close_action: &dyn Action,
    cx: &mut App,
) -> AnyElement {
    match button {
        WindowButton::Minimize => {
            WindowControl::new(id, WindowControlType::Minimize, cx).into_any_element()
        }
        WindowButton::Maximize => WindowControl::new(
            id,
            if is_maximized {
                WindowControlType::Restore
            } else {
                WindowControlType::Maximize
            },
            cx,
        )
        .into_any_element(),
        WindowButton::Close => {
            WindowControl::close(id, close_action.boxed_clone(), cx).into_any_element()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WindowControlType {
    Minimize,
    Restore,
    Maximize,
    Close,
}

impl WindowControlType {
    pub fn icon(self) -> IconAsset {
        match self {
            WindowControlType::Minimize => IconAsset::LinuxMinimize,
            WindowControlType::Restore => IconAsset::LinuxRestore,
            WindowControlType::Maximize => IconAsset::LinuxMaximize,
            WindowControlType::Close => IconAsset::LinuxClose,
        }
    }
}

pub struct WindowControlStyle {
    background: Hsla,
    background_hover: Hsla,
    background_active: Hsla,
    icon: Hsla,
}

impl WindowControlStyle {
    pub fn default(cx: &mut App) -> Self {
        match cx.theme().appearance() {
            Appearance::Light => Self {
                background: gpui::rgba(0x3d3d3d1a).into(),
                background_hover: gpui::rgba(0x1a1a1a26).into(),
                background_active: gpui::rgba(0x0a0a0a40).into(),
                icon: gpui::rgb(0x3d3d3d).into(),
            },
            Appearance::Dark => Self {
                background: gpui::rgba(0xf7f7f71a).into(),
                background_hover: gpui::rgba(0xf4f4f426).into(),
                background_active: gpui::rgba(0xeaeaea40).into(),
                icon: gpui::rgb(0xf7f7f7).into(),
            },
        }
    }
}

#[derive(IntoElement)]
pub struct WindowControl {
    id: ElementId,
    icon: WindowControlType,
    style: WindowControlStyle,
    close_action: Option<Box<dyn Action>>,
}

impl WindowControl {
    pub fn new(id: impl Into<ElementId>, icon: WindowControlType, cx: &mut App) -> Self {
        let style = WindowControlStyle::default(cx);

        Self {
            id: id.into(),
            icon,
            style,
            close_action: None,
        }
    }

    pub fn close(id: impl Into<ElementId>, close_action: Box<dyn Action>, cx: &mut App) -> Self {
        let style = WindowControlStyle::default(cx);

        Self {
            id: id.into(),
            icon: WindowControlType::Close,
            style,
            close_action: Some(close_action),
        }
    }
}

impl RenderOnce for WindowControl {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let icon = gpui::svg()
            .size_4()
            .flex_none()
            .path(self.icon.icon().path())
            .text_color(self.style.icon);
        let button_id = self.id;
        let group_name = button_id.to_string();

        let control_surface = gpui::div()
            .id((button_id.clone(), "surface"))
            .mx(gpui::rems(0.1875))
            .flex()
            .items_center()
            .justify_center()
            .content_center()
            .size_6()
            .rounded_2xl()
            .bg(self.style.background)
            .group_hover(group_name.clone(), |this| {
                this.bg(self.style.background_hover)
            })
            .group_active(group_name.clone(), |this| {
                this.bg(self.style.background_active)
            })
            .child(icon);

        gpui::div()
            .id(button_id)
            .group(group_name)
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .content_center()
            .min_w_6()
            .p_0()
            .cursor_pointer()
            .child(control_surface)
            .on_mouse_move(|_, _, cx| cx.stop_propagation())
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                match self.icon {
                    WindowControlType::Minimize => window.minimize_window(),
                    WindowControlType::Restore | WindowControlType::Maximize => {
                        window.zoom_window();
                    }
                    WindowControlType::Close => window.dispatch_action(
                        self.close_action
                            .as_ref()
                            .expect("close control should have a close action")
                            .boxed_clone(),
                        cx,
                    ),
                }
            })
    }
}
