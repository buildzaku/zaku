use gpui::{AnyElement, App, IntoElement, ParentElement, RenderOnce, Window, prelude::*};

use crate::{ActiveTheme, Color, Icon, IconAsset, IconSize, Severity};

#[derive(IntoElement)]
pub struct Banner {
    severity: Severity,
    children: Vec<AnyElement>,
    action_slot: Option<AnyElement>,
    wrap_content: bool,
}

impl Banner {
    pub fn new() -> Self {
        Self {
            severity: Severity::Info,
            children: Vec::new(),
            action_slot: None,
            wrap_content: false,
        }
    }

    pub fn severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    pub fn action_slot(mut self, element: impl IntoElement) -> Self {
        self.action_slot = Some(element.into_any_element());
        self
    }

    pub fn wrap_content(mut self, wrap: bool) -> Self {
        self.wrap_content = wrap;
        self
    }
}

impl Default for Banner {
    fn default() -> Self {
        Self::new()
    }
}

impl ParentElement for Banner {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Banner {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let banner = gpui::div()
            .flex()
            .items_center()
            .min_w_0()
            .py_0p5()
            .gap_1p5()
            .when(self.wrap_content, |this| this.flex_wrap())
            .justify_between()
            .rounded_sm()
            .border_1();

        let (icon, icon_color, bg_color, border_color) = match self.severity {
            Severity::Info => (
                IconAsset::Info,
                Color::Muted,
                cx.theme().status().info_background.opacity(0.5),
                cx.theme().colors().border.opacity(0.5),
            ),
            Severity::Success => (
                IconAsset::Check,
                Color::Success,
                cx.theme().status().success.opacity(0.1),
                cx.theme().status().success.opacity(0.2),
            ),
            Severity::Warning => (
                IconAsset::Warning,
                Color::Warning,
                cx.theme().status().warning_background.opacity(0.5),
                cx.theme().status().warning_border.opacity(0.4),
            ),
            Severity::Error => (
                IconAsset::XCircle,
                Color::Error,
                cx.theme().status().error.opacity(0.1),
                cx.theme().status().error.opacity(0.2),
            ),
        };

        let mut banner = banner.bg(bg_color).border_color(border_color);

        let icon_and_child = gpui::div()
            .flex()
            .items_start()
            .min_w_0()
            .flex_1()
            .gap_1p5()
            .child(
                gpui::div()
                    .flex()
                    .items_center()
                    .h(window.line_height())
                    .flex_shrink_0()
                    .child(Icon::new(icon).size(IconSize::XSmall).color(icon_color)),
            )
            .child(gpui::div().min_w_0().flex_1().children(self.children));

        if let Some(action_slot) = self.action_slot {
            banner = banner
                .pl_2()
                .pr_1()
                .child(icon_and_child)
                .child(action_slot);
        } else {
            banner = banner.px_2().child(icon_and_child);
        }

        banner
    }
}
