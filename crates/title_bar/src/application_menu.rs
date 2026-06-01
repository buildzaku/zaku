use gpui::{Action, Anchor, App, Context, Entity, Window, prelude::*};

use ui::{
    ButtonSize, ButtonVariant, ContextMenu, IconButton, IconButtonShape, IconName, IconSize,
    PopoverMenu, PopoverMenuHandle, prelude::*,
};

pub struct ApplicationMenu {
    handle: PopoverMenuHandle<ContextMenu>,
}

impl ApplicationMenu {
    pub fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self {
            handle: PopoverMenuHandle::default(),
        }
    }

    fn build_menu(window: &mut Window, cx: &mut App) -> Entity<ContextMenu> {
        ContextMenu::build(window, cx, |menu, _window, _cx| {
            menu.action(
                "Open Settings File",
                actions::zaku::OpenSettingsFile.boxed_clone(),
            )
            .action(
                "Open Keymap File",
                actions::zaku::OpenKeymapFile.boxed_clone(),
            )
        })
    }
}

impl Render for ApplicationMenu {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let handle = self.handle.clone();

        gpui::div().id("application-menu-item").occlude().child(
            PopoverMenu::new("application-menu-popover")
                .menu(move |window, cx| Some(Self::build_menu(window, cx)))
                .anchor(Anchor::TopRight)
                .offset(gpui::point(gpui::px(0.0), gpui::px(0.5)))
                .trigger(
                    IconButton::new("application-menu-trigger", IconName::Menu)
                        .variant(ButtonVariant::Subtle)
                        .size(ButtonSize::Compact)
                        .shape(IconButtonShape::Square)
                        .icon_size(IconSize::Small),
                )
                .with_handle(handle),
        )
    }
}
