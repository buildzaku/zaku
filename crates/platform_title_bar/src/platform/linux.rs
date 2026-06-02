use gpui::{
    Action, AnyElement, App, ElementId, Hsla, IntoElement, MAX_BUTTONS_PER_SIDE, MouseButton,
    RenderOnce, Window, WindowButton, prelude::*,
};

use ui::prelude::*;

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

        h_flex()
            .id(self.id)
            .when(!button_elements.is_empty(), |this| {
                this.gap_3()
                    .px_3()
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

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub enum WindowControlType {
    Minimize,
    Restore,
    Maximize,
    Close,
}

impl WindowControlType {
    pub fn icon(self) -> IconName {
        match self {
            WindowControlType::Minimize => IconName::WindowMinimize,
            WindowControlType::Restore => IconName::WindowRestore,
            WindowControlType::Maximize => IconName::WindowMaximize,
            WindowControlType::Close => IconName::WindowClose,
        }
    }
}

pub struct WindowControlStyle {
    background: Hsla,
    background_hover: Hsla,
    icon: Hsla,
    icon_hover: Hsla,
}

impl WindowControlStyle {
    pub fn default(cx: &mut App) -> Self {
        let colors = cx.theme().colors();

        Self {
            background: colors.ghost_element_background,
            background_hover: colors.ghost_element_hover,
            icon: colors.icon,
            icon_hover: colors.icon_muted,
        }
    }

    pub fn background(mut self, color: impl Into<Hsla>) -> Self {
        self.background = color.into();
        self
    }

    pub fn background_hover(mut self, color: impl Into<Hsla>) -> Self {
        self.background_hover = color.into();
        self
    }

    pub fn icon(mut self, color: impl Into<Hsla>) -> Self {
        self.icon = color.into();
        self
    }

    pub fn icon_hover(mut self, color: impl Into<Hsla>) -> Self {
        self.icon_hover = color.into();
        self
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

    pub fn custom_style(
        id: impl Into<ElementId>,
        icon: WindowControlType,
        style: WindowControlStyle,
    ) -> Self {
        Self {
            id: id.into(),
            icon,
            style,
            close_action: None,
        }
    }
}

impl RenderOnce for WindowControl {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let icon = gpui::svg()
            .size_4()
            .flex_none()
            .path(self.icon.icon().path())
            .text_color(self.style.icon)
            .group_hover("", |this| this.text_color(self.style.icon_hover));

        h_flex()
            .id(self.id)
            .group("window-control")
            .cursor_pointer()
            .justify_center()
            .content_center()
            .rounded_2xl()
            .w_5()
            .h_5()
            .bg(self.style.background)
            .hover(|this| this.bg(self.style.background_hover))
            .active(|this| this.bg(self.style.background_hover))
            .child(icon)
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
