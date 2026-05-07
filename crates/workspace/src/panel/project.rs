use gpui::{
    Action, AnyElement, App, ClickEvent, Context, Div, Entity, FocusHandle, Focusable,
    ListSizingBehavior, Pixels, Render, Stateful, Subscription, Task, UniformListScrollHandle,
    WeakEntity, Window, prelude::*,
};
use std::ops::Range;

use actions::workspace::project_panel;
use project::{
    Entry, EntryKind, Project, ProjectEntryId, ProjectEvent, RequestFileState, Snapshot,
};
use theme::ActiveTheme;
use ui::{
    Color, Icon, IconName, IconSize, Label, LabelCommon, LabelSize, ListItem, ListItemSpacing,
    ScrollAxes, Scrollbars, TrackLayout, WithScrollbar,
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
    marked_entries: Vec<SelectedEntry>,
    selection: Option<SelectedEntry>,
    _project_subscription: Subscription,
}

#[derive(Default)]
struct TreeState {
    visible_entries: Vec<Entry>,
    expanded_dir_ids: Option<Vec<ProjectEntryId>>,
}

struct EntryDetails {
    file_name: String,
    prefix_label: Option<String>,
    depth: u16,
    kind: EntryKind,
    is_expanded: bool,
    is_invalid: bool,
    is_marked: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct SelectedEntry(ProjectEntryId);

impl ProjectPanel {
    const DEFAULT_SIZE: Pixels = gpui::px(250.0);
    const INDENT_SIZE: Pixels = gpui::px(13.0);
    const PANEL_KEY: &str = "ProjectPanel";
    const PREFIX_LABEL_SLOT_WIDTH: Pixels = gpui::px(26.0);

    pub fn new(project: &Entity<Project>, pane: WeakEntity<Pane>, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            focus_handle: cx.focus_handle(),
            project: project.clone(),
            pane,
            scroll_handle: UniformListScrollHandle::new(),
            update_visible_entries_task: Task::ready(()),
            tree_state: TreeState::default(),
            marked_entries: Vec::new(),
            selection: None,
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

    fn details_for_entry(&self, snapshot: &Snapshot, entry: &Entry) -> EntryDetails {
        let expanded_dir_ids = self.tree_state.expanded_dir_ids.as_deref().unwrap_or(&[]);
        let is_expanded = entry.kind.is_dir() && expanded_dir_ids.binary_search(&entry.id).is_ok();
        let raw_file_name = entry.path.file_name().unwrap_or_default();
        let file_name = match entry.kind {
            EntryKind::File => raw_file_name
                .strip_suffix(".toml")
                .unwrap_or(raw_file_name)
                .to_string(),
            EntryKind::Dir | EntryKind::PendingDir | EntryKind::UnloadedDir => {
                entry.path.file_name().map_or_else(
                    || snapshot.root_name().as_unix_str().to_string(),
                    |name| name.to_string(),
                )
            }
        };
        let depth = u16::try_from(entry.path.components().count()).unwrap_or(u16::MAX);
        let mut is_invalid = false;
        let prefix_label = match entry.request.as_ref() {
            Some(RequestFileState::Parsed(request)) => {
                let method = request.config.method.trim().to_ascii_uppercase();
                if method == "DELETE" {
                    Some("DEL".to_string())
                } else {
                    Some(method.chars().take(5).collect())
                }
            }
            Some(RequestFileState::Invalid(_)) => {
                is_invalid = true;
                None
            }
            None => None,
        };
        let selection = SelectedEntry(entry.id);

        EntryDetails {
            file_name,
            prefix_label,
            depth,
            kind: entry.kind,
            is_expanded,
            is_invalid,
            is_marked: self.marked_entries.contains(&selection),
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
                    let Some(snapshot) = snapshot else {
                        return Vec::new();
                    };
                    let mut entries = Vec::new();
                    let mut traversal = snapshot.entries(0);

                    while let Some(entry) = traversal.entry() {
                        entries.push(entry.clone());

                        if entry.kind.is_dir() && expanded_dir_ids.binary_search(&entry.id).is_err()
                        {
                            traversal.advance_to_sibling();
                        } else {
                            traversal.advance();
                        }
                    }

                    entries
                })
                .await;

            this.update(cx, |this, cx| {
                this.tree_state.visible_entries = visible_entries;
                cx.notify();
            })
            .ok();
        });
    }

    fn toggle_expanded(
        &mut self,
        entry_id: ProjectEntryId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(expanded_dir_ids) = self.tree_state.expanded_dir_ids.as_mut() else {
            return;
        };

        match expanded_dir_ids.binary_search(&entry_id) {
            Ok(index) => {
                expanded_dir_ids.remove(index);
            }
            Err(index) => {
                expanded_dir_ids.insert(index, entry_id);
            }
        }

        self.update_visible_entries(cx);
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    fn render_entry(
        entry_id: ProjectEntryId,
        details: EntryDetails,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let icon: AnyElement = if details.kind.is_dir() {
            let icon = if details.is_expanded {
                IconName::FolderOpen
            } else {
                IconName::FolderClose
            };

            ui::h_flex()
                .flex_none()
                .items_center()
                .child(Icon::new(icon).size(IconSize::Medium).color(Color::Muted))
                .into_any_element()
        } else if details.is_invalid {
            ui::h_flex()
                .w(Self::PREFIX_LABEL_SLOT_WIDTH)
                .flex_none()
                .items_center()
                .justify_end()
                .child(
                    Icon::new(IconName::Close)
                        .size(IconSize::Small)
                        .color(Color::Error),
                )
                .into_any_element()
        } else if let Some(prefix_label) = details.prefix_label {
            ui::h_flex()
                .w(Self::PREFIX_LABEL_SLOT_WIDTH)
                .flex_none()
                .items_center()
                .justify_end()
                .child(
                    Label::new(prefix_label)
                        .size(LabelSize::XSmall)
                        .color(Color::Muted)
                        .single_line(),
                )
                .into_any_element()
        } else {
            ui::h_flex()
                .flex_none()
                .items_center()
                .child(
                    Icon::new(IconName::File)
                        .size(IconSize::Medium)
                        .color(Color::Muted),
                )
                .into_any_element()
        };
        let is_dir = details.kind.is_dir();
        let selection = SelectedEntry(entry_id);
        let theme_colors = cx.theme().colors();
        let bg_color = if details.is_marked {
            theme_colors.element_selected
        } else {
            theme_colors.panel_background
        };
        let bg_hover_color = if details.is_marked {
            theme_colors.element_selected
        } else {
            theme_colors.element_hover
        };

        gpui::div()
            .id(entry_id.to_usize())
            .relative()
            .group("project-entry")
            .cursor_pointer()
            .rounded_none()
            .bg(bg_color)
            .border_1()
            .border_color(bg_color)
            .hover(move |style| style.bg(bg_hover_color).border_color(bg_hover_color))
            .on_click(
                cx.listener(move |project_panel, event: &ClickEvent, window, cx| {
                    if event.is_right_click() {
                        return;
                    }

                    cx.stop_propagation();

                    if is_dir {
                        project_panel.marked_entries.clear();
                        project_panel.toggle_expanded(entry_id, window, cx);
                    } else {
                        project_panel.marked_entries.clear();
                        project_panel.marked_entries.push(selection);
                        project_panel.selection = Some(selection);
                        window.focus(&project_panel.focus_handle, cx);
                        cx.notify();
                    }
                }),
            )
            .child(
                ListItem::new(entry_id.to_usize())
                    .indent_level(details.depth)
                    .indent_step_size(Self::INDENT_SIZE)
                    .spacing(ListItemSpacing::Dense)
                    .selectable(false)
                    .child(icon)
                    .child(
                        ui::h_flex()
                            .h_6()
                            .child(Label::new(details.file_name).single_line()),
                    )
                    .overflow_x(),
            )
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
            .bg(theme_colors.panel_background)
            .child(
                gpui::uniform_list(
                    "project-panel-entries",
                    entry_count,
                    cx.processor(|this, range: Range<usize>, _window, cx| {
                        let Some(snapshot) = this.snapshot(cx) else {
                            return Vec::new();
                        };
                        let mut items = Vec::with_capacity(range.end.saturating_sub(range.start));

                        for index in range {
                            if let Some(entry) = this.tree_state.visible_entries.get(index) {
                                items.push(Self::render_entry(
                                    entry.id,
                                    this.details_for_entry(&snapshot, entry),
                                    cx,
                                ));
                            }
                        }

                        items
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
                        theme_colors.panel_background,
                        TrackLayout::Overlay,
                    )
                    .notify_content(),
                window,
                cx,
            )
    }
}
