use gpui::{
    Action, App, Context, FocusHandle, Focusable, FontWeight, Render, SharedString, WeakEntity,
    Window, prelude::*,
};
use jiff::Timestamp;
use std::path::{Path, PathBuf};

use metadata::{ZAKU_DESCRIPTION, ZAKU_NAME};
use theme::ActiveTheme;
use ui::{
    ButtonCommon, ButtonLike, ButtonSize, Clickable, Color, FixedWidth, Icon, IconAsset, IconSize,
    KeyBinding, Text, TextCommon, TextSize,
};

use crate::{OpenMode, Workspace, WorkspaceDb, WorkspaceId};

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
        gpui::div()
            .flex()
            .items_center()
            .px_1()
            .mb_2()
            .gap_2()
            .child(
                Text::new(self.title.to_ascii_uppercase())
                    .font_buffer(cx)
                    .color(Color::Muted)
                    .size(TextSize::XSmall),
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
    text: SharedString,
    icon: IconAsset,
    action: Box<dyn Action>,
}

impl SectionButton {
    fn new(
        text: impl Into<SharedString>,
        icon: IconAsset,
        action: &dyn Action,
        tab_index: usize,
        focus_handle: FocusHandle,
    ) -> Self {
        Self {
            focus_handle,
            tab_index,
            text: text.into(),
            icon,
            action: action.boxed_clone(),
        }
    }
}

impl RenderOnce for SectionButton {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = format!("welcome-button-{}-{}", self.text, self.tab_index);
        let action_ref = self.action.as_ref();
        let tab_index = isize::try_from(self.tab_index).expect("tab index should fit in isize");

        ButtonLike::new(id)
            .tab_index(tab_index)
            .full_width()
            .size(ButtonSize::Medium)
            .child(
                gpui::div()
                    .flex()
                    .items_center()
                    .w_full()
                    .justify_between()
                    .child(
                        gpui::div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Icon::new(self.icon)
                                    .color(Color::Muted)
                                    .size(IconSize::Small),
                            )
                            .child(Text::new(self.text)),
                    )
                    .child(
                        KeyBinding::for_action_in(action_ref, &self.focus_handle, cx)
                            .size(ui::rems_from_px(12.0)),
                    ),
            )
            .on_click(move |_, window, cx| {
                self.focus_handle.dispatch_action(&*self.action, window, cx);
            })
    }
}

struct SectionEntry {
    icon: IconAsset,
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
        gpui::div()
            .flex()
            .flex_col()
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

const CONTENT: Section<3> = Section {
    title: "Get Started",
    entries: [
        SectionEntry {
            icon: IconAsset::Plus,
            title: "New Project",
            action: &actions::workspace::NewProject,
        },
        SectionEntry {
            icon: IconAsset::FolderOpen,
            title: "Open Project",
            action: &actions::workspace::Open::DEFAULT,
        },
        SectionEntry {
            icon: IconAsset::ListSearch,
            title: "Open Command Palette",
            action: &actions::command_palette::Toggle,
        },
    ],
};

pub struct WelcomePage {
    workspace: WeakEntity<Workspace>,
    focus_handle: FocusHandle,
    recent_workspaces: Option<Vec<(WorkspaceId, PathBuf, Timestamp)>>,
}

impl WelcomePage {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        cx.on_focus(&focus_handle, window, |_, _, cx| cx.notify())
            .detach();

        let welcome_page = Self {
            workspace,
            focus_handle,
            recent_workspaces: None,
        };
        welcome_page.reload_recent_workspaces(window, cx);
        welcome_page
    }

    pub(crate) fn reload_recent_workspaces(&self, window: &mut Window, cx: &mut Context<Self>) {
        let fs = self
            .workspace
            .upgrade()
            .map(|workspace| workspace.read(cx).app_state().fs.clone());
        let workspace_db = WorkspaceDb::global(cx);
        cx.spawn_in(window, async move |this: WeakEntity<Self>, cx| {
            let Some(fs) = fs else {
                return;
            };
            let recent_workspaces = workspace_db
                .recent_workspaces_on_disk(fs.as_ref())
                .await
                .unwrap_or_else(|error| {
                    log::error!("Failed to load recent workspaces: {error}");
                    Vec::new()
                });

            if let Err(error) = this.update_in(cx, |welcome_page, _window, cx| {
                welcome_page.recent_workspaces = Some(recent_workspaces);
                cx.notify();
            }) {
                log::debug!("Failed to update welcome page recent workspaces: {error}");
            }
        })
        .detach();
    }

    fn select_next(
        _: &mut Self,
        _: &actions::menu::SelectNext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus_next(cx);
        cx.notify();
    }

    fn select_previous(
        _: &mut Self,
        _: &actions::menu::SelectPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus_prev(cx);
        cx.notify();
    }

    fn open_recent_project(
        &mut self,
        action: &actions::workspace::OpenRecentProject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(recent_workspaces) = self.recent_workspaces.as_ref() else {
            return;
        };
        let Some((_workspace_id, location, _timestamp)) = recent_workspaces.get(action.index)
        else {
            return;
        };
        let recent_workspace_path = location.clone();

        if let Err(error) = self.workspace.update(cx, |workspace, cx| {
            workspace
                .open_workspace_for_path(recent_workspace_path, OpenMode::Activate, window, cx)
                .detach_and_log_err(cx);
        }) {
            log::debug!("Failed to open recent workspace from welcome page: {error}");
        }
    }

    fn render_recent_project_section(recent_projects: Vec<impl IntoElement>) -> impl IntoElement {
        gpui::div()
            .flex()
            .flex_col()
            .w_full()
            .child(SectionHeader::new("Recent Projects"))
            .children(recent_projects)
    }

    fn render_recent_project(
        &self,
        project_index: usize,
        tab_index: usize,
        path: &Path,
    ) -> impl IntoElement {
        let title = path.file_name().map_or_else(
            || path.to_string_lossy().into_owned(),
            |file_name| file_name.to_string_lossy().into_owned(),
        );

        SectionButton::new(
            title,
            IconAsset::Folder,
            &actions::workspace::OpenRecentProject {
                index: project_index,
            },
            tab_index,
            self.focus_handle.clone(),
        )
    }
}

impl Focusable for WelcomePage {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for WelcomePage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let recent_projects = self
            .recent_workspaces
            .as_ref()
            .into_iter()
            .flatten()
            .take(5)
            .enumerate()
            .map(|(index, (_workspace_id, location, _timestamp))| {
                self.render_recent_project(index, CONTENT.entries.len() + index, location)
            })
            .collect::<Vec<_>>();

        let welcome_content = gpui::div()
            .flex()
            .flex_col()
            .flex_1()
            .justify_center()
            .overflow_hidden()
            .max_w_112()
            .mx_auto()
            .gap_6()
            .child(
                gpui::div()
                    .flex()
                    .items_center()
                    .w_full()
                    .justify_center()
                    .mb_4()
                    .child(
                        gpui::div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .child(
                                Text::new(format!("Welcome to {ZAKU_NAME}"))
                                    .size(TextSize::Large)
                                    .weight(FontWeight::MEDIUM),
                            )
                            .child(
                                Text::new(ZAKU_DESCRIPTION)
                                    .size(TextSize::Small)
                                    .color(Color::Muted)
                                    .italic(),
                            ),
                    ),
            )
            .child(CONTENT.render(0, &self.focus_handle, cx));

        let welcome_content = if recent_projects.is_empty() {
            welcome_content
        } else {
            welcome_content.child(Self::render_recent_project_section(recent_projects))
        };

        gpui::div()
            .flex()
            .items_center()
            .key_context("Welcome")
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::open_recent_project))
            .size_full()
            .justify_center()
            .overflow_hidden()
            .bg(cx.theme().colors().editor_background)
            .child(
                gpui::div()
                    .flex()
                    .items_center()
                    .relative()
                    .size_full()
                    .px_12()
                    .max_w(gpui::px(1100.0))
                    .child(welcome_content),
            )
    }
}
