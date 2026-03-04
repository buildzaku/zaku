use gpui::{
    Action, App, Context, FocusHandle, Focusable, FontWeight, Render, SharedString, WeakEntity,
    Window, prelude::*,
};

use menu::{SelectNext, SelectPrevious};
use theme::ActiveTheme;
use ui::{
    ButtonCommon, ButtonLike, ButtonSize, Clickable, Color, FixedWidth, Icon, IconName, IconSize,
    KeyBinding, Label, LabelCommon, LabelSize,
};

use crate::{OpenWorkspace, Workspace};

#[derive(IntoElement)]
struct SectionHeader {
    title: SharedString,
}

impl SectionHeader {
    fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
        }
    }
}

impl RenderOnce for SectionHeader {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        ui::h_flex()
            .px_1()
            .mb_2()
            .gap_2()
            .child(
                Label::new(self.title.to_ascii_uppercase())
                    .buffer_font(cx)
                    .color(Color::Muted)
                    .size(LabelSize::XSmall),
            )
            .child(
                gpui::div()
                    .h_px()
                    .flex_1()
                    .bg(cx.theme().colors().border_variant),
            )
    }
}

#[derive(IntoElement)]
struct SectionButton {
    focus_handle: FocusHandle,
    tab_index: usize,
    label: SharedString,
    icon: IconName,
    action: Box<dyn Action>,
}

impl SectionButton {
    fn new(
        label: impl Into<SharedString>,
        icon: IconName,
        action: &dyn Action,
        tab_index: usize,
        focus_handle: FocusHandle,
    ) -> Self {
        Self {
            focus_handle,
            tab_index,
            label: label.into(),
            icon,
            action: action.boxed_clone(),
        }
    }
}

impl RenderOnce for SectionButton {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = format!("welcome-button-{}-{}", self.label, self.tab_index);
        let action_ref = self.action.as_ref();

        ButtonLike::new(id)
            .tab_index(self.tab_index as isize)
            .full_width()
            .size(ButtonSize::Medium)
            .child(
                ui::h_flex()
                    .w_full()
                    .justify_between()
                    .child(
                        ui::h_flex()
                            .gap_2()
                            .child(
                                Icon::new(self.icon)
                                    .color(Color::Muted)
                                    .size(IconSize::Small),
                            )
                            .child(Label::new(self.label)),
                    )
                    .child(
                        KeyBinding::for_action_in(action_ref, &self.focus_handle, cx)
                            .size(ui::rems_from_px(12.0)),
                    ),
            )
            .on_click(move |_, window, cx| {
                self.focus_handle.dispatch_action(&*self.action, window, cx)
            })
    }
}

struct SectionEntry {
    icon: IconName,
    title: &'static str,
    action: &'static dyn Action,
}

impl SectionEntry {
    fn render(&self, button_index: usize, focus: &FocusHandle, _cx: &App) -> impl IntoElement {
        SectionButton::new(
            self.title,
            self.icon,
            self.action,
            button_index,
            focus.clone(),
        )
    }
}

struct Section<const COLS: usize> {
    title: &'static str,
    entries: [SectionEntry; COLS],
}

impl<const COLS: usize> Section<COLS> {
    fn render(self, index_offset: usize, focus: &FocusHandle, cx: &App) -> impl IntoElement {
        ui::v_flex()
            .min_w_full()
            .child(SectionHeader::new(self.title))
            .children(
                self.entries
                    .iter()
                    .enumerate()
                    .map(|(index, entry)| entry.render(index_offset + index, focus, cx)),
            )
    }
}

const CONTENT: Section<1> = Section {
    title: "Get Started",
    entries: [SectionEntry {
        icon: IconName::FolderOpen,
        title: "Open Project",
        action: &OpenWorkspace,
    }],
};

pub struct WelcomePage {
    focus_handle: FocusHandle,
    fallback_to_recent_projects: bool,
}

impl WelcomePage {
    pub fn new(
        _workspace: WeakEntity<Workspace>,
        fallback_to_recent_projects: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        cx.on_focus(&focus_handle, window, |_, _, cx| cx.notify())
            .detach();

        Self {
            focus_handle,
            fallback_to_recent_projects,
        }
    }

    fn select_next(&mut self, _: &SelectNext, window: &mut Window, cx: &mut Context<Self>) {
        window.focus_next(cx);
        cx.notify();
    }

    fn select_previous(&mut self, _: &SelectPrevious, window: &mut Window, cx: &mut Context<Self>) {
        window.focus_prev(cx);
        cx.notify();
    }
}

impl Focusable for WelcomePage {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for WelcomePage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let welcome_label = if self.fallback_to_recent_projects {
            "Welcome back to Zaku"
        } else {
            "Welcome to Zaku"
        };

        ui::h_flex()
            .key_context("Welcome")
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_next))
            .size_full()
            .justify_center()
            .overflow_hidden()
            .bg(cx.theme().colors().editor_background)
            .child(
                ui::h_flex()
                    .relative()
                    .size_full()
                    .px_12()
                    .max_w(gpui::px(1100.0))
                    .child(
                        ui::v_flex()
                            .flex_1()
                            .justify_center()
                            .overflow_hidden()
                            .max_w_112()
                            .mx_auto()
                            .gap_6()
                            .child(
                                ui::h_flex().w_full().justify_center().mb_4().child(
                                    ui::v_flex()
                                        .items_center()
                                        .child(
                                            Label::new(welcome_label)
                                                .size(LabelSize::Large)
                                                .weight(FontWeight::MEDIUM),
                                        )
                                        .child(
                                            Label::new("Fast, open-source API client with fangs.")
                                                .size(LabelSize::Small)
                                                .color(Color::Muted)
                                                .italic(),
                                        ),
                                ),
                            )
                            .child(CONTENT.render(0, &self.focus_handle, cx)),
                    ),
            )
    }
}
