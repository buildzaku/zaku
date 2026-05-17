use gpui::{
    Action, AnyElement, App, Bounds, ClickEvent, Context, Div, Entity, EventEmitter, FocusHandle,
    Focusable, FontWeight, KeyContext, ListHorizontalSizingBehavior, ListSizingBehavior,
    MouseButton, Pixels, Render, ScrollStrategy, Stateful, Subscription, Task,
    UniformListScrollHandle, WeakEntity, Window, prelude::*,
};
use smallvec::SmallVec;
use std::{cmp, ops::Range, path::Path, sync::Arc};

use actions::{
    menu::{Cancel, Confirm, SelectFirst, SelectLast, SelectNext, SelectPrevious},
    workspace::project_panel,
};
use editor::{Editor, EditorEvent};
use project::{
    Entry, EntryKind, Project, ProjectEntryId, ProjectEvent, ProjectPath, RequestFileState,
    Snapshot, WorktreeId,
};
use theme::ActiveTheme;
use ui::{
    Color, DynamicSpacing, Icon, IconName, IconSize, IndentGuideColors, IndentGuideLayout, Label,
    LabelCommon, LabelSize, ListItem, ListItemSpacing, RenderedIndentGuide, ScrollAxes, Scrollbars,
    TrackLayout, WithScrollbar,
};
use util::{
    path::{PathStyle, SortMode, SortOrder},
    rel_path::RelPath,
};

use workspace::{Panel, Workspace, pane::Pane};

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

struct UpdateVisibleEntriesTask {
    _visible_entries_task: Task<()>,
    focus_file_name_editor: bool,
    autoscroll: bool,
}

impl Default for UpdateVisibleEntriesTask {
    fn default() -> Self {
        Self {
            _visible_entries_task: Task::ready(()),
            focus_file_name_editor: false,
            autoscroll: false,
        }
    }
}

#[derive(Default)]
struct TreeState {
    visible_entries: Vec<Entry>,
    expanded_dir_ids: Option<Vec<ProjectEntryId>>,
    max_width_item_index: Option<usize>,
    edit_state: Option<EditState>,
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
    is_editing: bool,
    is_processing: bool,
}

#[derive(Debug)]
pub enum Event {
    OpenedEntry {
        entry_id: ProjectEntryId,
        focus_opened_item: bool,
        allow_preview: bool,
    },
}

impl EventEmitter<Event> for ProjectPanel {}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct SelectedEntry(ProjectEntryId);

#[derive(Clone, Debug)]
struct EditState {
    worktree_id: WorktreeId,
    entry_id: ProjectEntryId,
    is_dir: bool,
    processing_file_name: Option<Arc<RelPath>>,
    previously_focused: Option<SelectedEntry>,
}

pub struct ProjectPanel {
    focus_handle: FocusHandle,
    project: Entity<Project>,
    pane: WeakEntity<Pane>,
    scroll_handle: UniformListScrollHandle,
    update_visible_entries_task: UpdateVisibleEntriesTask,
    tree_state: TreeState,
    marked_entries: Vec<SelectedEntry>,
    selection: Option<SelectedEntry>,
    file_name_editor: Entity<Editor>,
    mouse_down: bool,
    _project_subscription: Subscription,
}

impl ProjectPanel {
    const PANEL_KEY: &str = "ProjectPanel";
    const DEFAULT_SIZE: Pixels = gpui::px(250.0);
    const INDENT_SIZE: Pixels = gpui::px(9.0);
    const DISCLOSURE_SLOT_WIDTH: Pixels = gpui::px(13.0);
    const PREFIX_LABEL_SLOT_WIDTH: Pixels = gpui::px(32.0);
    const NEW_ENTRY_ID: ProjectEntryId = ProjectEntryId::MAX;

    pub fn new(
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Entity<Self> {
        let project = workspace.project().clone();
        let pane = workspace.pane().downgrade();
        let project_panel = cx.new(|cx| {
            let file_name_editor = cx.new(|cx| Editor::single_line(window, cx));
            cx.subscribe_in(
                &file_name_editor,
                window,
                |project_panel: &mut ProjectPanel, _, editor_event, window, cx| match editor_event {
                    EditorEvent::BufferEdited => {
                        project_panel.autoscroll(cx);
                    }
                    EditorEvent::Blurred => {
                        if project_panel
                            .tree_state
                            .edit_state
                            .as_ref()
                            .is_some_and(|edit_state| edit_state.processing_file_name.is_none())
                        {
                            match project_panel.confirm_edit(false, window, cx) {
                                Some(task) => task.detach_and_log_err(cx),
                                None => project_panel.discard_edit_state(window, cx),
                            }
                        }
                    }
                },
            )
            .detach();

            let project_subscription = cx.subscribe_in(
                &project,
                window,
                |this: &mut ProjectPanel, _, _: &ProjectEvent, window, cx| {
                    this.update_visible_entries(None, false, false, window, cx);
                },
            );

            let mut this = Self {
                focus_handle: cx.focus_handle(),
                project: project.clone(),
                pane: pane.clone(),
                scroll_handle: UniformListScrollHandle::new(),
                update_visible_entries_task: UpdateVisibleEntriesTask::default(),
                tree_state: TreeState::default(),
                marked_entries: Vec::new(),
                selection: None,
                file_name_editor,
                mouse_down: false,
                _project_subscription: project_subscription,
            };
            this.update_visible_entries(None, false, false, window, cx);
            this
        });

        cx.subscribe_in(&project_panel, window, {
            let project_panel = project_panel.downgrade();
            move |workspace, _, event, window, cx| match event {
                &Event::OpenedEntry {
                    entry_id,
                    focus_opened_item,
                    allow_preview,
                } => {
                    let Some(worktree) = project.read(cx).worktree_for_entry(entry_id, cx) else {
                        return;
                    };
                    let Some(entry) = worktree.read(cx).entry_for_id(entry_id) else {
                        return;
                    };

                    let file_path = entry.path.clone();
                    let worktree_id = worktree.read(cx).id();
                    let entry_id = entry.id;

                    workspace
                        .open_path_preview(
                            ProjectPath {
                                worktree_id,
                                path: file_path,
                            },
                            None,
                            focus_opened_item,
                            allow_preview,
                            true,
                            window,
                            cx,
                        )
                        .detach_and_log_err(cx);

                    if let Some(project_panel) = project_panel.upgrade() {
                        let entry = SelectedEntry(entry_id);
                        project_panel.update(cx, |project_panel, _| {
                            project_panel.marked_entries.clear();
                            project_panel.marked_entries.push(entry);
                            project_panel.selection = Some(entry);
                        });

                        if !focus_opened_item {
                            let focus_handle = project_panel.read(cx).focus_handle.clone();
                            window.focus(&focus_handle, cx);
                        }
                    }
                }
            }
        })
        .detach();

        project_panel
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
        let file_name = file_name_for_entry(snapshot, entry);
        let depth = u16::try_from(entry.path.components().count()).unwrap_or(u16::MAX);
        let mut is_invalid = false;
        let prefix_label = match entry.request.as_ref() {
            Some(RequestFileState::Parsed(request)) => {
                let method = request.http.method.trim().to_ascii_uppercase();
                Some(match method.as_str() {
                    "GET" => "GET".to_string(),
                    "POST" => "POST".to_string(),
                    "PUT" => "PUT".to_string(),
                    "PATCH" => "PATCH".to_string(),
                    "DELETE" => "DEL".to_string(),
                    "HEAD" => "HEAD".to_string(),
                    "OPTIONS" => "OPT".to_string(),
                    _ => method.chars().take(5).collect(),
                })
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
            is_editing: false,
            is_processing: false,
        }
    }

    fn update_visible_entries(
        &mut self,
        new_selected_entry: Option<ProjectEntryId>,
        focus_file_name_editor: bool,
        autoscroll: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        let edit_state = self.tree_state.edit_state.clone();

        let visible_entries_task = cx.spawn_in(window, async move |this, cx| {
            let visible_entries = cx
                .background_spawn(async move {
                    let Some(snapshot) = snapshot else {
                        return (Vec::new(), None);
                    };
                    let mut entries = Vec::new();
                    let mut traversal = snapshot.entries(0);
                    let mut new_entry_parent_id = None;
                    let mut new_entry_kind = EntryKind::Dir;
                    if let Some(edit_state) = &edit_state {
                        new_entry_parent_id = Some(edit_state.entry_id);
                        new_entry_kind = if edit_state.is_dir {
                            EntryKind::Dir
                        } else {
                            EntryKind::File
                        };
                    }

                    while let Some(entry) = traversal.entry() {
                        entries.push(entry.clone());
                        if new_entry_parent_id == Some(entry.id) {
                            entries.push(Self::create_new_entry(entry, new_entry_kind));
                        }

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
                        let entry_label = file_name_for_entry(&snapshot, entry);
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

            this.update_in(cx, |this, window, cx| {
                let (visible_entries, max_width_item_index) = visible_entries;
                this.tree_state.visible_entries = visible_entries;
                this.tree_state.max_width_item_index = max_width_item_index;
                if let Some(entry_id) = new_selected_entry {
                    this.selection = Some(SelectedEntry(entry_id));
                }
                if this.update_visible_entries_task.focus_file_name_editor {
                    this.update_visible_entries_task.focus_file_name_editor = false;
                    this.file_name_editor.update(cx, |editor, cx| {
                        window.focus(&editor.focus_handle(cx), cx);
                    });
                }
                if this.update_visible_entries_task.autoscroll {
                    this.update_visible_entries_task.autoscroll = false;
                    this.autoscroll(cx);
                }
                cx.notify();
            })
            .ok();
        });

        self.update_visible_entries_task = UpdateVisibleEntriesTask {
            _visible_entries_task: visible_entries_task,
            focus_file_name_editor: focus_file_name_editor
                || self.update_visible_entries_task.focus_file_name_editor,
            autoscroll: autoscroll || self.update_visible_entries_task.autoscroll,
        };
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
            }
            window.focus(&self.focus_handle, cx);
            self.autoscroll(cx);
        }
    }

    fn select_last(&mut self, _: &SelectLast, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(entry) = self.tree_state.visible_entries.last() {
            let selection = SelectedEntry(entry.id);
            self.selection = Some(selection);
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
                self.update_visible_entries(None, false, false, window, cx);
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
            self.update_visible_entries(None, false, false, window, cx);
            window.focus(&self.focus_handle, cx);
            self.autoscroll(cx);
        }
    }

    fn collapse_selected_entry_and_children(
        &mut self,
        _: &project_panel::CollapseSelectedEntryAndChildren,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = self.selection else {
            return;
        };

        self.collapse_all_for_entry(selection.0, cx);
        self.update_visible_entries(None, false, false, window, cx);
        cx.notify();
    }

    fn collapse_all_entries(
        &mut self,
        _: &project_panel::CollapseAllEntries,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let snapshot = self.snapshot(cx);
        let Some(expanded_dir_ids) = self.tree_state.expanded_dir_ids.as_mut() else {
            return;
        };
        let Some(snapshot) = snapshot else {
            expanded_dir_ids.clear();
            self.update_visible_entries(None, false, false, window, cx);
            cx.notify();
            return;
        };

        if let Some(root_entry) = snapshot.root_entry() {
            expanded_dir_ids.retain(|entry_id| entry_id == &root_entry.id);
        } else {
            expanded_dir_ids.clear();
        }

        self.update_visible_entries(None, false, false, window, cx);
        cx.notify();
    }

    fn new_file(
        &mut self,
        _: &project_panel::NewFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.add_entry(false, window, cx);
    }

    fn new_directory(
        &mut self,
        _: &project_panel::NewDirectory,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.add_entry(true, window, cx);
    }

    fn add_entry(&mut self, is_dir: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some((worktree_id, entry_id)) = self
            .selection
            .and_then(|selection| {
                self.project
                    .read(cx)
                    .worktree_id_for_entry(selection.0, cx)
                    .map(|worktree_id| (worktree_id, selection.0))
            })
            .or_else(|| {
                let entry_id = self.snapshot(cx)?.root_entry()?.id;
                let worktree_id = self
                    .project
                    .read(cx)
                    .worktree_for_entry(entry_id, cx)?
                    .read(cx)
                    .id();

                self.selection = Some(SelectedEntry(entry_id));

                Some((worktree_id, entry_id))
            })
        else {
            return;
        };

        let directory_id;
        let Some(worktree) = self.project.read(cx).worktree_for_id(worktree_id, cx) else {
            return;
        };
        {
            let worktree = worktree.read(cx);
            let expanded_dir_ids = self.tree_state.expanded_dir_ids.get_or_insert_with(|| {
                worktree
                    .root_entry()
                    .map(|entry| vec![entry.id])
                    .unwrap_or_default()
            });

            if let Some(mut entry) = worktree.entry_for_id(entry_id) {
                loop {
                    if entry.is_dir() {
                        if let Err(index) = expanded_dir_ids.binary_search(&entry.id) {
                            expanded_dir_ids.insert(index, entry.id);
                        }
                        directory_id = entry.id;
                        break;
                    }
                    if let Some(parent_path) = entry.path.parent()
                        && let Some(parent_entry) = worktree.entry_for_path(parent_path)
                    {
                        entry = parent_entry;
                        continue;
                    }
                    return;
                }
            } else {
                return;
            }
        }

        let previously_focused = self.selection;
        self.marked_entries.clear();
        self.tree_state.edit_state = Some(EditState {
            worktree_id,
            entry_id: directory_id,
            is_dir,
            processing_file_name: None,
            previously_focused,
        });
        self.file_name_editor.update(cx, |editor, cx| {
            editor.clear(window, cx);
        });
        self.update_visible_entries(Some(Self::NEW_ENTRY_ID), true, true, window, cx);
        cx.notify();
    }

    fn confirm(&mut self, _: &Confirm, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(task) = self.confirm_edit(true, window, cx) {
            task.detach_and_log_err(cx);
        }
    }

    fn cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        self.marked_entries.clear();
        cx.notify();
        self.discard_edit_state(window, cx);
        window.focus(&self.focus_handle, cx);
    }

    fn confirm_edit(
        &mut self,
        refocus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<anyhow::Result<()>>> {
        let edit_state = self.tree_state.edit_state.as_mut()?;
        let worktree_id = edit_state.worktree_id;
        let mut file_name = self.file_name_editor.read(cx).text(cx);
        let path_style = self.project.read(cx).path_style(cx);
        if path_style.is_windows() {
            while let Some(trimmed) = file_name.strip_suffix('.') {
                file_name = trimmed.to_string();
            }
        }
        if file_name.trim().is_empty() {
            return None;
        }

        let file_name_indicates_dir = if path_style.is_windows() {
            file_name.ends_with('/') || file_name.ends_with('\\')
        } else {
            file_name.ends_with('/')
        };
        let file_name = if path_style.is_windows() {
            file_name.trim_start_matches(['/', '\\']).to_string()
        } else {
            file_name.trim_start_matches('/').to_string()
        };

        edit_state.is_dir = edit_state.is_dir || file_name_indicates_dir;
        let is_dir = edit_state.is_dir;
        let file_name = file_name_for_new_entry(file_name, is_dir, path_style);
        let file_name = RelPath::new(Path::new(file_name.as_str()), path_style)
            .ok()?
            .into_arc();
        let worktree = self.project.read(cx).worktree_for_id(worktree_id, cx)?;
        let entry = worktree.read(cx).entry_for_id(edit_state.entry_id)?.clone();
        let new_path = entry.path.join(&file_name);
        if worktree.read(cx).entry_for_path(&new_path).is_some() {
            return None;
        }

        let edited_entry_id = Self::NEW_ENTRY_ID;
        self.selection = Some(SelectedEntry(Self::NEW_ENTRY_ID));

        let new_project_path: ProjectPath = (worktree_id, new_path).into();
        let edit_task = self.project.update(cx, |project, cx| {
            project.create_entry(new_project_path.clone(), is_dir, cx)
        });

        if refocus {
            window.focus(&self.focus_handle, cx);
        }
        edit_state.processing_file_name = Some(file_name);
        cx.notify();

        Some(cx.spawn_in(window, async move |project_panel, cx| {
            let new_entry = edit_task.await;
            project_panel.update(cx, |project_panel, cx| {
                project_panel.tree_state.edit_state = None;
                cx.notify();
            })?;

            match new_entry {
                Err(error) => {
                    project_panel
                        .update_in(cx, |project_panel, window, cx| {
                            project_panel.marked_entries.clear();
                            project_panel.update_visible_entries(None, false, false, window, cx);
                        })
                        .ok();
                    Err(error)?;
                }
                Ok(new_entry) => {
                    project_panel.update_in(cx, |project_panel, window, cx| {
                        if let Some(selection) = &mut project_panel.selection
                            && selection.0 == edited_entry_id
                        {
                            selection.0 = new_entry.id;
                            project_panel.marked_entries.clear();
                            project_panel.expand_to_selection(cx);
                        }
                        project_panel.update_visible_entries(None, false, false, window, cx);
                        if !is_dir {
                            Self::open_entry(new_entry.id, true, false, cx);
                        }
                        cx.notify();
                    })?;
                }
            }

            Ok(())
        }))
    }

    fn discard_edit_state(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(edit_state) = self.tree_state.edit_state.take() {
            let previously_focused = edit_state.previously_focused.map(|entry| entry.0);
            self.update_visible_entries(
                previously_focused,
                false,
                previously_focused.is_some(),
                window,
                cx,
            );
        }
    }

    fn expand_to_selection(&mut self, cx: &mut Context<Self>) -> Option<()> {
        let selection = self.selection?;
        let snapshot = self.snapshot(cx)?;
        let entry = snapshot.entry_for_id(selection.0)?;
        let expanded_dir_ids = self.tree_state.expanded_dir_ids.as_mut()?;

        for ancestor in entry.path.ancestors() {
            let Some(ancestor_entry) = snapshot.entry_for_path(ancestor) else {
                continue;
            };
            if ancestor_entry.is_dir()
                && let Err(index) = expanded_dir_ids.binary_search(&ancestor_entry.id)
            {
                expanded_dir_ids.insert(index, ancestor_entry.id);
            }
        }

        Some(())
    }

    fn create_new_entry(parent_entry: &Entry, new_entry_kind: EntryKind) -> Entry {
        Entry {
            id: Self::NEW_ENTRY_ID,
            kind: new_entry_kind,
            path: parent_entry.path.join(RelPath::unix("\0").unwrap()),
            inode: 0,
            mtime: parent_entry.mtime,
            canonical_path: parent_entry.canonical_path.clone(),
            is_external: false,
            is_fifo: parent_entry.is_fifo,
            size: parent_entry.size,
            request: None,
        }
    }

    fn for_each_visible_entry(
        &self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<ProjectPanel>,
        callback: &mut dyn FnMut(
            ProjectEntryId,
            EntryDetails,
            &mut Window,
            &mut Context<ProjectPanel>,
        ),
    ) {
        let Some(snapshot) = self.snapshot(cx) else {
            return;
        };

        let end_index = range.end.min(self.tree_state.visible_entries.len());
        let entry_range = range.start.min(end_index)..end_index;
        for entry in &self.tree_state.visible_entries[entry_range] {
            let mut details = self.details_for_entry(&snapshot, entry);

            if let Some(edit_state) = &self.tree_state.edit_state
                && entry.id == Self::NEW_ENTRY_ID
            {
                if let Some(processing_file_name) = &edit_state.processing_file_name {
                    details.is_processing = true;
                    details.file_name.clear();
                    details
                        .file_name
                        .push_str(processing_file_name.as_unix_str());
                } else {
                    details.file_name.clear();
                    details.is_editing = true;
                }
            }

            callback(entry.id, details, window, cx);
        }
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
        } else {
            Self::open_entry(entry_id, false, true, cx);
            cx.notify();
        }
    }

    fn open_entry(
        entry_id: ProjectEntryId,
        focus_opened_item: bool,
        allow_preview: bool,
        cx: &mut Context<Self>,
    ) {
        cx.emit(Event::OpenedEntry {
            entry_id,
            focus_opened_item,
            allow_preview,
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

        self.update_visible_entries(None, false, false, window, cx);
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

        self.update_visible_entries(None, false, false, window, cx);
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
                            Icon::new(IconName::WarningCircle)
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
                                .size(LabelSize::Small)
                                .weight(FontWeight::MEDIUM)
                                .color(Color::Muted)
                                .alpha(0.7)
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
        details: &EntryDetails,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let is_dir = details.kind.is_dir();
        let selection = SelectedEntry(entry_id);
        let theme_colors = cx.theme().colors();
        let is_active = details.is_selected && self.focus_handle.contains_focused(window, cx);
        let bg_color = if details.is_marked {
            theme_colors.element_selected
        } else if is_active {
            theme_colors.element_selection_background
        } else {
            theme_colors.panel_background
        };
        let bg_hover_color = if details.is_marked {
            theme_colors.element_selected
        } else if is_active {
            theme_colors.element_selection_background
        } else {
            theme_colors.element_hover
        };
        let show_editor = details.is_editing && !details.is_processing;

        gpui::div()
            .id(entry_id.to_usize())
            .relative()
            .group("project-entry")
            .cursor_pointer()
            .rounded_none()
            .bg(bg_color)
            .border_1()
            .border_color(gpui::transparent_black())
            .hover(move |style| style.bg(bg_hover_color))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |project_panel, _, _, cx| {
                    project_panel.mouse_down = true;
                    cx.propagate();
                }),
            )
            .on_click(
                cx.listener(move |project_panel, event: &ClickEvent, window, cx| {
                    if event.is_right_click() || show_editor {
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
                        let click_count = event.click_count();
                        let focus_opened_item = click_count > 1;
                        let allow_preview = click_count == 1;
                        Self::open_entry(entry_id, focus_opened_item, allow_preview, cx);
                    }
                }),
            )
            .child(
                ListItem::new(entry_id.to_usize())
                    .indent_level(details.depth)
                    .indent_step_size(Self::INDENT_SIZE)
                    .spacing(ListItemSpacing::Dense)
                    .selectable(false)
                    .child(Self::render_entry_prefix(details))
                    .child(if show_editor {
                        ui::h_flex()
                            .h_6()
                            .w_full()
                            .child(self.file_name_editor.clone())
                    } else {
                        ui::h_flex()
                            .h_6()
                            .child(Label::new(details.file_name.clone()).single_line())
                    })
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

fn file_name_for_entry(snapshot: &Snapshot, entry: &Entry) -> String {
    match entry.kind {
        EntryKind::File => request_name(entry).map_or_else(
            || file_stem_for_entry(entry).to_string(),
            ToString::to_string,
        ),
        EntryKind::Dir | EntryKind::PendingDir | EntryKind::UnloadedDir => {
            entry.path.file_name().map_or_else(
                || snapshot.root_name().as_unix_str().to_string(),
                ToString::to_string,
            )
        }
    }
}

fn request_name(entry: &Entry) -> Option<&str> {
    let Some(RequestFileState::Parsed(request)) = entry.request.as_ref() else {
        return None;
    };

    request.meta.name.as_deref().and_then(|name| {
        let name = name.trim();
        if name.is_empty() { None } else { Some(name) }
    })
}

fn file_stem_for_entry(entry: &Entry) -> &str {
    let file_name = entry.path.file_name().unwrap_or_default();
    file_name.strip_suffix(".toml").unwrap_or(file_name)
}

fn file_name_for_new_entry(mut file_name: String, is_dir: bool, path_style: PathStyle) -> String {
    if is_dir {
        return file_name;
    }

    let last_component = if path_style.is_windows() {
        file_name
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(file_name.as_str())
    } else {
        file_name.rsplit('/').next().unwrap_or(file_name.as_str())
    };
    if Path::new(last_component)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"))
    {
        return file_name;
    }

    file_name.push_str(".toml");
    file_name
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
            .on_action(cx.listener(Self::new_file))
            .on_action(cx.listener(Self::new_directory))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::cancel))
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
                        let mut items = Vec::with_capacity(range.end.saturating_sub(range.start));
                        this.for_each_visible_entry(
                            range,
                            window,
                            cx,
                            &mut |entry_id, details, window, cx| {
                                items.push(this.render_entry(entry_id, &details, window, cx));
                            },
                        );
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
                                this.update_visible_entries(None, false, false, window, cx);
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

    use gpui::{Entity, TestAppContext, VisualTestContext};
    use indoc::indoc;
    use serde_json::json;
    use std::{collections::HashSet, ops::Range, sync::Arc};

    use actions::workspace::project_panel;
    use fs::Fs;
    use project::{Project, ProjectPath, RequestFileState};
    use request_editor::RequestEditor;
    use settings::SettingsStore;
    use theme::LoadThemes;
    use util::rel_path::rel_path;
    use util_macros::path;
    use workspace::{Root, SharedState, Workspace};

    fn init_test(shared_state: Arc<SharedState>, cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
            theme::init(LoadThemes::JustBase, cx);
            workspace::init(shared_state, cx);
            crate::init(cx);
            editor::init(cx);
            request_editor::init(cx);
        });
    }

    fn toggle_expand_dir(panel: &Entity<ProjectPanel>, path: &str, cx: &mut VisualTestContext) {
        let path = rel_path(path);
        panel.update_in(cx, |panel, window, cx| {
            if let Some(worktree) = panel.project.read(cx).worktree(cx) {
                let worktree = worktree.read(cx);
                if let Ok(relative_path) = path.strip_prefix(worktree.root_name())
                    && let Some(entry) = worktree.entry_for_path(relative_path)
                {
                    panel.toggle_expanded(entry.id, window, cx);
                    return;
                }
            }

            panic!("No worktree for path {path:?}");
        });
        cx.run_until_parked();
    }

    fn select_path(panel: &Entity<ProjectPanel>, path: &str, cx: &mut VisualTestContext) {
        let path = rel_path(path);
        panel.update_in(cx, |panel, window, cx| {
            if let Some(worktree) = panel.project.read(cx).worktree(cx) {
                let worktree = worktree.read(cx);
                if let Ok(relative_path) = path.strip_prefix(worktree.root_name())
                    && let Some(entry) = worktree.entry_for_path(relative_path)
                {
                    panel.update_visible_entries(Some(entry.id), false, false, window, cx);
                    return;
                }
            }

            panic!("No worktree for path {path:?}");
        });
        cx.run_until_parked();
    }

    fn visible_entries_as_strings(
        panel: &Entity<ProjectPanel>,
        range: Range<usize>,
        cx: &mut VisualTestContext,
    ) -> Vec<String> {
        panel.update_in(cx, |panel, window, cx| {
            let mut items = Vec::new();
            let mut project_entries = HashSet::new();
            let mut has_editor = false;

            panel.for_each_visible_entry(range, window, cx, &mut |entry_id, details, _, _| {
                if details.is_editing {
                    assert!(!has_editor, "duplicate editor entry");
                    has_editor = true;
                } else {
                    assert!(
                        project_entries.insert(entry_id),
                        "duplicate project entry {entry_id:?}"
                    );
                }

                let indent = "    ".repeat(usize::from(details.depth));
                let icon = if details.kind.is_dir() {
                    if details.is_expanded { "v " } else { "> " }
                } else {
                    "  "
                };

                #[cfg(target_os = "windows")]
                let file_name = details.file_name.replace('\\', "/");

                #[cfg(any(target_os = "macos", target_os = "linux"))]
                let file_name = details.file_name;

                let name = if details.is_editing {
                    format!("[EDITOR: '{file_name}']")
                } else if details.is_processing {
                    format!("[PROCESSING: '{file_name}']")
                } else {
                    file_name
                };
                let selected = if details.is_selected {
                    "  <== selected"
                } else {
                    ""
                };
                let marked = if details.is_marked {
                    "  <== marked"
                } else {
                    ""
                };

                items.push(format!("{indent}{icon}{name}{selected}{marked}"));
            });
            items
        })
    }

    fn ensure_single_file_is_opened(
        workspace: &Entity<Workspace>,
        expected_path: &str,
        cx: &mut VisualTestContext,
    ) {
        workspace.update_in(cx, |workspace, _, cx| {
            let worktree = workspace
                .project()
                .read(cx)
                .worktree(cx)
                .expect("workspace should have a worktree");
            let worktree_id = worktree.read(cx).id();

            let open_project_paths = workspace
                .pane()
                .read(cx)
                .active_item()
                .and_then(|item| item.project_path(cx))
                .into_iter()
                .collect::<Vec<_>>();
            assert_eq!(
                open_project_paths,
                vec![ProjectPath {
                    worktree_id,
                    path: Arc::from(rel_path(expected_path)),
                }],
                "Should have opened file, selected in project panel"
            );
        });
    }

    #[gpui::test]
    async fn test_sort_mode_directories_first(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state, cx);

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
        let (root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(cx.new(|cx| Workspace::test_new(project, window, cx)))
        });
        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());
        let panel = workspace.update_in(cx, ProjectPanel::new);
        cx.run_until_parked();

        let actual = visible_entries_as_strings(&panel, 0..50, cx);

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

    #[gpui::test]
    async fn test_new_file(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state, cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                "collection": {},
                "existing.toml": "",
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let (root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(cx.new(|cx| Workspace::test_new(project, window, cx)))
        });
        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());
        let panel = workspace.update_in(cx, ProjectPanel::new);
        cx.run_until_parked();

        select_path(&panel, "project", cx);
        panel.update_in(cx, |panel, window, cx| {
            panel.new_file(&project_panel::NewFile, window, cx);
        });
        cx.run_until_parked();

        panel.update_in(cx, |panel, window, cx| {
            assert!(panel.file_name_editor.read(cx).is_focused(window));
        });
        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            vec![
                String::from("v project"),
                String::from("    > collection"),
                String::from("      [EDITOR: '']  <== selected"),
                String::from("      existing"),
            ]
        );

        panel
            .update_in(cx, |panel, window, cx| {
                panel.file_name_editor.update(cx, |editor, cx| {
                    editor.set_text("New request", cx);
                });
                panel.confirm_edit(true, window, cx).unwrap()
            })
            .await
            .unwrap();
        cx.run_until_parked();

        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            vec![
                String::from("v project"),
                String::from("    > collection"),
                String::from("      existing"),
                String::from("      New request  <== selected  <== marked"),
            ]
        );

        let request_state = panel.update(cx, |panel, cx| {
            let worktree = panel
                .project
                .read(cx)
                .worktree(cx)
                .expect("project should have a worktree");
            worktree
                .read(cx)
                .entry_for_path(rel_path("New request.toml"))
                .expect("new request should exist")
                .request
                .clone()
        });
        assert!(matches!(request_state, Some(RequestFileState::Parsed(_))));
    }

    #[gpui::test]
    async fn test_new_directory(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state, cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                "existing.toml": "",
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let (root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(cx.new(|cx| Workspace::test_new(project, window, cx)))
        });
        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());
        let panel = workspace.update_in(cx, ProjectPanel::new);
        cx.run_until_parked();

        select_path(&panel, "project", cx);
        panel.update_in(cx, |panel, window, cx| {
            panel.new_directory(&project_panel::NewDirectory, window, cx);
        });
        cx.run_until_parked();

        panel.update_in(cx, |panel, window, cx| {
            assert!(panel.file_name_editor.read(cx).is_focused(window));
        });
        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            vec![
                String::from("v project"),
                String::from("    > [EDITOR: '']  <== selected"),
                String::from("      existing"),
            ]
        );

        panel
            .update_in(cx, |panel, window, cx| {
                panel.file_name_editor.update(cx, |editor, cx| {
                    editor.set_text("New collection", cx);
                });
                panel.confirm_edit(true, window, cx).unwrap()
            })
            .await
            .unwrap();
        cx.run_until_parked();

        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            vec![
                String::from("v project"),
                String::from("    v New collection  <== selected"),
                String::from("      existing"),
            ]
        );

        let metadata = temp_fs
            .metadata("project/New collection".as_ref())
            .await
            .unwrap()
            .expect("new collection should exist");
        assert!(metadata.is_dir);
    }

    #[gpui::test]
    async fn test_file_open_in_request_editor(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state, cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                "collection": {
                    "first.toml": indoc! {r#"
                        [meta]
                        version = 1
                        name = "First"

                        [http]
                        method = "GET"
                        url = "https://api.zaku.dev/first"
                    "#},
                    "second.toml": indoc! {r#"
                        [meta]
                        version = 1
                        name = "Second"

                        [http]
                        method = "POST"
                        url = "https://api.zaku.dev/second"
                    "#},
                    "third.toml": indoc! {r#"
                        [meta]
                        version = 1
                        name = "Third"

                        [http]
                        method = "PUT"
                        url = "https://api.zaku.dev/third"
                    "#},
                }
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let (root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(cx.new(|cx| Workspace::test_new(project, window, cx)))
        });
        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());
        let panel = workspace.update_in(cx, ProjectPanel::new);

        cx.run_until_parked();

        toggle_expand_dir(&panel, "project/collection", cx);
        select_path(&panel, "project/collection/first.toml", cx);
        panel.update_in(cx, |panel, window, cx| {
            panel.open(&project_panel::Open, window, cx);
        });
        cx.run_until_parked();

        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            vec![
                String::from("v project"),
                String::from("    v collection"),
                String::from("          First  <== selected  <== marked"),
                String::from("          Second"),
                String::from("          Third"),
            ]
        );

        ensure_single_file_is_opened(&workspace, "collection/first.toml", cx);
        workspace.update_in(cx, |workspace, _, cx| {
            assert!(workspace.active_item_as::<RequestEditor>(cx).is_some());
        });

        select_path(&panel, "project/collection/second.toml", cx);
        panel.update_in(cx, |panel, window, cx| {
            panel.open(&project_panel::Open, window, cx);
        });
        cx.run_until_parked();

        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            vec![
                String::from("v project"),
                String::from("    v collection"),
                String::from("          First"),
                String::from("          Second  <== selected  <== marked"),
                String::from("          Third"),
            ]
        );

        ensure_single_file_is_opened(&workspace, "collection/second.toml", cx);
        workspace.update_in(cx, |workspace, _, cx| {
            assert!(workspace.active_item_as::<RequestEditor>(cx).is_some());
        });
    }
}
