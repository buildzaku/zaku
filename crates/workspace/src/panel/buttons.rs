use gpui::{Context, Entity, Focusable, IntoElement, ParentElement, Render, Styled};
use ui::{
    ButtonCommon, Clickable, Color, IconButton, IconButtonShape, IconSize, StyledTypography,
    Tooltip,
};

use crate::{dock::Dock, status_bar::StatusItemView};

pub struct PanelButtons {
    dock: Entity<Dock>,
}

impl PanelButtons {
    pub fn new(dock: Entity<Dock>, cx: &mut Context<Self>) -> Self {
        cx.observe(&dock, |_, _, cx| cx.notify()).detach();
        Self { dock }
    }
}

impl Render for PanelButtons {
    fn render(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dock = self.dock.read(cx);
        let active_index = dock.active_panel_index();
        let is_open = dock.is_open();
        let focus_handle = dock.focus_handle(cx);
        let buttons = dock
            .panel_entries()
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                let icon = entry.panel().icon(window, cx)?;
                let icon_tooltip = entry.panel().icon_tooltip(window, cx)?;
                let name = entry.panel().persistent_name();

                let is_active_button = Some(index) == active_index && is_open;
                let (action, tooltip) = if is_active_button {
                    let action = dock.toggle_action();
                    (action, format!("Close {} Dock", dock.position().label()))
                } else {
                    let action = entry.panel().toggle_action(window, cx);
                    (action, icon_tooltip.to_string())
                };

                let action = action.boxed_clone();
                let tooltip = tooltip.clone();
                let focus_handle = focus_handle.clone();

                Some(
                    IconButton::new(format!("{name}-button-{is_active_button}"), icon)
                        .variant(ui::ButtonVariant::Subtle)
                        .icon_size(IconSize::Small)
                        .shape(IconButtonShape::Square)
                        .icon_color(Color::Muted)
                        .selected_icon_color(Color::Default)
                        .toggle_state(is_active_button)
                        .tooltip(Tooltip::for_action_title(tooltip, action.as_ref()))
                        .on_click(move |_, window, cx| {
                            window.focus(&focus_handle, cx);
                            window.dispatch_action(action.boxed_clone(), cx);
                        }),
                )
            })
            .collect::<Vec<_>>();

        gpui::div()
            .flex()
            .flex_row()
            .gap_1()
            .children(buttons)
            .font_ui(cx)
            .text_ui_sm(cx)
    }
}

impl StatusItemView for PanelButtons {
    fn set_active_pane(
        &mut self,
        _active_pane: &Entity<crate::pane::Pane>,
        _cx: &mut Context<Self>,
    ) {
        // Panel buttons are not dependent on center-pane active item.
    }
}
