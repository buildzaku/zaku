use gpui::{
    Action, App, Context, Entity, FocusHandle, Focusable, Pixels, Render, Subscription, Task,
    WeakEntity, Window, prelude::*,
};

use actions::workspace::project_panel;
use project::{EntryKind, Project, ProjectEvent, Snapshot};
use theme::ActiveTheme;
use ui::{Color, Icon, IconName, IconSize, Label, LabelCommon, LabelSize};

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
    update_visible_entries_task: Task<()>,
    visible_entries: Option<VisibleEntries>,
    _project_subscription: Subscription,
}

struct VisibleEntries {
    entries: Vec<VisibleEntry>,
}

struct VisibleEntry {
    depth: usize,
    kind: EntryKind,
    label: String,
}

struct EntryDetails {
    depth: usize,
    kind: EntryKind,
    label: String,
}

impl ProjectPanel {
    const DEFAULT_SIZE: Pixels = gpui::px(250.0);
    const PANEL_KEY: &str = "ProjectPanel";

    pub fn new(project: Entity<Project>, pane: WeakEntity<Pane>, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            focus_handle: cx.focus_handle(),
            project: project.clone(),
            pane,
            update_visible_entries_task: Task::ready(()),
            visible_entries: None,
            _project_subscription: cx.subscribe(&project, |this, _, _: &ProjectEvent, cx| {
                this.update_visible_entries(cx);
            }),
        };
        this.update_visible_entries(cx);
        this
    }

    fn snapshot(&self, cx: &App) -> Option<Snapshot> {
        self.project.read(cx).snapshot(cx)
    }

    fn root_label(snapshot: &Snapshot) -> String {
        if snapshot.root_name().is_empty() {
            snapshot.abs_path().to_string_lossy().into_owned()
        } else {
            snapshot
                .root_name()
                .display(snapshot.path_style())
                .into_owned()
        }
    }

    fn visible_entry(snapshot: &Snapshot, entry: &project::Entry) -> VisibleEntry {
        let file_name = entry.path.file_name().unwrap_or_default();
        let label = match entry.kind {
            EntryKind::File => file_name
                .strip_suffix(".toml")
                .unwrap_or(file_name)
                .to_string(),
            EntryKind::Dir | EntryKind::PendingDir | EntryKind::UnloadedDir => {
                if Some(entry) == snapshot.root_entry() && !snapshot.root_name().is_empty() {
                    snapshot
                        .root_name()
                        .display(snapshot.path_style())
                        .into_owned()
                } else {
                    file_name.to_string()
                }
            }
        };

        VisibleEntry {
            depth: entry.path.component_count().saturating_sub(1),
            kind: entry.kind,
            label,
        }
    }

    fn update_visible_entries(&mut self, cx: &mut Context<Self>) {
        let snapshot = self
            .project
            .read(cx)
            .worktree(cx)
            .map(|worktree| worktree.read(cx).snapshot());

        self.update_visible_entries_task = cx.spawn(async move |this, cx| {
            let visible_entries = cx
                .background_spawn(async move {
                    snapshot.map(|snapshot| VisibleEntries {
                        entries: snapshot
                            .entries(0)
                            .filter(|entry| !entry.path.is_empty())
                            .map(|entry| Self::visible_entry(&snapshot, entry))
                            .collect(),
                    })
                })
                .await;

            this.update(cx, |this, cx| {
                this.visible_entries = visible_entries;
                cx.notify();
            })
            .ok();
        });
    }

    fn entry_details(entry: &VisibleEntry) -> EntryDetails {
        EntryDetails {
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
        let indentation = gpui::px((details.depth as f32) * 12.0);

        ui::h_flex()
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme_colors = cx.theme().colors();
        let snapshot = self.snapshot(cx);
        let entries = self
            .visible_entries
            .as_ref()
            .into_iter()
            .flat_map(|visible_entries| visible_entries.entries.iter())
            .map(|entry| Self::render_entry(Self::entry_details(entry)))
            .collect::<Vec<_>>();

        gpui::div()
            .track_focus(&self.focus_handle)
            .flex()
            .flex_col()
            .size_full()
            .bg(theme_colors.surface_background)
            .when_some(
                snapshot.as_ref().map(Self::root_label),
                |panel, root_label| {
                    panel.child(
                        ui::h_flex()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .py_1()
                            .child(
                                Icon::new(IconName::FolderOpen)
                                    .size(IconSize::Small)
                                    .color(Color::Muted),
                            )
                            .child(Label::new(root_label).size(LabelSize::Small)),
                    )
                },
            )
            .child(
                gpui::div()
                    .id("project-panel-entries")
                    .flex_1()
                    .overflow_y_scroll()
                    .children(entries),
            )
    }
}
