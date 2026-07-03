use gpui::{Action, App, Context, Entity, Window, prelude::*};

use ui::{
    ActiveTheme, Color, ContextMenu, IconButton, IconButtonShape, IconName, IconSize, PopoverMenu,
    PopoverMenuHandle, SelectableButton, Tooltip,
};

pub(crate) struct ApplicationMenu {
    handle: PopoverMenuHandle<ContextMenu>,
}

impl ApplicationMenu {
    pub(crate) fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let handle = self.handle.clone();
        let selected_background = cx.theme().colors().ghost_element_hover;

        gpui::div()
            .key_context("ApplicationMenu")
            .id("application-menu-item")
            .occlude()
            .child(
                PopoverMenu::new("application-menu-popover")
                    .menu(move |window, cx| Some(Self::build_menu(window, cx)))
                    .offset(gpui::point(gpui::px(0.0), gpui::px(0.5)))
                    .trigger_with_tooltip(
                        IconButton::new("application-menu-trigger", IconName::Menu)
                            .shape(IconButtonShape::Square)
                            .icon_size(IconSize::Small)
                            .selected_background(selected_background)
                            .selected_icon_color(Color::Default),
                        Tooltip::text("Open Application Menu"),
                    )
                    .with_handle(handle),
            )
    }
}
