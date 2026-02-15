use gpui::{Action, Context, Entity, Focusable, IntoElement, Render, Window};
use ui::{
    ButtonCommon, Color, ContextMenu, IconButton, IconButtonShape, IconName, IconSize, PopoverMenu,
    Tooltip,
};

use crate::{
    ToggleBottomDock, ToggleLeftDock, ToggleRightDock, pane::Pane, status_bar::StatusItemView,
};

pub struct ContextMenuDemo {
    active_pane: Entity<Pane>,
}

impl ContextMenuDemo {
    pub fn new(active_pane: Entity<Pane>, cx: &mut Context<Self>) -> Self {
        cx.observe(&active_pane, |_, _, cx| cx.notify()).detach();
        Self { active_pane }
    }
}

impl Render for ContextMenuDemo {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let action_context = self.active_pane.read(cx).focus_handle(cx);
        let action_context_for_menu = action_context.clone();
        let menu_builder = move |window: &mut Window, cx: &mut gpui::App| {
            let action_context_for_menu = action_context_for_menu.clone();
            Some(ContextMenu::build(window, cx, move |menu, _, _| {
                menu.context(action_context_for_menu.clone())
                    .header("Context Menu Demo")
                    .separator()
                    .action("Toggle Left Dock", ToggleLeftDock.boxed_clone())
                    .action("Toggle Bottom Dock", ToggleBottomDock.boxed_clone())
                    .action("Toggle Right Dock", ToggleRightDock.boxed_clone())
                    .separator()
                    .action_disabled_when(true, "Disabled Item", ToggleRightDock.boxed_clone())
                    .label("Non-selectable label")
            }))
        };

        PopoverMenu::new("context-menu-demo")
            .anchor(gpui::Corner::BottomRight)
            .attach(gpui::Corner::TopRight)
            .menu(menu_builder)
            .trigger_with_tooltip(
                IconButton::new("context-menu-demo-trigger", IconName::CaretDown)
                    .variant(ui::ButtonVariant::Subtle)
                    .icon_size(IconSize::Small)
                    .shape(IconButtonShape::Square)
                    .icon_color(Color::Muted)
                    .selected_icon_color(Color::Default),
                Tooltip::text("Context menu demo"),
            )
    }
}

impl StatusItemView for ContextMenuDemo {
    fn set_active_pane(&mut self, active_pane: &Entity<Pane>, cx: &mut Context<Self>) {
        self.active_pane = active_pane.clone();
        cx.notify();
    }
}
