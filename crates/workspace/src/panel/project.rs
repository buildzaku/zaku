use gpui::{
    Action, App, Context, Entity, FocusHandle, Focusable, ListSizingBehavior, Pixels, Render,
    Subscription, Task, UniformListScrollHandle, WeakEntity, Window, prelude::*,
};
use std::ops::Range;

use actions::workspace::project_panel;
use project::{EntryKind, Project, ProjectEntryId, ProjectEvent, Snapshot};
use theme::ActiveTheme;
use ui::{
    Color, Icon, IconName, IconSize, Label, LabelCommon, LabelSize, ScrollAxes, Scrollbars,
    WithScrollbar,
};

use crate::{Workspace, pane::Pane, panel::Panel};

pub fn init(cx: &mut App) {
    cx.observe_new(
        |workspace: &mut Workspace, _window, _: &mut Context<Workspace>| {
            workspace.register_action(|workspace, _: &project_panel::ToggleFocus, window, cx| {
                workspace.toggle_panel_focus::<ProjectPanel>(window, cx);
            });
        },
    )
    .detach();
}

pub struct ProjectPanel {
    focus_handle: FocusHandle,
    project: Entity<Project>,
    pane: WeakEntity<Pane>,
    scroll_handle: UniformListScrollHandle,
    update_visible_entries_task: Task<()>,
    tree_state: TreeState,
    _project_subscription: Subscription,
}

#[derive(Default)]
struct TreeState {
    visible_entries: Vec<VisibleEntry>,
    expanded_dir_ids: Option<Vec<ProjectEntryId>>,
}

struct VisibleEntries {
    entries: Vec<VisibleEntry>,
}

struct VisibleEntry {
    id: ProjectEntryId,
    depth: u16,
    kind: EntryKind,
    label: String,
}

struct EntryDetails {
    id: ProjectEntryId,
    depth: u16,
    kind: EntryKind,
    label: String,
}

impl ProjectPanel {
    const DEFAULT_SIZE: Pixels = gpui::px(250.0);
    const PANEL_KEY: &str = "ProjectPanel";

    pub fn new(project: &Entity<Project>, pane: WeakEntity<Pane>, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            focus_handle: cx.focus_handle(),
            project: project.clone(),
            pane,
            scroll_handle: UniformListScrollHandle::new(),
            update_visible_entries_task: Task::ready(()),
            tree_state: TreeState::default(),
            _project_subscription: cx.subscribe(project, |this, _, _: &ProjectEvent, cx| {
                this.update_visible_entries(cx);
            }),
        };
        this.update_visible_entries(cx);
        this
    }

    fn snapshot(&self, cx: &App) -> Option<Snapshot> {
        self.project.read(cx).snapshot(cx)
    }

    fn visible_entry(snapshot: &Snapshot, entry: &project::Entry) -> VisibleEntry {
        let file_name = entry.path.file_name().unwrap_or_default();
        let label = match entry.kind {
            EntryKind::File => file_name
                .strip_suffix(".toml")
                .unwrap_or(file_name)
                .to_string(),
            EntryKind::Dir | EntryKind::PendingDir | EntryKind::UnloadedDir => {
                if Some(entry) == snapshot.root_entry() {
                    snapshot.root_name().as_unix_str().to_string()
                } else {
                    file_name.to_string()
                }
            }
        };
        let depth = u16::try_from(entry.path.components().count()).unwrap_or(u16::MAX);

        VisibleEntry {
            id: entry.id,
            depth,
            kind: entry.kind,
            label,
        }
    }

    fn update_visible_entries(&mut self, cx: &mut Context<Self>) {
        let snapshot = self.snapshot(cx);

        if let Some(snapshot) = snapshot.as_ref() {
            if let Some(root_entry) = snapshot.root_entry()
                && self.tree_state.expanded_dir_ids.is_none()
            {
                self.tree_state.expanded_dir_ids = Some(vec![root_entry.id]);
            }
        } else {
            self.tree_state.expanded_dir_ids = None;
        }

        let expanded_dir_ids = self.tree_state.expanded_dir_ids.clone().unwrap_or_default();

        self.update_visible_entries_task = cx.spawn(async move |this, cx| {
            let visible_entries = cx
                .background_spawn(async move {
                    snapshot.map(|snapshot| {
                        let mut entries = Vec::new();
                        let mut traversal = snapshot.entries(0);

                        while let Some(entry) = traversal.entry() {
                            entries.push(Self::visible_entry(&snapshot, entry));

                            if entry.is_dir() && expanded_dir_ids.binary_search(&entry.id).is_err()
                            {
                                traversal.advance_to_sibling();
                            } else {
                                traversal.advance();
                            }
                        }

                        VisibleEntries { entries }
                    })
                })
                .await;

            this.update(cx, |this, cx| {
                this.tree_state.visible_entries = visible_entries
                    .map_or_else(Vec::new, |visible_entries| visible_entries.entries);
                cx.notify();
            })
            .ok();
        });
    }

    fn entry_details(entry: &VisibleEntry) -> EntryDetails {
        EntryDetails {
            id: entry.id,
            depth: entry.depth,
            kind: entry.kind,
            label: entry.label.clone(),
        }
    }

    fn render_entry(details: EntryDetails) -> impl IntoElement {
        let icon = match details.kind {
            EntryKind::File => IconName::File,
            EntryKind::Dir | EntryKind::PendingDir | EntryKind::UnloadedDir => IconName::FolderOpen,
        };
        let indentation = gpui::px(f32::from(details.depth) * 12.0);

        ui::h_flex()
            .id(details.id.to_usize())
            .w_full()
            .items_center()
            .gap_2()
            .px_3()
            .py_1()
            .child(gpui::div().w(indentation))
            .child(Icon::new(icon).size(IconSize::Small).color(Color::Muted))
            .child(Label::new(details.label).size(LabelSize::Small))
    }
}

impl Focusable for ProjectPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for ProjectPanel {
    fn panel_key() -> &'static str {
        Self::PANEL_KEY
    }

    fn default_size(&self, _window: &Window, _: &App) -> Pixels {
        Self::DEFAULT_SIZE
    }

    fn icon(&self, _window: &Window, _: &App) -> Option<IconName> {
        Some(IconName::Tree)
    }

    fn icon_tooltip(&self, _window: &Window, _: &App) -> Option<&'static str> {
        Some("Project Panel")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        project_panel::ToggleFocus.boxed_clone()
    }

    fn starts_open(&self, _window: &Window, cx: &App) -> bool {
        self.snapshot(cx)
            .and_then(|snapshot| snapshot.root_entry().cloned())
            .is_some_and(|entry| entry.is_dir())
    }

    fn activation_priority(&self) -> u32 {
        1
    }

    fn enabled(&self, cx: &App) -> bool {
        self.pane
            .upgrade()
            .is_some_and(|pane| !pane.read(cx).should_display_welcome_page())
            && self.project.read(cx).worktree(cx).is_some()
    }
}

impl Render for ProjectPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme_colors = cx.theme().colors();
        let entry_count = self.tree_state.visible_entries.len();

        gpui::div()
            .track_focus(&self.focus_handle)
            .flex()
            .flex_col()
            .size_full()
            .bg(theme_colors.surface_background)
            .child(
                gpui::uniform_list(
                    "project-panel-entries",
                    entry_count,
                    cx.processor(|this, range: Range<usize>, _window, _cx| {
                        range
                            .filter_map(|index| this.tree_state.visible_entries.get(index))
                            .map(|entry| Self::render_entry(Self::entry_details(entry)))
                            .collect::<Vec<_>>()
                    }),
                )
                .with_sizing_behavior(ListSizingBehavior::Infer)
                .track_scroll(&self.scroll_handle)
                .size_full(),
            )
            .custom_scrollbars(
                Scrollbars::new(ScrollAxes::Vertical)
                    .tracked_scroll_handle(&self.scroll_handle)
                    .with_track_along(
                        ScrollAxes::Vertical,
                        theme_colors.scrollbar_track_background,
                    )
                    .notify_content(),
                window,
                cx,
            )
    }
}
