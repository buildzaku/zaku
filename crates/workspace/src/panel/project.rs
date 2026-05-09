use gpui::{
    Action, AnyElement, App, Bounds, ClickEvent, Context, Div, Entity, FocusHandle, Focusable,
    KeyContext, ListHorizontalSizingBehavior, ListSizingBehavior, MouseButton, Pixels, Render,
    ScrollStrategy, Stateful, Subscription, Task, UniformListScrollHandle, WeakEntity, Window,
    prelude::*,
};
use smallvec::SmallVec;
use std::{cmp, ops::Range};

use actions::{
    menu::{SelectFirst, SelectLast, SelectNext, SelectPrevious},
    workspace::project_panel,
};
use project::{
    Entry, EntryKind, Project, ProjectEntryId, ProjectEvent, RequestFileState, Snapshot,
};
use theme::ActiveTheme;
use ui::{
    Color, DynamicSpacing, Icon, IconName, IconSize, IndentGuideColors, IndentGuideLayout, Label,
    LabelCommon, LabelSize, ListItem, ListItemSpacing, RenderedIndentGuide, ScrollAxes, Scrollbars,
    TrackLayout, WithScrollbar,
};
use util::path::{SortMode, SortOrder};

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
    mouse_down: bool,
    _project_subscription: Subscription,
}

#[derive(Default)]
struct TreeState {
    visible_entries: Vec<Entry>,
    expanded_dir_ids: Option<Vec<ProjectEntryId>>,
    max_width_item_index: Option<usize>,
}

struct EntryDetails {
    file_name: String,
    prefix_label: Option<String>,
    depth: u16,
    kind: EntryKind,
    is_expanded: bool,
    is_invalid: bool,
    is_selected: bool,
    is_marked: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct SelectedEntry(ProjectEntryId);

impl ProjectPanel {
    const PANEL_KEY: &str = "ProjectPanel";
    const DEFAULT_SIZE: Pixels = gpui::px(250.0);
    const INDENT_SIZE: Pixels = gpui::px(9.0);
    const DISCLOSURE_SLOT_WIDTH: Pixels = gpui::px(13.0);
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
            mouse_down: false,
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

    fn dispatch_context() -> KeyContext {
        let mut dispatch_context = KeyContext::new_with_defaults();
        dispatch_context.add(Self::PANEL_KEY);
        dispatch_context.add("menu");
        dispatch_context.add("not_editing");
        dispatch_context
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
            is_selected: self.selection == Some(selection),
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
                        return (Vec::new(), None);
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

                    entries.sort_by(|lhs, rhs| {
                        cmp_worktree_entries(
                            lhs,
                            rhs,
                            SortMode::DirectoriesFirst,
                            SortOrder::Default,
                        )
                    });

                    let mut max_width_item = None;
                    for (index, entry) in entries.iter().enumerate() {
                        let entry_label = match entry.kind {
                            EntryKind::File => {
                                let name = entry.path.file_name().unwrap_or_default();
                                name.strip_suffix(".toml").unwrap_or(name)
                            }
                            EntryKind::Dir | EntryKind::PendingDir | EntryKind::UnloadedDir => {
                                entry
                                    .path
                                    .file_name()
                                    .unwrap_or_else(|| snapshot.root_name().as_unix_str())
                            }
                        };
                        let prefix_chars = usize::from(entry.request.is_some()) * 5;
                        let width_estimate = item_width_estimate(
                            entry.path.components().count(),
                            entry_label.chars().count() + prefix_chars,
                            false,
                        );

                        match max_width_item.as_mut() {
                            Some((widest_index, width)) if *width < width_estimate => {
                                *widest_index = index;
                                *width = width_estimate;
                            }
                            None => max_width_item = Some((index, width_estimate)),
                            _ => {}
                        }
                    }

                    (entries, max_width_item.map(|(index, _)| index))
                })
                .await;

            this.update(cx, |this, cx| {
                let (visible_entries, max_width_item_index) = visible_entries;
                this.tree_state.visible_entries = visible_entries;
                this.tree_state.max_width_item_index = max_width_item_index;
                cx.notify();
            })
            .ok();
        });
    }

    fn index_for_selection(&self, selection: SelectedEntry) -> Option<usize> {
        self.tree_state
            .visible_entries
            .iter()
            .position(|entry| entry.id == selection.0)
    }

    fn autoscroll(&mut self, cx: &mut Context<Self>) {
        if let Some(index) = self
            .selection
            .and_then(|selection| self.index_for_selection(selection))
        {
            self.scroll_handle
                .scroll_to_item(index, ScrollStrategy::Center);
            cx.notify();
        }
    }

    fn select_previous(&mut self, _: &SelectPrevious, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(selection) = self.selection {
            let Some(current_index) = self.index_for_selection(selection) else {
                return;
            };
            let Some(target_index) = current_index.checked_sub(1) else {
                return;
            };
            let Some(entry) = self.tree_state.visible_entries.get(target_index) else {
                return;
            };

            let selection = SelectedEntry(entry.id);
            self.selection = Some(selection);
            if window.modifiers().shift {
                self.marked_entries.push(selection);
            } else {
                self.marked_entries.clear();
                self.marked_entries.push(selection);
            }
            window.focus(&self.focus_handle, cx);
            self.autoscroll(cx);
        } else {
            self.select_first(&SelectFirst, window, cx);
        }
    }

    fn select_next(&mut self, _: &SelectNext, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(selection) = self.selection {
            let Some(current_index) = self.index_for_selection(selection) else {
                return;
            };
            let Some(target_index) = current_index.checked_add(1) else {
                return;
            };
            let Some(entry) = self.tree_state.visible_entries.get(target_index) else {
                return;
            };

            let selection = SelectedEntry(entry.id);
            self.selection = Some(selection);
            if window.modifiers().shift {
                self.marked_entries.push(selection);
            } else {
                self.marked_entries.clear();
                self.marked_entries.push(selection);
            }
            window.focus(&self.focus_handle, cx);
            self.autoscroll(cx);
        } else {
            self.select_first(&SelectFirst, window, cx);
        }
    }

    fn select_first(&mut self, _: &SelectFirst, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(entry) = self.tree_state.visible_entries.first() {
            let selection = SelectedEntry(entry.id);
            self.selection = Some(selection);
            if window.modifiers().shift {
                self.marked_entries.push(selection);
            } else {
                self.marked_entries.clear();
                self.marked_entries.push(selection);
            }
            window.focus(&self.focus_handle, cx);
            self.autoscroll(cx);
        }
    }

    fn select_last(&mut self, _: &SelectLast, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(entry) = self.tree_state.visible_entries.last() {
            let selection = SelectedEntry(entry.id);
            self.selection = Some(selection);
            if window.modifiers().shift {
                self.marked_entries.push(selection);
            } else {
                self.marked_entries.clear();
                self.marked_entries.push(selection);
            }
            window.focus(&self.focus_handle, cx);
            self.autoscroll(cx);
        }
    }

    fn expand_selected_entry(
        &mut self,
        _: &project_panel::ExpandSelectedEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = self.selection else {
            return;
        };
        let Some((entry_id, entry_kind)) = self
            .tree_state
            .visible_entries
            .iter()
            .find(|entry| entry.id == selection.0)
            .map(|entry| (entry.id, entry.kind))
        else {
            return;
        };

        if !entry_kind.is_dir() {
            return;
        }

        let search_result = {
            let Some(expanded_dir_ids) = self.tree_state.expanded_dir_ids.as_ref() else {
                return;
            };
            expanded_dir_ids.binary_search(&entry_id)
        };

        match search_result {
            Ok(_) => self.select_next(&SelectNext, window, cx),
            Err(index) => {
                let Some(expanded_dir_ids) = self.tree_state.expanded_dir_ids.as_mut() else {
                    return;
                };
                expanded_dir_ids.insert(index, entry_id);
                self.update_visible_entries(cx);
                window.focus(&self.focus_handle, cx);
                cx.notify();
            }
        }
    }

    fn collapse_selected_entry(
        &mut self,
        _: &project_panel::CollapseSelectedEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = self.selection else {
            return;
        };
        let Some(snapshot) = self.snapshot(cx) else {
            return;
        };
        let Some(mut entry) = snapshot.entry_for_id(selection.0).cloned() else {
            return;
        };

        let collapsed_entry_id = {
            let Some(expanded_dir_ids) = self.tree_state.expanded_dir_ids.as_mut() else {
                return;
            };
            let mut collapsed_entry_id = None;

            loop {
                if let Ok(index) = expanded_dir_ids.binary_search(&entry.id) {
                    expanded_dir_ids.remove(index);
                    collapsed_entry_id = Some(entry.id);
                    break;
                }

                let Some(parent_entry) = entry
                    .path
                    .parent()
                    .and_then(|path| snapshot.entry_for_path(path))
                    .cloned()
                else {
                    break;
                };
                entry = parent_entry;
            }

            collapsed_entry_id
        };

        if let Some(entry_id) = collapsed_entry_id {
            let selection = SelectedEntry(entry_id);
            self.selection = Some(selection);
            self.marked_entries.clear();
            self.marked_entries.push(selection);
            self.update_visible_entries(cx);
            window.focus(&self.focus_handle, cx);
            self.autoscroll(cx);
        }
    }

    fn collapse_selected_entry_and_children(
        &mut self,
        _: &project_panel::CollapseSelectedEntryAndChildren,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = self.selection else {
            return;
        };

        self.collapse_all_for_entry(selection.0, cx);
        self.update_visible_entries(cx);
        cx.notify();
    }

    fn collapse_all_entries(
        &mut self,
        _: &project_panel::CollapseAllEntries,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let snapshot = self.snapshot(cx);
        let Some(expanded_dir_ids) = self.tree_state.expanded_dir_ids.as_mut() else {
            return;
        };
        let Some(snapshot) = snapshot else {
            expanded_dir_ids.clear();
            self.update_visible_entries(cx);
            cx.notify();
            return;
        };

        if let Some(root_entry) = snapshot.root_entry() {
            expanded_dir_ids.retain(|entry_id| entry_id == &root_entry.id);
        } else {
            expanded_dir_ids.clear();
        }

        self.update_visible_entries(cx);
        cx.notify();
    }

    fn open(&mut self, _: &project_panel::Open, window: &mut Window, cx: &mut Context<Self>) {
        let Some(selection) = self.selection else {
            return;
        };
        let Some((entry_id, entry_kind)) = self
            .tree_state
            .visible_entries
            .iter()
            .find(|entry| entry.id == selection.0)
            .map(|entry| (entry.id, entry.kind))
        else {
            return;
        };

        if entry_kind.is_dir() {
            self.toggle_expanded(entry_id, window, cx);
        }
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

    fn toggle_expand_all(
        &mut self,
        entry_id: ProjectEntryId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(expanded_dir_ids) = self.tree_state.expanded_dir_ids.as_ref() else {
            return;
        };
        let is_expanded = expanded_dir_ids.binary_search(&entry_id).is_ok();
        if is_expanded {
            self.collapse_all_for_entry(entry_id, cx);
        } else {
            self.expand_all_for_entry(entry_id, cx);
        }

        self.update_visible_entries(cx);
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    fn expand_all_for_entry(&mut self, entry_id: ProjectEntryId, cx: &mut Context<Self>) {
        let Some(snapshot) = self.snapshot(cx) else {
            return;
        };
        let Some(expanded_dir_ids) = self.tree_state.expanded_dir_ids.as_mut() else {
            return;
        };

        let mut dirs_to_expand = vec![entry_id];
        while let Some(current_id) = dirs_to_expand.pop() {
            let Some(current_entry) = snapshot.entry_for_id(current_id) else {
                continue;
            };
            if !current_entry.is_dir() {
                continue;
            }

            if let Err(index) = expanded_dir_ids.binary_search(&current_id) {
                expanded_dir_ids.insert(index, current_id);
            }

            for child in snapshot.child_entries(&current_entry.path) {
                if child.is_dir() {
                    dirs_to_expand.push(child.id);
                }
            }
        }
    }

    fn collapse_all_for_entry(&mut self, entry_id: ProjectEntryId, cx: &mut Context<Self>) {
        let Some(snapshot) = self.snapshot(cx) else {
            return;
        };
        let Some(expanded_dir_ids) = self.tree_state.expanded_dir_ids.as_mut() else {
            return;
        };

        let mut dirs_to_collapse = vec![entry_id];
        while let Some(current_id) = dirs_to_collapse.pop() {
            let Some(current_entry) = snapshot.entry_for_id(current_id) else {
                continue;
            };

            if let Ok(index) = expanded_dir_ids.binary_search(&current_id) {
                expanded_dir_ids.remove(index);
            }

            for child in snapshot.child_entries(&current_entry.path) {
                if child.is_dir() {
                    dirs_to_collapse.push(child.id);
                }
            }
        }
    }

    fn find_active_indent_guide(&self, indent_guides: &[IndentGuideLayout]) -> Option<usize> {
        let selection = self.selection?;
        let selection_row = self.index_for_selection(selection)?;
        let selected_entry = self.tree_state.visible_entries.get(selection_row)?;
        let expanded_dir_ids = self.tree_state.expanded_dir_ids.as_deref().unwrap_or(&[]);
        let mut parent_row = selection_row;
        let mut depth = selected_entry.path.components().count();

        let is_expanded_dir = selected_entry.kind.is_dir()
            && expanded_dir_ids.binary_search(&selected_entry.id).is_ok();
        if !is_expanded_dir {
            depth = depth.checked_sub(1)?;
            parent_row = self.tree_state.visible_entries[..selection_row]
                .iter()
                .enumerate()
                .rev()
                .find_map(|(row, entry)| {
                    (entry.path.components().count() == depth).then_some(row)
                })?;
        }

        let start = parent_row.checked_add(1)?;
        let end = self.tree_state.visible_entries[start..]
            .iter()
            .position(|entry| entry.path.components().count() <= depth)
            .map_or(self.tree_state.visible_entries.len(), |offset| {
                start + offset
            });
        let active_range = start..end;

        indent_guides.iter().enumerate().find_map(|(index, guide)| {
            if guide.offset.x == depth
                && active_range.start <= guide.offset.y + guide.length
                && guide.offset.y <= active_range.end
            {
                Some(index)
            } else {
                None
            }
        })
    }

    fn render_entry_prefix(details: &EntryDetails) -> AnyElement {
        if details.kind.is_dir() {
            let icon = if details.is_expanded {
                IconName::FolderOpen
            } else {
                IconName::FolderClose
            };
            let disclosure_icon = if details.is_expanded {
                IconName::CaretDown
            } else {
                IconName::CaretRight
            };

            ui::h_flex()
                .flex_none()
                .items_center()
                .gap_0p5()
                .child(
                    gpui::div()
                        .w(Self::DISCLOSURE_SLOT_WIDTH)
                        .flex_none()
                        .items_center()
                        .justify_center()
                        .child(
                            Icon::new(disclosure_icon)
                                .size(IconSize::Small)
                                .color(Color::Muted),
                        ),
                )
                .child(Icon::new(icon).size(IconSize::Medium).color(Color::Muted))
                .into_any_element()
        } else if details.is_invalid {
            ui::h_flex()
                .flex_none()
                .items_center()
                .gap_0p5()
                .child(gpui::div().w(Self::DISCLOSURE_SLOT_WIDTH).flex_none())
                .child(
                    ui::h_flex()
                        .w(Self::PREFIX_LABEL_SLOT_WIDTH)
                        .flex_none()
                        .items_center()
                        .justify_end()
                        .child(
                            Icon::new(IconName::Close)
                                .size(IconSize::Small)
                                .color(Color::Error),
                        ),
                )
                .into_any_element()
        } else if let Some(prefix_label) = details.prefix_label.as_ref() {
            ui::h_flex()
                .flex_none()
                .items_center()
                .gap_0p5()
                .child(gpui::div().w(Self::DISCLOSURE_SLOT_WIDTH).flex_none())
                .child(
                    ui::h_flex()
                        .w(Self::PREFIX_LABEL_SLOT_WIDTH)
                        .flex_none()
                        .items_center()
                        .justify_end()
                        .child(
                            Label::new(prefix_label.clone())
                                .size(LabelSize::XSmall)
                                .color(Color::Muted)
                                .single_line(),
                        ),
                )
                .into_any_element()
        } else {
            ui::h_flex()
                .flex_none()
                .items_center()
                .gap_0p5()
                .child(gpui::div().w(Self::DISCLOSURE_SLOT_WIDTH).flex_none())
                .child(
                    Icon::new(IconName::File)
                        .size(IconSize::Medium)
                        .color(Color::Muted),
                )
                .into_any_element()
        }
    }

    fn render_entry(
        &self,
        entry_id: ProjectEntryId,
        details: EntryDetails,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let is_dir = details.kind.is_dir();
        let selection = SelectedEntry(entry_id);
        let is_active = details.is_selected;
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
        let border_color =
            if !self.mouse_down && is_active && self.focus_handle.contains_focused(window, cx) {
                theme_colors.border_focused
            } else {
                bg_color
            };
        let border_hover_color =
            if !self.mouse_down && is_active && self.focus_handle.contains_focused(window, cx) {
                theme_colors.border_focused
            } else {
                bg_hover_color
            };

        gpui::div()
            .id(entry_id.to_usize())
            .relative()
            .group("project-entry")
            .cursor_pointer()
            .rounded_none()
            .bg(bg_color)
            .border_1()
            .border_color(border_color)
            .hover(move |style| style.bg(bg_hover_color).border_color(border_hover_color))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |project_panel, _, _, cx| {
                    project_panel.mouse_down = true;
                    cx.propagate();
                }),
            )
            .on_click(
                cx.listener(move |project_panel, event: &ClickEvent, window, cx| {
                    if event.is_right_click() {
                        return;
                    }
                    if event.standard_click() {
                        project_panel.mouse_down = false;
                    }

                    cx.stop_propagation();

                    if is_dir {
                        project_panel.marked_entries.clear();
                        project_panel.marked_entries.push(selection);
                        project_panel.selection = Some(selection);
                        if window.modifiers().alt {
                            project_panel.toggle_expand_all(entry_id, window, cx);
                        } else {
                            project_panel.toggle_expanded(entry_id, window, cx);
                        }
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
                    .child(Self::render_entry_prefix(&details))
                    .child(
                        ui::h_flex()
                            .h_6()
                            .child(Label::new(details.file_name).single_line()),
                    )
                    .overflow_x(),
            )
    }
}

#[inline]
fn cmp_worktree_entries(a: &Entry, b: &Entry, mode: SortMode, order: SortOrder) -> cmp::Ordering {
    let a = (a.path.as_ref(), a.is_file());
    let b = (b.path.as_ref(), b.is_file());
    util::path::compare_rel_paths_by(a, b, mode, order)
}

fn item_width_estimate(depth: usize, item_text_chars: usize, is_symlink: bool) -> usize {
    const ICON_SIZE_FACTOR: usize = 2;
    let mut item_width = depth * ICON_SIZE_FACTOR + item_text_chars;
    if is_symlink {
        item_width += ICON_SIZE_FACTOR;
    }
    item_width
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
            .key_context(Self::dispatch_context())
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_first))
            .on_action(cx.listener(Self::select_last))
            .on_action(cx.listener(Self::expand_selected_entry))
            .on_action(cx.listener(Self::collapse_selected_entry))
            .on_action(cx.listener(Self::collapse_all_entries))
            .on_action(cx.listener(Self::collapse_selected_entry_and_children))
            .on_action(cx.listener(Self::open))
            .flex()
            .flex_col()
            .size_full()
            .bg(theme_colors.panel_background)
            .child(
                gpui::uniform_list(
                    "project-panel-entries",
                    entry_count,
                    cx.processor(|this, range: Range<usize>, window, cx| {
                        let Some(snapshot) = this.snapshot(cx) else {
                            return Vec::new();
                        };
                        let mut items = Vec::with_capacity(range.end.saturating_sub(range.start));

                        for index in range {
                            if let Some(entry) = this.tree_state.visible_entries.get(index) {
                                items.push(this.render_entry(
                                    entry.id,
                                    this.details_for_entry(&snapshot, entry),
                                    window,
                                    cx,
                                ));
                            }
                        }

                        items
                    }),
                )
                .with_decoration(
                    ui::indent_guides(Self::INDENT_SIZE, IndentGuideColors::panel(cx))
                        .with_compute_indents_fn(cx.entity(), |this, range, _window, _cx| {
                            let mut items =
                                SmallVec::with_capacity(range.end.saturating_sub(range.start));
                            for index in range {
                                if let Some(entry) = this.tree_state.visible_entries.get(index) {
                                    items.push(entry.path.components().count());
                                }
                            }
                            items
                        })
                        .on_click(cx.listener(
                            |this, active_indent_guide: &IndentGuideLayout, window, cx| {
                                if !window.modifiers().secondary() {
                                    return;
                                }

                                let row = active_indent_guide.offset.y;
                                let Some(snapshot) = this.snapshot(cx) else {
                                    return;
                                };
                                let Some(parent_entry_id) = this
                                    .tree_state
                                    .visible_entries
                                    .get(row)
                                    .and_then(|entry| entry.path.parent())
                                    .and_then(|path| snapshot.entry_for_path(path))
                                    .map(|entry| entry.id)
                                else {
                                    return;
                                };
                                let Some(expanded_dir_ids) =
                                    this.tree_state.expanded_dir_ids.as_mut()
                                else {
                                    return;
                                };
                                let Ok(index) = expanded_dir_ids.binary_search(&parent_entry_id)
                                else {
                                    return;
                                };

                                expanded_dir_ids.remove(index);
                                this.update_visible_entries(cx);
                                window.focus(&this.focus_handle, cx);
                                cx.notify();
                            },
                        ))
                        .with_render_fn(cx.entity(), |this, params, _, cx| {
                            const HITBOX_OVERDRAW: Pixels = gpui::px(3.0);
                            const PADDING_Y: Pixels = gpui::px(1.0);

                            let active_guide = this.find_active_indent_guide(&params.indent_guides);
                            let indent_size = params.indent_size;
                            let item_height = params.item_height;
                            let left_offset = DynamicSpacing::Base06.px(cx)
                                + Self::DISCLOSURE_SLOT_WIDTH * 0.5
                                - gpui::px(0.5);

                            params
                                .indent_guides
                                .into_iter()
                                .enumerate()
                                .map(|(index, layout)| {
                                    let guide_x = layout.offset.x * indent_size + left_offset;
                                    let guide_y = layout.offset.y * item_height + PADDING_Y;
                                    let guide_height =
                                        layout.length * item_height - PADDING_Y * 2.0;
                                    let bounds = Bounds::new(
                                        gpui::point(guide_x, guide_y),
                                        gpui::size(gpui::px(1.0), guide_height),
                                    );
                                    let hitbox_x = bounds.origin.x - HITBOX_OVERDRAW;
                                    let hitbox_width = bounds.size.width + HITBOX_OVERDRAW * 2.0;

                                    RenderedIndentGuide {
                                        bounds,
                                        layout,
                                        is_active: Some(index) == active_guide,
                                        hitbox: Some(Bounds::new(
                                            gpui::point(hitbox_x, bounds.origin.y),
                                            gpui::size(hitbox_width, bounds.size.height),
                                        )),
                                    }
                                })
                                .collect()
                        }),
                )
                .with_sizing_behavior(ListSizingBehavior::Infer)
                .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
                .with_width_from_item(self.tree_state.max_width_item_index)
                .track_scroll(&self.scroll_handle)
                .size_full(),
            )
            .custom_scrollbars(
                Scrollbars::new(ScrollAxes::Both)
                    .tracked_scroll_handle(&self.scroll_handle)
                    .with_track_along(
                        ScrollAxes::Vertical,
                        theme_colors.panel_background,
                        TrackLayout::Overlay,
                    )
                    .with_track_along(
                        ScrollAxes::Horizontal,
                        theme_colors.panel_background,
                        TrackLayout::Classic,
                    )
                    .notify_content(),
                window,
                cx,
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use gpui::TestAppContext;
    use serde_json::json;
    use std::sync::Arc;

    use fs::TempFs;
    use util_macros::path;

    fn visible_entries_as_strings(
        panel: &ProjectPanel,
        range: Range<usize>,
        cx: &App,
    ) -> Vec<String> {
        let snapshot = panel
            .snapshot(cx)
            .expect("project panel should have a snapshot");

        panel
            .tree_state
            .visible_entries
            .iter()
            .skip(range.start)
            .take(range.end.saturating_sub(range.start))
            .map(|entry| {
                let details = panel.details_for_entry(&snapshot, entry);
                let indent = "    ".repeat(usize::from(details.depth));
                let icon = if details.kind.is_dir() {
                    if details.is_expanded { "v " } else { "> " }
                } else {
                    "  "
                };
                let marked = if details.is_marked {
                    "  <== marked"
                } else {
                    ""
                };

                format!("{indent}{icon}{}{marked}", details.file_name)
            })
            .collect()
    }

    #[gpui::test]
    async fn test_sort_mode_directories_first(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = Arc::new(TempFs::new(cx.executor()));
        temp_fs.insert_tree(
            path!("project"),
            json!({
                "zebra.toml": "",
                "Apple": {},
                "banana.toml": "",
                "Carrot": {},
                "aardvark.toml": "",
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs, &project_path, cx).await;
        let panel = cx.new(|cx| ProjectPanel::new(&project, WeakEntity::new_invalid(), cx));

        cx.run_until_parked();

        let actual = panel.read_with(cx, |panel, cx| visible_entries_as_strings(panel, 0..50, cx));

        assert_eq!(
            actual,
            vec![
                String::from("v project"),
                String::from("    > Apple"),
                String::from("    > Carrot"),
                String::from("      aardvark"),
                String::from("      banana"),
                String::from("      zebra"),
            ]
        );
    }
}
