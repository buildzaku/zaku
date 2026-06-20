use anyhow::Context as AnyhowContext;
use gpui::{
    Action, Anchor, AnyElement, App, Bounds, ClickEvent, ClipboardItem, Context, DismissEvent, Div,
    Entity, EventEmitter, FocusHandle, Focusable, FontWeight, KeyContext,
    ListHorizontalSizingBehavior, ListSizingBehavior, MouseButton, MouseDownEvent, Pixels, Point,
    PromptLevel, Render, ScrollStrategy, Stateful, Subscription, Task, UniformListScrollHandle,
    WeakEntity, Window, prelude::*,
};
use smallvec::SmallVec;
use std::{
    cmp,
    collections::BTreeSet,
    ops::Range,
    path::{Path, PathBuf},
    sync::Arc,
};

use editor::{Editor, EditorEvent, MultiBufferOffset, SelectionEffects};
use path::{PathStyle, RelPath, SortMode, SortOrder};
use project::{
    Entry, EntryKind, Project, ProjectEntryId, ProjectEvent, ProjectPath, Snapshot, Worktree,
    WorktreeId,
};
use theme::ActiveTheme;
use ui::{
    Color, ContextMenu, DynamicSpacing, Icon, IconName, IconSize, IndentGuideColors,
    IndentGuideLayout, Label, LabelCommon, LabelSize, ListItem, ListItemSpacing,
    RenderedIndentGuide, ScrollAxes, Scrollbars, TrackLayout, WithScrollbar,
};
use util::ResultExt;

use workspace::{Panel, Workspace, pane::Pane};

pub fn init(cx: &mut App) {
    cx.observe_new(
        |workspace: &mut Workspace, _window, _: &mut Context<Workspace>| {
            workspace.register_action(
                |workspace, _: &actions::project_panel::ToggleFocus, window, cx| {
                    workspace.toggle_panel_focus::<ProjectPanel>(window, cx);
                },
            );
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
pub enum ProjectPanelEvent {
    OpenedEntry {
        entry_id: ProjectEntryId,
        focus_opened_item: bool,
        allow_preview: bool,
    },
}

impl EventEmitter<ProjectPanelEvent> for ProjectPanel {}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct SelectedEntry(ProjectEntryId);

#[derive(Clone, Debug)]
enum ValidationState {
    None,
    Warning(String),
    Error(String),
}

#[derive(Clone, Debug)]
struct EditState {
    worktree_id: WorktreeId,
    entry_id: ProjectEntryId,
    leaf_entry_id: Option<ProjectEntryId>,
    is_dir: bool,
    processing_file_name: Option<Arc<RelPath>>,
    previously_focused: Option<SelectedEntry>,
    validation_state: ValidationState,
}

impl EditState {
    fn is_new_entry(&self) -> bool {
        self.leaf_entry_id.is_none()
    }
}

#[derive(Clone, Debug)]
enum ClipboardEntry {
    Copied(BTreeSet<SelectedEntry>),
    Cut(BTreeSet<SelectedEntry>),
}

enum PasteTask {
    Rename {
        task: Task<anyhow::Result<Entry>>,
    },
    Copy {
        task: Task<anyhow::Result<Option<Entry>>>,
    },
}

impl ClipboardEntry {
    fn is_cut(&self) -> bool {
        matches!(self, Self::Cut(_))
    }

    fn items(&self) -> &BTreeSet<SelectedEntry> {
        match self {
            Self::Copied(entries) | Self::Cut(entries) => entries,
        }
    }

    fn into_copy_entry(self) -> Self {
        match self {
            Self::Copied(_) => self,
            Self::Cut(entries) => Self::Copied(entries),
        }
    }
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
    context_menu: Option<(Entity<ContextMenu>, Point<Pixels>, Subscription)>,
    file_name_editor: Entity<Editor>,
    clipboard: Option<ClipboardEntry>,
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
                        project_panel.populate_validation_error(cx);
                        project_panel.autoscroll(cx);
                    }
                    EditorEvent::Blurred => {
                        let Some(is_new_entry) = project_panel
                            .tree_state
                            .edit_state
                            .as_ref()
                            .and_then(|edit_state| {
                                edit_state
                                    .processing_file_name
                                    .is_none()
                                    .then_some(edit_state.is_new_entry())
                            })
                        else {
                            return;
                        };

                        if is_new_entry {
                            match project_panel.confirm_edit(false, window, cx) {
                                Some(task) => task.detach_and_log_err(cx),
                                None => project_panel.discard_edit_state(window, cx),
                            }
                        } else {
                            project_panel.discard_edit_state(window, cx);
                        }
                    }
                    EditorEvent::DirtyChanged
                    | EditorEvent::FileHandleChanged
                    | EditorEvent::Saved
                    | EditorEvent::TitleChanged => {}
                },
            )
            .detach();

            let project_subscription = cx.subscribe_in(
                &project,
                window,
                |this: &mut ProjectPanel, project, event: &ProjectEvent, window, cx| match event {
                    ProjectEvent::ActiveEntryChanged(Some(entry_id)) => {
                        this.reveal_entry(project, *entry_id, window, cx).log_err();
                    }
                    ProjectEvent::ActiveEntryChanged(None) => {
                        this.marked_entries.clear();
                    }
                    ProjectEvent::WorktreeAdded(worktree_id)
                    | ProjectEvent::WorktreeUpdatedEntries(worktree_id, _) => {
                        if project
                            .read(cx)
                            .worktree_for_id(*worktree_id, cx)
                            .is_some_and(|worktree| worktree.read(cx).is_visible())
                        {
                            this.update_visible_entries(None, false, false, window, cx);
                        }
                    }
                    ProjectEvent::WorktreeRemoved(_) => {
                        this.update_visible_entries(None, false, false, window, cx);
                    }
                    ProjectEvent::DeletedEntry(_, _) => {}
                    ProjectEvent::EntryMetadataUpdated(_) => {
                        cx.notify();
                    }
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
                context_menu: None,
                file_name_editor,
                clipboard: None,
                mouse_down: false,
                _project_subscription: project_subscription,
            };
            this.update_visible_entries(None, false, false, window, cx);
            this
        });

        cx.subscribe_in(&project_panel, window, {
            let project_panel = project_panel.downgrade();
            move |workspace, _, event, window, cx| match event {
                &ProjectPanelEvent::OpenedEntry {
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

    fn dispatch_context(&self, window: &Window, cx: &Context<Self>) -> KeyContext {
        let mut dispatch_context = KeyContext::new_with_defaults();
        dispatch_context.add(Self::PANEL_KEY);
        dispatch_context.add("menu");

        let identifier = if self.file_name_editor.focus_handle(cx).is_focused(window) {
            "editing"
        } else {
            "not_editing"
        };

        dispatch_context.add(identifier);
        dispatch_context
    }

    fn reveal_entry(
        &mut self,
        project: &Entity<Project>,
        entry_id: ProjectEntryId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        project
            .read(cx)
            .worktree_for_entry(entry_id, cx)
            .context("can't reveal a non-existent entry in the project panel")?;

        self.expand_entry(entry_id, cx);
        self.update_visible_entries(Some(entry_id), false, true, window, cx);
        self.marked_entries.clear();
        self.marked_entries.push(SelectedEntry(entry_id));
        cx.notify();
        Ok(())
    }

    fn details_for_entry(&self, snapshot: &Snapshot, entry: &Entry, cx: &App) -> EntryDetails {
        let expanded_dir_ids = self.tree_state.expanded_dir_ids.as_deref().unwrap_or(&[]);
        let is_expanded = entry.kind.is_dir() && expanded_dir_ids.binary_search(&entry.id).is_ok();
        let file_name = file_name_for_entry(snapshot, entry);
        let depth = u16::try_from(display_depth(entry)).unwrap_or(u16::MAX);
        let mut prefix_label = None;
        let mut is_invalid = false;
        if let Some(metadata) = self.project.read(cx).entry_metadata(entry) {
            prefix_label.clone_from(&metadata.prefix_label);
            is_invalid = metadata.is_invalid;
        }
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

    fn load_entry_metadata_for_range(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        let end_index = range.end.min(self.tree_state.visible_entries.len());
        let entry_range = range.start.min(end_index)..end_index;
        let entries = self.tree_state.visible_entries[entry_range].to_vec();

        for entry in entries {
            self.project
                .update(cx, |project, cx| project.load_entry_metadata(&entry, cx));
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
                    if let Some(edit_state) = &edit_state
                        && edit_state.is_new_entry()
                    {
                        new_entry_parent_id = Some(edit_state.entry_id);
                        new_entry_kind = if edit_state.is_dir {
                            EntryKind::Dir
                        } else {
                            EntryKind::File
                        };
                    }

                    let root_entry_id = snapshot.root_entry().map(|entry| entry.id);
                    while let Some(entry) = traversal.entry() {
                        if root_entry_id != Some(entry.id)
                            && (entry.kind.is_dir() || entry.is_request)
                        {
                            entries.push(entry.clone());
                        }
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
                        let prefix_chars = usize::from(entry.is_request) * 5;
                        let width_estimate = item_width_estimate(
                            display_depth(entry),
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

    fn select_previous(
        &mut self,
        _: &actions::menu::SelectPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
            self.select_first(&actions::menu::SelectFirst, window, cx);
        }
    }

    fn select_next(
        &mut self,
        _: &actions::menu::SelectNext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
            self.select_first(&actions::menu::SelectFirst, window, cx);
        }
    }

    fn select_first(
        &mut self,
        _: &actions::menu::SelectFirst,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    fn select_last(
        &mut self,
        _: &actions::menu::SelectLast,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(entry) = self.tree_state.visible_entries.last() {
            let selection = SelectedEntry(entry.id);
            self.selection = Some(selection);
            window.focus(&self.focus_handle, cx);
            self.autoscroll(cx);
        }
    }

    fn expand_selected_entry(
        &mut self,
        _: &actions::project_panel::ExpandSelectedEntry,
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
            Ok(_) => self.select_next(&actions::menu::SelectNext, window, cx),
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
        _: &actions::project_panel::CollapseSelectedEntry,
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
                if snapshot.root_entry().map(|entry| entry.id) != Some(entry.id)
                    && let Ok(index) = expanded_dir_ids.binary_search(&entry.id)
                {
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
        _: &actions::project_panel::CollapseSelectedEntryAndChildren,
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
        _: &actions::project_panel::CollapseAllEntries,
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

    fn deploy_context_menu(
        &mut self,
        position: Point<Pixels>,
        entry_id: ProjectEntryId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .project
            .read(cx)
            .worktree_id_for_entry(entry_id, cx)
            .is_none()
        {
            return;
        }

        self.selection = Some(SelectedEntry(entry_id));
        let has_pasteable_content = self.has_pasteable_content();

        let context_menu = ContextMenu::build(window, cx, |menu, _, _| {
            menu.context(self.focus_handle.clone())
                .action("New Request", Box::new(actions::project_panel::NewFile))
                .action(
                    "New Collection",
                    Box::new(actions::project_panel::NewDirectory),
                )
                .separator()
                .action(
                    ui::utils::reveal_in_file_manager_label(),
                    Box::new(actions::project_panel::RevealInFileManager),
                )
                .separator()
                .action("Cut", Box::new(actions::project_panel::Cut))
                .action("Copy", Box::new(actions::project_panel::Copy))
                .action("Duplicate", Box::new(actions::project_panel::Duplicate))
                .action_disabled_when(
                    !has_pasteable_content,
                    "Paste",
                    Box::new(actions::project_panel::Paste),
                )
                .separator()
                .action("Copy Path", Box::new(actions::workspace::CopyPath))
                .action(
                    "Copy Relative Path",
                    Box::new(actions::workspace::CopyRelativePath),
                )
                .separator()
                .action("Rename", Box::new(actions::project_panel::Rename))
                .action(
                    "Trash",
                    Box::new(actions::project_panel::Trash { skip_prompt: false }),
                )
                .action(
                    "Delete",
                    Box::new(actions::project_panel::Delete { skip_prompt: false }),
                )
        });

        window.focus(&context_menu.focus_handle(cx), cx);
        let subscription = cx.subscribe(&context_menu, |this, _, _: &DismissEvent, cx| {
            this.context_menu.take();
            cx.notify();
        });
        self.context_menu = Some((context_menu, position, subscription));
        cx.notify();
    }

    fn new_file(
        &mut self,
        _: &actions::project_panel::NewFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.add_entry(false, window, cx);
    }

    fn new_directory(
        &mut self,
        _: &actions::project_panel::NewDirectory,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.add_entry(true, window, cx);
    }

    fn cut(&mut self, _: &actions::project_panel::Cut, _: &mut Window, cx: &mut Context<Self>) {
        let entries = self.disjoint_effective_entries(cx);
        if !entries.is_empty() {
            self.write_entries_to_system_clipboard(&entries, cx);
            self.clipboard = Some(ClipboardEntry::Cut(entries));
            cx.notify();
        }
    }

    fn copy(&mut self, _: &actions::project_panel::Copy, _: &mut Window, cx: &mut Context<Self>) {
        let entries = self.disjoint_effective_entries(cx);
        if !entries.is_empty() {
            self.write_entries_to_system_clipboard(&entries, cx);
            self.clipboard = Some(ClipboardEntry::Copied(entries));
            cx.notify();
        }
    }

    fn create_paste_path(
        &self,
        source: SelectedEntry,
        (worktree, target_entry): (Entity<Worktree>, &Entry),
        cx: &App,
    ) -> Option<(Arc<RelPath>, Option<Range<usize>>)> {
        let mut new_path = target_entry.path.to_rel_path_buf();
        if target_entry.is_file() || (target_entry.is_dir() && target_entry.id == source.0) {
            new_path.pop();
        }

        let source_worktree = self.project.read(cx).worktree_for_entry(source.0, cx)?;
        let source_worktree = source_worktree.read(cx);
        let source_entry = source_worktree.entry_for_id(source.0)?;
        let clipboard_entry_file_name = source_entry.path.file_name()?.to_string();
        new_path.push(RelPath::unix(&clipboard_entry_file_name).ok()?);

        let (extension, file_name_without_extension) = if source_entry.is_file() {
            (
                new_path.extension().map(ToString::to_string),
                new_path.file_stem()?.to_string(),
            )
        } else {
            (None, clipboard_entry_file_name.clone())
        };

        let file_name_len = file_name_without_extension.len();
        let mut disambiguation_range = None;
        let mut index = 0;
        {
            let worktree = worktree.read(cx);
            while worktree.entry_for_path(new_path.as_rel_path()).is_some() {
                new_path.pop();

                let mut new_file_name = file_name_without_extension.clone();
                let disambiguation = " copy";
                let mut disambiguation_len = disambiguation.len();
                new_file_name.push_str(disambiguation);

                if index > 0 {
                    let extra_disambiguation = format!(" {index}");
                    disambiguation_len += extra_disambiguation.len();
                    new_file_name.push_str(&extra_disambiguation);
                }
                if let Some(extension) = extension.as_ref() {
                    new_file_name.push('.');
                    new_file_name.push_str(extension);
                }

                new_path.push(RelPath::unix(&new_file_name).ok()?);
                disambiguation_range = Some(0..(file_name_len + disambiguation_len));
                index += 1;
            }
        }

        Some((new_path.as_rel_path().into(), disambiguation_range))
    }

    fn paste(
        &mut self,
        _: &actions::project_panel::Paste,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(task) = self.paste_impl(window, cx) {
            task.detach_and_log_err(cx);
        }
    }

    fn paste_impl(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<anyhow::Result<()>>> {
        let (worktree, entry) = self.selected_entry_handle(cx)?;
        let entry = entry.clone();
        let worktree_id = worktree.read(cx).id();
        let clipboard_entries = self
            .clipboard
            .as_ref()
            .filter(|clipboard| !clipboard.items().is_empty())?;

        let clip_is_cut = clipboard_entries.is_cut();
        let mut paste_tasks = Vec::new();
        let mut disambiguation_range = None;
        for clipboard_entry in clipboard_entries.items() {
            let (new_path, new_disambiguation_range) =
                self.create_paste_path(*clipboard_entry, (worktree.clone(), &entry), cx)?;
            let destination: ProjectPath = (worktree_id, new_path).into();
            let task = if clip_is_cut {
                let task = self.project.update(cx, |project, cx| {
                    project.rename_entry(clipboard_entry.0, destination, cx)
                });
                PasteTask::Rename { task }
            } else {
                let task = self.project.update(cx, |project, cx| {
                    project.copy_entry(clipboard_entry.0, destination, cx)
                });
                PasteTask::Copy { task }
            };
            paste_tasks.push(task);
            disambiguation_range = new_disambiguation_range.or(disambiguation_range);
        }

        let item_count = paste_tasks.len();
        let task = cx.spawn_in(window, async move |project_panel, cx| {
            let mut last_succeed = None;
            for task in paste_tasks {
                match task {
                    PasteTask::Rename { task } => {
                        if let Some(entry) = task.await.log_err() {
                            last_succeed = Some(entry);
                        }
                    }
                    PasteTask::Copy { task } => {
                        if let Some(Some(entry)) = task.await.log_err() {
                            last_succeed = Some(entry);
                        }
                    }
                }
            }

            if let Some(entry) = last_succeed {
                project_panel.update_in(cx, |project_panel, window, cx| {
                    project_panel.selection = Some(SelectedEntry(entry.id));

                    if item_count == 1 {
                        if !entry.is_dir() {
                            Self::open_entry(entry.id, disambiguation_range.is_none(), false, cx);
                        }

                        if disambiguation_range.is_some() {
                            cx.defer_in(window, |project_panel, window, cx| {
                                project_panel.rename_impl(disambiguation_range, window, cx);
                            });
                        }
                    }
                })?;
            }

            anyhow::Ok(())
        });

        if clip_is_cut {
            self.clipboard = self.clipboard.take().map(ClipboardEntry::into_copy_entry);
        }

        self.expand_entry(entry.id, cx);
        Some(task)
    }

    fn duplicate(
        &mut self,
        _: &actions::project_panel::Duplicate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(task) = self.duplicate_impl(window, cx) {
            task.detach_and_log_err(cx);
        }
    }

    fn duplicate_impl(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<anyhow::Result<()>>> {
        self.copy(&actions::project_panel::Copy, window, cx);
        self.paste_impl(window, cx)
    }

    fn rename_impl(
        &mut self,
        selection: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(selection_entry) = self.selection
            && let Some(worktree) = self.project.read(cx).root_worktree(cx)
            && let Some(entry) = worktree.read(cx).entry_for_id(selection_entry.0).cloned()
        {
            #[cfg(target_os = "windows")]
            if worktree
                .read(cx)
                .root_entry()
                .is_some_and(|root_entry| root_entry.id == entry.id)
            {
                return;
            }

            let worktree_id = worktree.read(cx).id();
            self.tree_state.edit_state = Some(EditState {
                worktree_id,
                entry_id: entry.id,
                leaf_entry_id: Some(entry.id),
                is_dir: entry.is_dir(),
                processing_file_name: None,
                previously_focused: None,
                validation_state: ValidationState::None,
            });
            let file_name = if entry.is_file() {
                file_stem_for_entry(&entry).to_string()
            } else {
                entry.path.file_name().unwrap_or_default().to_string()
            };
            let selection = selection.unwrap_or_else(|| {
                let selection_end = if entry.is_file() {
                    file_name.len()
                } else {
                    entry.path.file_stem().map_or(file_name.len(), str::len)
                };
                0..selection_end
            });
            self.file_name_editor.update(cx, |editor, cx| {
                editor.set_text(&file_name, cx);
                editor.change_selections(SelectionEffects::default(), cx, |selections| {
                    selections.select_ranges([
                        MultiBufferOffset(selection.start)..MultiBufferOffset(selection.end)
                    ]);
                });
            });
            self.update_visible_entries(None, true, true, window, cx);
            cx.notify();
        }
    }

    fn rename(
        &mut self,
        _: &actions::project_panel::Rename,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.rename_impl(None, window, cx);
    }

    fn remove(
        &mut self,
        trash: bool,
        skip_prompt: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(task) = self.remove_impl(trash, skip_prompt, window, cx) {
            task.detach_and_log_err(cx);
        }
    }

    fn remove_impl(
        &mut self,
        trash: bool,
        skip_prompt: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<anyhow::Result<()>>> {
        let items_to_delete = self.disjoint_effective_entries(cx);
        if items_to_delete.is_empty() {
            return None;
        }

        let (dirty_buffers, file_paths) = {
            let project = self.project.read(cx);
            let dirty_buffers = self.pane.upgrade().map_or(0, |pane| {
                pane.read(cx)
                    .items()
                    .filter(|item| {
                        item.is_dirty(cx)
                            && item
                                .project_entry_ids(cx)
                                .iter()
                                .any(|entry_id| items_to_delete.contains(&SelectedEntry(*entry_id)))
                    })
                    .count()
            });
            let file_paths = items_to_delete
                .iter()
                .filter_map(|selection| {
                    let project_path = project.path_for_entry(selection.0, cx)?;
                    Some((selection.0, project_path.path.file_name()?.to_string()))
                })
                .collect::<Vec<_>>();

            (dirty_buffers, file_paths)
        };
        if file_paths.is_empty() {
            return None;
        }

        let answer = if skip_prompt {
            None
        } else {
            let operation = if trash { "Trash" } else { "Delete" };
            let message_start = if trash {
                "Do you want to trash"
            } else {
                "Are you sure you want to permanently delete"
            };
            let prompt = match file_paths.first() {
                Some((_, path)) if file_paths.len() == 1 => {
                    let unsaved_warning = if dirty_buffers > 0 {
                        "\n\nIt has unsaved changes, which will be lost."
                    } else {
                        ""
                    };

                    format!("{message_start} `{path}`?{unsaved_warning}")
                }
                _ => {
                    const CUTOFF_POINT: usize = 10;
                    let names = if file_paths.len() > CUTOFF_POINT {
                        let truncated_path_counts = file_paths.len() - CUTOFF_POINT;
                        let mut paths = file_paths
                            .iter()
                            .map(|(_, path)| format!("`{path}`"))
                            .take(CUTOFF_POINT)
                            .collect::<Vec<_>>();
                        paths.truncate(CUTOFF_POINT);
                        if truncated_path_counts == 1 {
                            paths.push(".. 1 file not shown".into());
                        } else {
                            paths.push(format!(".. {truncated_path_counts} files not shown"));
                        }
                        paths
                    } else {
                        file_paths
                            .iter()
                            .map(|(_, path)| format!("`{path}`"))
                            .collect()
                    };
                    let unsaved_warning = if dirty_buffers == 0 {
                        String::new()
                    } else if dirty_buffers == 1 {
                        "\n\n1 of these has unsaved changes, which will be lost.".to_string()
                    } else {
                        format!(
                            "\n\n{dirty_buffers} of these have unsaved changes, which will be lost."
                        )
                    };

                    format!(
                        "{message_start} the following {} files?\n{}{unsaved_warning}",
                        file_paths.len(),
                        names.join("\n")
                    )
                }
            };
            let detail = (!trash).then_some("This cannot be undone.");
            Some(window.prompt(
                PromptLevel::Info,
                &prompt,
                detail,
                &[operation, "Cancel"],
                cx,
            ))
        };
        let next_selection = self.find_next_selection_after_deletion(&items_to_delete, cx);
        Some(cx.spawn_in(window, async move |panel, cx| {
            if let Some(answer) = answer
                && answer.await != Ok(0)
            {
                return anyhow::Ok(());
            }

            for (entry_id, _) in file_paths {
                panel
                    .update(cx, |panel, cx| {
                        panel.project.update(cx, |project, cx| {
                            project
                                .delete_entry(entry_id, trash, cx)
                                .context("no such entry")
                        })
                    })??
                    .await?;
            }

            panel.update_in(cx, |panel, window, cx| {
                panel.marked_entries.clear();
                if let Some(next_selection) = next_selection {
                    panel.update_visible_entries(Some(next_selection.0), false, true, window, cx);
                } else {
                    panel.selection = None;
                    panel.update_visible_entries(None, false, true, window, cx);
                }
            })?;
            anyhow::Ok(())
        }))
    }

    fn trash(
        &mut self,
        action: &actions::project_panel::Trash,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remove(true, action.skip_prompt, window, cx);
    }

    fn delete(
        &mut self,
        action: &actions::project_panel::Delete,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remove(false, action.skip_prompt, window, cx);
    }

    fn copy_path(
        &mut self,
        _: &actions::workspace::CopyPath,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let abs_file_paths = {
            let project = self.project.read(cx);
            self.effective_entries()
                .into_iter()
                .filter_map(|entry| {
                    let project_path = project.path_for_entry(entry.0, cx)?;
                    Some(
                        project
                            .absolute_path(&project_path, cx)?
                            .to_string_lossy()
                            .to_string(),
                    )
                })
                .collect::<Vec<_>>()
        };
        if !abs_file_paths.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(abs_file_paths.join("\n")));
        }
    }

    fn copy_relative_path(
        &mut self,
        _: &actions::workspace::CopyRelativePath,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path_style = self.project.read(cx).path_style(cx);
        let file_paths = {
            let project = self.project.read(cx);
            self.effective_entries()
                .into_iter()
                .filter_map(|entry| {
                    Some(
                        project
                            .path_for_entry(entry.0, cx)?
                            .path
                            .display(path_style)
                            .into_owned(),
                    )
                })
                .collect::<Vec<_>>()
        };
        if !file_paths.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(file_paths.join("\n")));
        }
    }

    fn reveal_in_file_manager(
        &mut self,
        _: &actions::project_panel::RevealInFileManager,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(path) = self.reveal_in_file_manager_path(cx) {
            self.project
                .update(cx, |project, cx| project.reveal_path(&path, cx));
        }
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
            leaf_entry_id: None,
            is_dir,
            processing_file_name: None,
            previously_focused,
            validation_state: ValidationState::None,
        });
        self.file_name_editor.update(cx, |editor, cx| {
            editor.clear(window, cx);
        });
        self.update_visible_entries(Some(Self::NEW_ENTRY_ID), true, true, window, cx);
        cx.notify();
    }

    fn confirm(&mut self, _: &actions::menu::Confirm, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(task) = self.confirm_edit(true, window, cx) {
            task.detach_and_log_err(cx);
        }
    }

    fn cancel(&mut self, _: &actions::menu::Cancel, window: &mut Window, cx: &mut Context<Self>) {
        self.marked_entries.clear();
        cx.notify();
        self.discard_edit_state(window, cx);
        window.focus(&self.focus_handle, cx);
    }

    fn populate_validation_error(&mut self, cx: &mut Context<Self>) {
        let Some(edit_state) = self.tree_state.edit_state.as_mut() else {
            return;
        };
        let worktree_id = edit_state.worktree_id;
        let entry_id = edit_state.entry_id;
        let is_dir = edit_state.is_dir;
        let is_new_entry = edit_state.is_new_entry();
        let mut file_name = self.file_name_editor.read(cx).text(cx);
        let path_style = self.project.read(cx).path_style(cx);

        if file_name.is_empty() {
            edit_state.validation_state = ValidationState::None;
            cx.notify();
            return;
        }

        if path_style.is_windows() {
            while let Some(trimmed) = file_name.strip_suffix('.') {
                file_name = trimmed.to_string();
            }
        }

        if file_name.trim().is_empty() {
            edit_state.validation_state =
                ValidationState::Error("File or directory name must be provided.".to_string());
            cx.notify();
            return;
        }

        let file_name_indicates_dir = if path_style.is_windows() {
            file_name.ends_with('/') || file_name.ends_with('\\')
        } else {
            file_name.ends_with('/')
        };
        let is_dir = is_dir || (is_new_entry && file_name_indicates_dir);
        let entry_kind = if is_dir { "Directory" } else { "File" };
        let trimmed_file_name = file_name.trim();
        let has_leading_or_trailing_whitespace = trimmed_file_name != file_name.as_str();
        let file_name = if path_style.is_windows() {
            file_name.trim_start_matches(['/', '\\'])
        } else {
            file_name.trim_start_matches('/')
        };
        if file_name.is_empty() {
            edit_state.validation_state =
                ValidationState::Error("File or directory name must be provided.".to_string());
            cx.notify();
            return;
        }
        if is_missing_entry_name(file_name, is_dir, path_style) {
            edit_state.validation_state =
                ValidationState::Error("File or directory name must be provided.".to_string());
            cx.notify();
            return;
        }

        if has_leading_or_trailing_whitespace {
            edit_state.validation_state = ValidationState::Warning(format!(
                "{entry_kind} name contains leading or trailing whitespace."
            ));
            cx.notify();
            return;
        }

        let file_name = file_name_for_new_entry(file_name, is_dir, path_style);
        let Ok(file_name) = RelPath::new(Path::new(file_name.as_str()), path_style) else {
            edit_state.validation_state = ValidationState::Warning(format!(
                "{entry_kind} name contains leading or trailing whitespace."
            ));
            cx.notify();
            return;
        };
        let file_name = file_name.into_arc();

        if let Some(worktree) = self.project.read(cx).worktree_for_id(worktree_id, cx)
            && let Some(entry) = worktree.read(cx).entry_for_id(entry_id).cloned()
        {
            let new_path = if is_new_entry {
                entry.path.join(&file_name)
            } else if let Some(parent) = entry.path.parent() {
                parent.join(&file_name)
            } else {
                file_name.clone()
            };
            if let Some(existing_entry) = worktree.read(cx).entry_for_path(&new_path)
                && (is_new_entry || existing_entry.id != entry.id)
            {
                let existing_entry_kind = if existing_entry.is_dir() {
                    "Directory"
                } else {
                    "File"
                };
                edit_state.validation_state = ValidationState::Error(format!(
                    "{existing_entry_kind} '{}' already exists at location. Please choose a different name.",
                    file_name.as_unix_str()
                ));
                cx.notify();
                return;
            }
        }

        edit_state.validation_state = ValidationState::None;
        cx.notify();
    }

    fn confirm_edit(
        &mut self,
        refocus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<anyhow::Result<()>>> {
        let edit_state = self.tree_state.edit_state.as_mut()?;
        let worktree_id = edit_state.worktree_id;
        let is_new_entry = edit_state.is_new_entry();
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
            file_name.trim_start_matches(['/', '\\'])
        } else {
            file_name.trim_start_matches('/')
        };
        if file_name.is_empty() {
            return None;
        }

        let is_dir = edit_state.is_dir || (is_new_entry && file_name_indicates_dir);
        if is_missing_entry_name(file_name, is_dir, path_style) {
            return None;
        }

        edit_state.is_dir = is_dir;
        let file_name = file_name_for_new_entry(file_name, is_dir, path_style);
        let file_name = RelPath::new(Path::new(file_name.as_str()), path_style)
            .ok()?
            .into_arc();
        let worktree = self.project.read(cx).worktree_for_id(worktree_id, cx)?;
        let entry = worktree.read(cx).entry_for_id(edit_state.entry_id)?.clone();

        let edit_task;
        let edited_entry_id;
        if is_new_entry {
            let new_path = entry.path.join(&file_name);
            if worktree.read(cx).entry_for_path(&new_path).is_some() {
                return None;
            }

            edited_entry_id = Self::NEW_ENTRY_ID;
            self.selection = Some(SelectedEntry(Self::NEW_ENTRY_ID));
            let new_project_path: ProjectPath = (worktree_id, new_path).into();
            edit_task = self.project.update(cx, |project, cx| {
                project.create_entry(new_project_path, is_dir, cx)
            });
        } else {
            let new_path = if let Some(parent) = entry.path.parent() {
                parent.join(&file_name)
            } else {
                file_name.clone()
            };
            if let Some(existing_entry) = worktree.read(cx).entry_for_path(&new_path) {
                if existing_entry.id == entry.id && refocus {
                    window.focus(&self.focus_handle, cx);
                }
                return None;
            }

            edited_entry_id = entry.id;
            let new_project_path: ProjectPath = (worktree_id, new_path).into();
            edit_task = self.project.update(cx, |project, cx| {
                project.rename_entry(edited_entry_id, new_project_path, cx)
            });
        }

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
                        project_panel.update_visible_entries(None, false, is_new_entry, window, cx);
                        if is_new_entry && !is_dir {
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
            let root_entry_id = self
                .snapshot(cx)
                .and_then(|snapshot| snapshot.root_entry().map(|entry| entry.id));
            let previously_focused = edit_state
                .previously_focused
                .map(|entry| entry.0)
                .filter(|entry_id| Some(*entry_id) != root_entry_id);
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

    fn expand_entry(&mut self, entry_id: ProjectEntryId, cx: &mut Context<Self>) {
        self.project.update(cx, |project, cx| {
            if let Some(worktree) = project.worktree_for_entry(entry_id, cx)
                && let Some(expanded_dir_ids) = self.tree_state.expanded_dir_ids.as_mut()
            {
                project.expand_entry(entry_id, cx);
                let worktree = worktree.read(cx);

                if let Some(mut entry) = worktree.entry_for_id(entry_id) {
                    loop {
                        if let Err(index) = expanded_dir_ids.binary_search(&entry.id) {
                            expanded_dir_ids.insert(index, entry.id);
                        }

                        if let Some(parent_entry) = entry
                            .path
                            .parent()
                            .and_then(|path| worktree.entry_for_path(path))
                        {
                            entry = parent_entry;
                        } else {
                            break;
                        }
                    }
                }
            }
        });
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
            is_request: false,
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
            let mut details = self.details_for_entry(&snapshot, entry, cx);

            if let Some(edit_state) = &self.tree_state.edit_state {
                let is_edited_entry = if edit_state.is_new_entry() {
                    entry.id == Self::NEW_ENTRY_ID
                } else {
                    entry.id == edit_state.entry_id
                };

                if is_edited_entry {
                    if let Some(processing_file_name) = &edit_state.processing_file_name {
                        details.is_processing = true;
                        details.file_name.clear();
                        let processing_file_name = processing_file_name.as_unix_str();
                        if details.kind.is_file() {
                            details.file_name.push_str(
                                processing_file_name
                                    .strip_suffix(".toml")
                                    .unwrap_or(processing_file_name),
                            );
                        } else {
                            details.file_name.push_str(processing_file_name);
                        }
                    } else {
                        if edit_state.is_new_entry() {
                            details.file_name.clear();
                        } else {
                            details.file_name = self.file_name_editor.read(cx).text(cx);
                        }
                        details.is_editing = true;
                    }
                }
            }

            callback(entry.id, details, window, cx);
        }
    }

    fn open(
        &mut self,
        _: &actions::project_panel::Open,
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
        cx.emit(ProjectPanelEvent::OpenedEntry {
            entry_id,
            focus_opened_item,
            allow_preview,
        });
    }

    fn reveal_in_file_manager_path(&self, cx: &App) -> Option<PathBuf> {
        let project = self.project.read(cx);
        if let Some(selection) = self.selection
            && let Some(worktree) = project.worktree_for_entry(selection.0, cx)
        {
            let worktree = worktree.read(cx);
            if let Some(entry) = worktree.entry_for_id(selection.0) {
                return Some(worktree.absolutize(&entry.path));
            }
        }

        let root_entry_id = project.snapshot(cx)?.root_entry()?.id;
        let worktree = project.worktree_for_entry(root_entry_id, cx)?;
        let worktree = worktree.read(cx);
        let root_entry = worktree.entry_for_id(root_entry_id)?;
        Some(worktree.absolutize(&root_entry.path))
    }

    fn selected_entry_handle<'a>(&self, cx: &'a App) -> Option<(Entity<Worktree>, &'a Entry)> {
        let selection = self.selection?;
        let project = self.project.read(cx);
        let worktree = project.worktree_for_entry(selection.0, cx)?;
        let entry = worktree.read(cx).entry_for_id(selection.0)?;
        Some((worktree, entry))
    }

    pub fn selected_entry_project_path(&self, cx: &App) -> Option<ProjectPath> {
        let (worktree, entry) = self.selected_entry_handle(cx)?;
        Some(ProjectPath {
            worktree_id: worktree.read(cx).id(),
            path: entry.path.clone(),
        })
    }

    #[cfg(test)]
    fn visible_entries(&self, cx: &App) -> Vec<ProjectPath> {
        let project = self.project.read(cx);
        self.tree_state
            .visible_entries
            .iter()
            .filter_map(|entry| {
                let worktree = project.worktree_for_entry(entry.id, cx)?;
                Some(ProjectPath {
                    worktree_id: worktree.read(cx).id(),
                    path: entry.path.clone(),
                })
            })
            .collect()
    }

    fn write_entries_to_system_clipboard(&self, entries: &BTreeSet<SelectedEntry>, cx: &mut App) {
        let project = self.project.read(cx);
        let paths = entries
            .iter()
            .filter_map(|entry| {
                let project_path = project.path_for_entry(entry.0, cx)?;
                Some(
                    project
                        .absolute_path(&project_path, cx)?
                        .to_string_lossy()
                        .to_string(),
                )
            })
            .collect::<Vec<_>>();

        if !paths.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(paths.join("\n")));
        }
    }

    fn has_pasteable_content(&self) -> bool {
        self.clipboard
            .as_ref()
            .is_some_and(|clipboard| !clipboard.items().is_empty())
    }

    fn disjoint_entries(
        &self,
        entries: BTreeSet<SelectedEntry>,
        cx: &App,
    ) -> BTreeSet<SelectedEntry> {
        let mut sanitized_entries = BTreeSet::new();
        if entries.is_empty() {
            return sanitized_entries;
        }

        let project = self.project.read(cx);
        let entries = entries
            .into_iter()
            .filter(|entry| !project.entry_is_worktree_root(entry.0, cx))
            .collect::<Vec<_>>();
        let dir_paths = entries
            .iter()
            .filter_map(|entry| {
                let worktree = project.worktree_for_entry(entry.0, cx)?;
                let entry = worktree.read(cx).entry_for_id(entry.0)?.clone();
                entry.is_dir().then_some(entry.path)
            })
            .collect::<BTreeSet<_>>();

        sanitized_entries.extend(entries.into_iter().filter(|entry| {
            let Some(worktree) = project.worktree_for_entry(entry.0, cx) else {
                return false;
            };
            let Some(entry_info) = worktree.read(cx).entry_for_id(entry.0).cloned() else {
                return false;
            };
            let entry_path = entry_info.path.as_ref();
            let inside_selected_dir = dir_paths.iter().any(|dir_path| {
                entry_path != dir_path.as_ref() && entry_path.starts_with(dir_path.as_ref())
            });
            !inside_selected_dir
        }));

        sanitized_entries
    }

    fn effective_entries(&self) -> BTreeSet<SelectedEntry> {
        if let Some(selection) = self.selection {
            if self.marked_entries.is_empty() {
                return BTreeSet::from([selection]);
            }

            if self.marked_entries.len() == 1 && !self.marked_entries.contains(&selection) {
                return BTreeSet::from([selection]);
            }
        }

        self.marked_entries.iter().copied().collect::<BTreeSet<_>>()
    }

    fn disjoint_effective_entries(&self, cx: &App) -> BTreeSet<SelectedEntry> {
        self.disjoint_entries(self.effective_entries(), cx)
    }

    fn find_next_selection_after_deletion(
        &self,
        sanitized_entries: &BTreeSet<SelectedEntry>,
        cx: &mut Context<Self>,
    ) -> Option<SelectedEntry> {
        if sanitized_entries.is_empty() {
            return None;
        }

        let worktree = self.project.read(cx).root_worktree(cx)?;
        let worktree = worktree.read(cx);
        let latest_entry = sanitized_entries
            .iter()
            .filter_map(|entry| worktree.entry_for_id(entry.0))
            .max_by(|lhs, rhs| {
                cmp_worktree_entries(lhs, rhs, SortMode::DirectoriesFirst, SortOrder::Default)
            })?;
        let parent_path = latest_entry.path.parent()?;
        let parent_entry = worktree.entry_for_path(parent_path)?;

        let mut siblings = worktree
            .child_entries(parent_path)
            .filter(|sibling| {
                sibling.id == latest_entry.id
                    || !sanitized_entries.contains(&SelectedEntry(sibling.id))
            })
            .cloned()
            .collect::<Vec<_>>();
        siblings.sort_by(|lhs, rhs| {
            cmp_worktree_entries(lhs, rhs, SortMode::DirectoriesFirst, SortOrder::Default)
        });

        let sibling_entry_index = siblings
            .iter()
            .position(|sibling| sibling.id == latest_entry.id)?;

        if let Some(next_sibling) = sibling_entry_index
            .checked_add(1)
            .and_then(|index| siblings.get(index))
        {
            return Some(SelectedEntry(next_sibling.id));
        }
        if let Some(previous_sibling) = sibling_entry_index
            .checked_sub(1)
            .and_then(|index| siblings.get(index))
        {
            return Some(SelectedEntry(previous_sibling.id));
        }

        if worktree
            .root_entry()
            .is_some_and(|root_entry| root_entry.id == parent_entry.id)
        {
            return None;
        }

        Some(SelectedEntry(parent_entry.id))
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

            if snapshot.root_entry().map(|entry| entry.id) != Some(current_id)
                && let Ok(index) = expanded_dir_ids.binary_search(&current_id)
            {
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
        let mut depth = display_depth(selected_entry);

        let is_expanded_dir = selected_entry.kind.is_dir()
            && expanded_dir_ids.binary_search(&selected_entry.id).is_ok();
        if !is_expanded_dir {
            depth = depth.checked_sub(1)?;
            parent_row = self.tree_state.visible_entries[..selection_row]
                .iter()
                .enumerate()
                .rev()
                .find_map(|(row, entry)| (display_depth(entry) == depth).then_some(row))?;
        }

        let start = parent_row.checked_add(1)?;
        let end = self.tree_state.visible_entries[start..]
            .iter()
            .position(|entry| display_depth(entry) <= depth)
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

    fn render_root_header(root_name: String, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors();

        ui::h_flex()
            .flex_none()
            .h(DynamicSpacing::Base36.px(cx))
            .w_full()
            .px(DynamicSpacing::Base12.px(cx))
            .bg(colors.panel_background)
            .child(
                Label::new(root_name)
                    .size(LabelSize::Small)
                    .weight(FontWeight::MEDIUM)
                    .truncate(),
            )
            .into_any_element()
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
                    ui::h_flex()
                        .w(Self::PREFIX_LABEL_SLOT_WIDTH)
                        .flex_none()
                        .items_center()
                        .justify_end()
                        .child(
                            Icon::new(IconName::FileGeneric)
                                .size(IconSize::Medium)
                                .color(Color::Muted),
                        ),
                )
                .into_any_element()
        }
    }

    fn render_entry(
        &self,
        entry_id: ProjectEntryId,
        details: &EntryDetails,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let is_dir = details.kind.is_dir();
        let selection = SelectedEntry(entry_id);
        let colors = cx.theme().colors();
        let show_editor = details.is_editing && !details.is_processing;
        let is_marked = details.is_marked && !show_editor;
        let is_selected = details.is_selected && !show_editor;
        let bg_color = if is_marked {
            colors.element_selected
        } else if is_selected {
            colors.element_selection_background
        } else {
            colors.panel_background
        };
        let bg_hover_color = if is_marked {
            colors.element_selected
        } else if is_selected {
            colors.element_selection_background
        } else {
            colors.element_hover
        };
        let validation_color_and_message = if show_editor {
            let validation_state = self
                .tree_state
                .edit_state
                .as_ref()
                .map_or(ValidationState::None, |edit_state| {
                    edit_state.validation_state.clone()
                });
            match validation_state {
                ValidationState::Error(message) => Some((Color::Error.color(cx), message)),
                ValidationState::Warning(message) => Some((Color::Warning.color(cx), message)),
                ValidationState::None => None,
            }
        } else {
            None
        };

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
                            .mr(Pixels::ZERO - DynamicSpacing::Base06.px(cx) - gpui::px(1.0))
                            .border_1()
                            .border_color(
                                validation_color_and_message
                                    .as_ref()
                                    .map_or(colors.border_focused.opacity(0.7), |(color, _)| {
                                        *color
                                    }),
                            )
                            .child(self.file_name_editor.clone())
                    } else {
                        ui::h_flex()
                            .h_6()
                            .child(Label::new(details.file_name.clone()).single_line())
                    })
                    .on_secondary_mouse_down(cx.listener(
                        move |project_panel, event: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            if !project_panel.marked_entries.contains(&selection) {
                                project_panel.marked_entries.clear();
                            }
                            project_panel.deploy_context_menu(event.position, entry_id, window, cx);
                        },
                    ))
                    .overflow_x(),
            )
            .when_some(validation_color_and_message, |this, (color, message)| {
                this.relative().child(gpui::deferred(
                    gpui::div()
                        .occlude()
                        .absolute()
                        .top_full()
                        .left(gpui::px(-1.0))
                        .right(gpui::px(-1.0))
                        .py_1()
                        .px_2()
                        .border_1()
                        .border_color(color)
                        .bg(cx.theme().colors().background)
                        .child(
                            Label::new(message)
                                .color(Color::from(color))
                                .size(LabelSize::Small),
                        ),
                ))
            })
    }
}

#[inline]
fn cmp_worktree_entries(a: &Entry, b: &Entry, mode: SortMode, order: SortOrder) -> cmp::Ordering {
    let a = (a.path.as_ref(), a.is_file());
    let b = (b.path.as_ref(), b.is_file());
    path::compare_rel_paths_by(a, b, mode, order)
}

fn display_depth(entry: &Entry) -> usize {
    entry.path.components().count().saturating_sub(1)
}

fn file_name_for_entry(snapshot: &Snapshot, entry: &Entry) -> String {
    match entry.kind {
        EntryKind::File => file_stem_for_entry(entry).to_string(),
        EntryKind::Dir | EntryKind::PendingDir | EntryKind::UnloadedDir => {
            entry.path.file_name().map_or_else(
                || snapshot.root_name().as_unix_str().to_string(),
                ToString::to_string,
            )
        }
    }
}

fn file_stem_for_entry(entry: &Entry) -> &str {
    let file_name = entry.path.file_name().unwrap_or_default();
    file_name.strip_suffix(".toml").unwrap_or(file_name)
}

fn is_missing_entry_name(file_name: &str, is_dir: bool, path_style: PathStyle) -> bool {
    let Ok(file_name) = RelPath::new(Path::new(file_name), path_style) else {
        return false;
    };
    let Some(last_component) = file_name.file_name() else {
        return true;
    };

    if is_dir {
        return last_component.trim().is_empty();
    }

    let path = Path::new(last_component);
    let file_stem = if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"))
    {
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or(last_component)
    } else {
        last_component
    };

    file_stem.trim().is_empty()
}

fn file_name_for_new_entry(file_name: &str, is_dir: bool, path_style: PathStyle) -> String {
    if is_dir {
        return file_name.to_string();
    }

    let last_component = if path_style.is_windows() {
        file_name.rsplit(['/', '\\']).next().unwrap_or(file_name)
    } else {
        file_name.rsplit('/').next().unwrap_or(file_name)
    };
    if Path::new(last_component)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"))
    {
        return file_name.to_string();
    }

    let mut file_name = file_name.to_string();
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
    fn persistent_name() -> &'static str {
        Self::PANEL_KEY
    }

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
        actions::project_panel::ToggleFocus.boxed_clone()
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
            && self.project.read(cx).root_worktree(cx).is_some()
    }
}

impl Render for ProjectPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root_header = self
            .snapshot(cx)
            .map(|snapshot| snapshot.root_name().as_unix_str().to_string())
            .map(|root_name| Self::render_root_header(root_name, cx));
        let colors = cx.theme().colors();
        let entry_count = self.tree_state.visible_entries.len();

        gpui::div()
            .track_focus(&self.focus_handle)
            .key_context(self.dispatch_context(window, cx))
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
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::duplicate))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::rename))
            .on_action(cx.listener(Self::trash))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::copy_path))
            .on_action(cx.listener(Self::copy_relative_path))
            .on_action(cx.listener(Self::reveal_in_file_manager))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::open))
            .flex()
            .flex_col()
            .relative()
            .size_full()
            .bg(colors.panel_background)
            .when_some(root_header, |this, root_header| this.child(root_header))
            .child(
                gpui::div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(
                        gpui::uniform_list(
                            "project-panel-entries",
                            entry_count,
                            cx.processor(|this, range: Range<usize>, window, cx| {
                                this.load_entry_metadata_for_range(range.clone(), cx);
                                let mut items =
                                    Vec::with_capacity(range.end.saturating_sub(range.start));
                                this.for_each_visible_entry(
                                    range,
                                    window,
                                    cx,
                                    &mut |entry_id, details, window, cx| {
                                        items.push(
                                            this.render_entry(entry_id, &details, window, cx),
                                        );
                                    },
                                );
                                items
                            }),
                        )
                        .with_decoration(
                            ui::indent_guides(Self::INDENT_SIZE, IndentGuideColors::panel(cx))
                                .with_compute_indents_fn(
                                    cx.entity(),
                                    |this, range, _window, _cx| {
                                        let mut items = SmallVec::with_capacity(
                                            range.end.saturating_sub(range.start),
                                        );
                                        for index in range {
                                            if let Some(entry) =
                                                this.tree_state.visible_entries.get(index)
                                            {
                                                items.push(display_depth(entry));
                                            }
                                        }
                                        items
                                    },
                                )
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
                                        if snapshot
                                            .root_entry()
                                            .is_some_and(|entry| entry.id == parent_entry_id)
                                        {
                                            return;
                                        }
                                        let Some(expanded_dir_ids) =
                                            this.tree_state.expanded_dir_ids.as_mut()
                                        else {
                                            return;
                                        };
                                        let Ok(index) =
                                            expanded_dir_ids.binary_search(&parent_entry_id)
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

                                    let active_guide =
                                        this.find_active_indent_guide(&params.indent_guides);
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
                                            let guide_x =
                                                layout.offset.x * indent_size + left_offset;
                                            let guide_y = layout.offset.y * item_height + PADDING_Y;
                                            let guide_height =
                                                layout.length * item_height - PADDING_Y * 2.0;
                                            let bounds = Bounds::new(
                                                gpui::point(guide_x, guide_y),
                                                gpui::size(gpui::px(1.0), guide_height),
                                            );
                                            let hitbox_x = bounds.origin.x - HITBOX_OVERDRAW;
                                            let hitbox_width =
                                                bounds.size.width + HITBOX_OVERDRAW * 2.0;

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
                        .with_horizontal_sizing_behavior(
                            ListHorizontalSizingBehavior::Unconstrained,
                        )
                        .with_width_from_item(self.tree_state.max_width_item_index)
                        .track_scroll(&self.scroll_handle)
                        .size_full(),
                    )
                    .custom_scrollbars(
                        Scrollbars::new(ScrollAxes::Both)
                            .tracked_scroll_handle(&self.scroll_handle)
                            .with_track_along(
                                ScrollAxes::Vertical,
                                colors.panel_background,
                                TrackLayout::Overlay,
                            )
                            .with_track_along(
                                ScrollAxes::Horizontal,
                                colors.panel_background,
                                TrackLayout::Classic,
                            )
                            .notify_content(),
                        window,
                        cx,
                    )
                    .flex_1()
                    .w_full(),
            )
            .when(self.context_menu.is_some(), |this| {
                this.child(
                    gpui::div()
                        .absolute()
                        .top_0()
                        .right_0()
                        .bottom_0()
                        .left_0()
                        .occlude(),
                )
            })
            .children(self.context_menu.as_ref().map(|(menu, position, _)| {
                gpui::deferred(
                    gpui::anchored()
                        .position(*position)
                        .anchor(Anchor::TopLeft)
                        .child(menu.clone()),
                )
                .with_priority(3)
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use gpui::{Entity, TestAppContext, VisualTestContext};
    use indoc::indoc;
    use serde_json::json;
    use std::{collections::HashSet, ops::Range, sync::Arc};

    use fs::Fs;
    use path::rel_path;
    use project::{Project, ProjectEvent, ProjectPath};
    use request_editor::RequestEditor;
    use settings::SettingsStore;
    use theme::LoadThemes;
    use util_macros::path;
    use workspace::{SharedState, Workspace, build_workspace, pane::PaneEvent};

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
            if let Some(worktree) = panel.project.read(cx).root_worktree(cx) {
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
            if let Some(worktree) = panel.project.read(cx).root_worktree(cx) {
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

    fn select_path_with_mark(panel: &Entity<ProjectPanel>, path: &str, cx: &mut VisualTestContext) {
        let path = rel_path(path);
        panel.update_in(cx, |panel, window, cx| {
            if let Some(worktree) = panel.project.read(cx).root_worktree(cx) {
                let worktree = worktree.read(cx);
                if let Ok(relative_path) = path.strip_prefix(worktree.root_name())
                    && let Some(entry) = worktree.entry_for_path(relative_path)
                {
                    let selection = SelectedEntry(entry.id);
                    if !panel.marked_entries.contains(&selection) {
                        panel.marked_entries.push(selection);
                    }
                    panel.selection = Some(selection);
                    window.focus(&panel.focus_handle, cx);
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
            let worktree = workspace.project().read(cx).root_worktree(cx).unwrap();
            let worktree_id = worktree.read(cx).id();

            let opened_project_paths = workspace
                .pane()
                .read(cx)
                .active_item()
                .and_then(|item| item.project_path(cx))
                .into_iter()
                .collect::<Vec<_>>();
            assert_eq!(
                opened_project_paths,
                vec![ProjectPath {
                    worktree_id,
                    path: Arc::from(rel_path(expected_path)),
                }],
                "Should have opened file, selected in project panel"
            );
        });
    }

    #[track_caller]
    fn assert_validation_state(
        panel: &Entity<ProjectPanel>,
        expected: ValidationState,
        cx: &mut VisualTestContext,
    ) {
        let actual = panel.update(cx, |panel, _| {
            panel
                .tree_state
                .edit_state
                .as_ref()
                .unwrap()
                .validation_state
                .clone()
        });

        match (actual, expected) {
            (ValidationState::None, ValidationState::None) => {}
            (ValidationState::Warning(actual), ValidationState::Warning(expected))
            | (ValidationState::Error(actual), ValidationState::Error(expected)) => {
                assert_eq!(actual, expected);
            }
            (actual, expected) => panic!("Expected {expected:?}, got {actual:?}"),
        }
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
        let (workspace, cx) = build_workspace(&project, cx);
        let panel = workspace.update_in(cx, ProjectPanel::new);
        cx.run_until_parked();

        let actual = visible_entries_as_strings(&panel, 0..50, cx);

        assert_eq!(
            actual,
            vec![
                String::from("> Apple"),
                String::from("> Carrot"),
                String::from("  aardvark"),
                String::from("  banana"),
                String::from("  zebra"),
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
        let (workspace, cx) = build_workspace(&project, cx);
        let panel = workspace.update_in(cx, ProjectPanel::new);
        cx.run_until_parked();

        panel.update_in(cx, |panel, window, cx| {
            panel.new_file(&actions::project_panel::NewFile, window, cx);
        });
        cx.run_until_parked();

        panel.update_in(cx, |panel, window, cx| {
            assert!(panel.file_name_editor.read(cx).is_focused(window));
        });
        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            vec![
                String::from("> collection"),
                String::from("  [EDITOR: '']  <== selected"),
                String::from("  existing"),
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
                String::from("> collection"),
                String::from("  existing"),
                String::from("  New request  <== selected  <== marked"),
            ]
        );

        let is_request = panel.update(cx, |panel, cx| {
            let worktree = panel.project.read(cx).root_worktree(cx).unwrap();
            worktree
                .read(cx)
                .entry_for_path(rel_path("New request.toml"))
                .unwrap()
                .is_request
        });
        assert!(is_request);
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
        let (workspace, cx) = build_workspace(&project, cx);
        let panel = workspace.update_in(cx, ProjectPanel::new);
        cx.run_until_parked();

        panel.update_in(cx, |panel, window, cx| {
            panel.new_directory(&actions::project_panel::NewDirectory, window, cx);
        });
        cx.run_until_parked();

        panel.update_in(cx, |panel, window, cx| {
            assert!(panel.file_name_editor.read(cx).is_focused(window));
        });
        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            vec![
                String::from("> [EDITOR: '']  <== selected"),
                String::from("  existing"),
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
                String::from("v New collection  <== selected"),
                String::from("  existing"),
            ]
        );

        let metadata = temp_fs
            .metadata("project/New collection".as_ref())
            .await
            .unwrap()
            .unwrap();
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

                        [http]
                        method = "GET"
                        url = "https://api.zaku.dev/first"
                    "#},
                    "second.toml": indoc! {r#"
                        [meta]
                        version = 1

                        [http]
                        method = "POST"
                        url = "https://api.zaku.dev/second"
                    "#},
                    "third.toml": indoc! {r#"
                        [meta]
                        version = 1

                        [http]
                        method = "PUT"
                        url = "https://api.zaku.dev/third"
                    "#},
                }
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let (workspace, cx) = build_workspace(&project, cx);
        let panel = workspace.update_in(cx, ProjectPanel::new);
        let pane = workspace.update_in(cx, |workspace, _, _| workspace.pane().clone());

        cx.run_until_parked();

        toggle_expand_dir(&panel, "project/collection", cx);
        select_path(&panel, "project/collection/first.toml", cx);
        panel.update_in(cx, |panel, window, cx| {
            panel.open(&actions::project_panel::Open, window, cx);
        });
        pane.condition::<PaneEvent>(cx, |pane, cx| {
            pane.active_item()
                .and_then(|item| item.project_path(cx))
                .is_some_and(|project_path| {
                    project_path.path.as_ref() == rel_path("collection/first.toml")
                })
        })
        .await;

        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            vec![
                String::from("v collection"),
                String::from("      first  <== selected  <== marked"),
                String::from("      second"),
                String::from("      third"),
            ]
        );

        ensure_single_file_is_opened(&workspace, "collection/first.toml", cx);
        workspace.update_in(cx, |workspace, _, cx| {
            assert!(workspace.active_item_as::<RequestEditor>(cx).is_some());
        });

        select_path(&panel, "project/collection/second.toml", cx);
        panel.update_in(cx, |panel, window, cx| {
            panel.open(&actions::project_panel::Open, window, cx);
        });
        pane.condition::<PaneEvent>(cx, |pane, cx| {
            pane.active_item()
                .and_then(|item| item.project_path(cx))
                .is_some_and(|project_path| {
                    project_path.path.as_ref() == rel_path("collection/second.toml")
                })
        })
        .await;

        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            vec![
                String::from("v collection"),
                String::from("      first"),
                String::from("      second  <== selected  <== marked"),
                String::from("      third"),
            ]
        );

        ensure_single_file_is_opened(&workspace, "collection/second.toml", cx);
        workspace.update_in(cx, |workspace, _, cx| {
            assert!(workspace.active_item_as::<RequestEditor>(cx).is_some());
        });
    }

    #[gpui::test]
    async fn test_autoreveal_active_entry(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state, cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                "collection": {
                    "nested": {
                        "first.toml": "",
                    },
                    "second.toml": "",
                    "third.toml": "",
                },
                "other": {
                    "fourth.toml": "",
                }
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs, &project_path, cx).await;
        let (workspace, cx) = build_workspace(&project, cx);
        let panel = workspace.update_in(cx, ProjectPanel::new);
        cx.run_until_parked();

        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            &["> collection", "> other"]
        );

        let first_entry = panel.update(cx, |panel, cx| {
            let worktree = panel.project.read(cx).root_worktree(cx).unwrap();
            worktree
                .read(cx)
                .entry_for_path(rel_path("collection/nested/first.toml"))
                .unwrap()
                .id
        });
        let fourth_entry = panel.update(cx, |panel, cx| {
            let worktree = panel.project.read(cx).root_worktree(cx).unwrap();
            worktree
                .read(cx)
                .entry_for_path(rel_path("other/fourth.toml"))
                .unwrap()
                .id
        });

        panel.update(cx, |panel, cx| {
            panel.project.update(cx, |_, cx| {
                cx.emit(ProjectEvent::ActiveEntryChanged(Some(first_entry)));
            });
        });
        cx.run_until_parked();

        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            &[
                "v collection",
                "    v nested",
                "          first  <== selected  <== marked",
                "      second",
                "      third",
                "> other",
            ]
        );

        panel.update(cx, |panel, cx| {
            panel.project.update(cx, |_, cx| {
                cx.emit(ProjectEvent::ActiveEntryChanged(Some(fourth_entry)));
            });
        });
        cx.run_until_parked();

        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            &[
                "v collection",
                "    v nested",
                "          first",
                "      second",
                "      third",
                "v other",
                "      fourth  <== selected  <== marked",
            ]
        );
    }

    #[gpui::test]
    async fn test_new_entry_validation(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state, cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                "first": {},
                "first.toml": "",
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs, &project_path, cx).await;
        let (workspace, cx) = build_workspace(&project, cx);
        let panel = workspace.update_in(cx, ProjectPanel::new);
        cx.run_until_parked();

        panel.update_in(cx, |panel, window, cx| {
            panel.new_file(&actions::project_panel::NewFile, window, cx);
        });
        cx.run_until_parked();

        panel.update_in(cx, |panel, _, cx| {
            panel.file_name_editor.update(cx, |editor, cx| {
                editor.set_text("   ", cx);
            });
        });
        cx.run_until_parked();
        assert_validation_state(
            &panel,
            ValidationState::Error("File or directory name must be provided.".to_string()),
            cx,
        );
        assert!(
            panel
                .update_in(cx, |panel, window, cx| panel.confirm_edit(true, window, cx))
                .is_none()
        );

        panel.update_in(cx, |panel, _, cx| {
            panel.file_name_editor.update(cx, |editor, cx| {
                editor.set_text("   .toml", cx);
            });
        });
        cx.run_until_parked();
        assert_validation_state(
            &panel,
            ValidationState::Error("File or directory name must be provided.".to_string()),
            cx,
        );
        assert!(
            panel
                .update_in(cx, |panel, window, cx| panel.confirm_edit(true, window, cx))
                .is_none()
        );

        panel.update_in(cx, |panel, _, cx| {
            panel.file_name_editor.update(cx, |editor, cx| {
                editor.set_text("     second", cx);
            });
        });
        cx.run_until_parked();
        assert_validation_state(
            &panel,
            ValidationState::Warning(
                "File name contains leading or trailing whitespace.".to_string(),
            ),
            cx,
        );

        panel.update_in(cx, |panel, _, cx| {
            panel.file_name_editor.update(cx, |editor, cx| {
                editor.set_text("second", cx);
            });
        });
        cx.run_until_parked();
        assert_validation_state(&panel, ValidationState::None, cx);

        panel.update_in(cx, |panel, _, cx| {
            panel.file_name_editor.update(cx, |editor, cx| {
                editor.set_text("first", cx);
            });
        });
        cx.run_until_parked();
        assert_validation_state(
            &panel,
            ValidationState::Error(
                "File 'first.toml' already exists at location. Please choose a different name."
                    .to_string(),
            ),
            cx,
        );

        panel.update_in(cx, |panel, _, cx| {
            panel.file_name_editor.update(cx, |editor, cx| {
                editor.set_text("first/", cx);
            });
        });
        cx.run_until_parked();
        assert_validation_state(
            &panel,
            ValidationState::Error(
                "Directory 'first' already exists at location. Please choose a different name."
                    .to_string(),
            ),
            cx,
        );

        panel.update_in(cx, |panel, _, cx| {
            panel.file_name_editor.update(cx, |editor, cx| {
                editor.set_text("first.toml/", cx);
            });
        });
        cx.run_until_parked();
        assert_validation_state(
            &panel,
            ValidationState::Error(
                "File 'first.toml' already exists at location. Please choose a different name."
                    .to_string(),
            ),
            cx,
        );
    }

    #[gpui::test]
    async fn test_copy_paste(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state, cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                "first.v2.toml": "",
                "first.toml": "",
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs, &project_path, cx).await;
        let (workspace, cx) = build_workspace(&project, cx);
        let panel = workspace.update_in(cx, ProjectPanel::new);
        cx.run_until_parked();

        select_path(&panel, "project/first.toml", cx);
        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            vec![
                String::from("  first  <== selected"),
                String::from("  first.v2"),
            ]
        );

        panel.update_in(cx, |panel, window, cx| {
            panel.copy(&actions::project_panel::Copy, window, cx);
            panel.paste(&actions::project_panel::Paste, window, cx);
        });

        panel
            .condition::<ProjectPanelEvent>(cx, |panel, cx| {
                panel
                    .tree_state
                    .edit_state
                    .as_ref()
                    .is_some_and(|edit_state| edit_state.processing_file_name.is_none())
                    && panel.file_name_editor.read(cx).text(cx) == "first copy"
            })
            .await;
        cx.run_until_parked();

        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            vec![
                String::from("  first"),
                String::from("  [EDITOR: 'first copy']  <== selected  <== marked"),
                String::from("  first.v2"),
            ]
        );

        panel.update_in(cx, |panel, window, cx| {
            panel.file_name_editor.update(cx, |editor, cx| {
                let file_name_selections = editor
                    .selections
                    .all::<MultiBufferOffset>(&editor.display_snapshot(cx));
                assert_eq!(
                    file_name_selections.len(),
                    1,
                    "File editing should have a single selection, but got: {file_name_selections:?}"
                );
                let file_name_selection = &file_name_selections[0];
                assert_eq!(
                    file_name_selection.start,
                    MultiBufferOffset(0),
                    "Should select from the beginning of the file name"
                );
                assert_eq!(
                    file_name_selection.end,
                    MultiBufferOffset("first copy".len()),
                    "Should select the file name disambiguation"
                );
            });
            assert!(panel.confirm_edit(true, window, cx).is_none());
        });

        panel.update_in(cx, |panel, window, cx| {
            panel.paste(&actions::project_panel::Paste, window, cx);
        });

        panel
            .condition::<ProjectPanelEvent>(cx, |panel, cx| {
                panel
                    .tree_state
                    .edit_state
                    .as_ref()
                    .is_some_and(|edit_state| edit_state.processing_file_name.is_none())
                    && panel.file_name_editor.read(cx).text(cx) == "first copy 1"
            })
            .await;
        cx.run_until_parked();

        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            vec![
                String::from("  first"),
                String::from("  first copy"),
                String::from("  [EDITOR: 'first copy 1']  <== selected  <== marked"),
                String::from("  first.v2"),
            ]
        );

        panel.update_in(cx, |panel, window, cx| {
            assert!(panel.confirm_edit(true, window, cx).is_none());
        });
    }

    #[gpui::test]
    async fn test_cut_paste(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state, cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                "collection": {},
                "other": {},
                "first.toml": "",
                "second.toml": "",
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs, &project_path, cx).await;
        let (workspace, cx) = build_workspace(&project, cx);
        let panel = workspace.update_in(cx, ProjectPanel::new);
        cx.run_until_parked();

        select_path_with_mark(&panel, "project/first.toml", cx);
        select_path_with_mark(&panel, "project/second.toml", cx);

        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            vec![
                String::from("> collection"),
                String::from("> other"),
                String::from("  first  <== marked"),
                String::from("  second  <== selected  <== marked"),
            ]
        );

        panel.update_in(cx, |panel, window, cx| {
            panel.cut(&actions::project_panel::Cut, window, cx);
        });

        select_path(&panel, "project/collection", cx);
        panel.update_in(cx, |panel, window, cx| {
            panel.paste(&actions::project_panel::Paste, window, cx);
            panel.update_visible_entries(None, false, false, window, cx);
        });

        panel
            .condition::<ProjectPanelEvent>(cx, |panel, cx| {
                let visible_entries = panel.visible_entries(cx);
                let contains_path = |path| {
                    visible_entries
                        .iter()
                        .any(|entry| entry.path.as_ref() == rel_path(path))
                };

                contains_path("collection/first.toml")
                    && contains_path("collection/second.toml")
                    && !contains_path("first.toml")
                    && !contains_path("second.toml")
            })
            .await;
        cx.run_until_parked();

        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            vec![
                String::from("v collection"),
                String::from("      first  <== marked"),
                String::from("      second  <== selected  <== marked"),
                String::from("> other"),
            ]
        );

        panel.update_in(cx, |panel, window, cx| {
            panel.cancel(&actions::menu::Cancel, window, cx);
        });
        cx.run_until_parked();

        select_path(&panel, "project/other", cx);
        panel.update_in(cx, |panel, window, cx| {
            panel.paste(&actions::project_panel::Paste, window, cx);
        });

        panel
            .condition::<ProjectPanelEvent>(cx, |panel, cx| {
                let visible_entries = panel.visible_entries(cx);
                let contains_path = |path| {
                    visible_entries
                        .iter()
                        .any(|entry| entry.path.as_ref() == rel_path(path))
                };

                contains_path("other/first.toml") && contains_path("other/second.toml")
            })
            .await;
        cx.run_until_parked();

        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            vec![
                String::from("v collection"),
                String::from("      first"),
                String::from("      second"),
                String::from("v other"),
                String::from("      first"),
                String::from("      second  <== selected"),
            ]
        );
    }

    #[gpui::test]
    async fn test_duplicate(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state, cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                "collection": {
                    "first.toml": "",
                    "second.toml": "",
                },
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs, &project_path, cx).await;
        let (workspace, cx) = build_workspace(&project, cx);
        let panel = workspace.update_in(cx, ProjectPanel::new);
        cx.run_until_parked();

        toggle_expand_dir(&panel, "project/collection", cx);
        select_path(&panel, "project/collection/first.toml", cx);
        panel.update_in(cx, |panel, window, cx| {
            panel.duplicate(&actions::project_panel::Duplicate, window, cx);
        });

        panel
            .condition::<ProjectPanelEvent>(cx, |panel, cx| {
                let visible_entries = panel.visible_entries(cx);
                let contains_path = |path| {
                    visible_entries
                        .iter()
                        .any(|entry| entry.path.as_ref() == rel_path(path))
                };

                panel
                    .tree_state
                    .edit_state
                    .as_ref()
                    .is_some_and(|edit_state| edit_state.processing_file_name.is_none())
                    && panel.file_name_editor.read(cx).text(cx) == "first copy"
                    && contains_path("collection/first.toml")
                    && contains_path("collection/first copy.toml")
                    && contains_path("collection/second.toml")
            })
            .await;
        cx.run_until_parked();

        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            vec![
                String::from("v collection"),
                String::from("      first"),
                String::from("      [EDITOR: 'first copy']  <== selected  <== marked"),
                String::from("      second"),
            ]
        );
    }

    #[gpui::test]
    async fn test_rename(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state, cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                "collection": {
                    "first.toml": "",
                    "second.toml": "",
                },
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs, &project_path, cx).await;
        let (workspace, cx) = build_workspace(&project, cx);
        let panel = workspace.update_in(cx, ProjectPanel::new);
        cx.run_until_parked();

        toggle_expand_dir(&panel, "project/collection", cx);
        select_path(&panel, "project/collection/first.toml", cx);
        panel.update_in(cx, |panel, window, cx| {
            panel.rename(&actions::project_panel::Rename, window, cx);
        });
        cx.run_until_parked();

        panel.update_in(cx, |panel, window, cx| {
            assert!(panel.file_name_editor.read(cx).is_focused(window));
        });
        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            vec![
                String::from("v collection"),
                String::from("      [EDITOR: 'first']  <== selected"),
                String::from("      second"),
            ]
        );

        let confirm = panel.update_in(cx, |panel, window, cx| {
            panel.file_name_editor.update(cx, |editor, cx| {
                let file_name_selections = editor
                    .selections
                    .all::<MultiBufferOffset>(&editor.display_snapshot(cx));
                assert_eq!(
                    file_name_selections.len(),
                    1,
                    "File editing should have a single selection, but got: {file_name_selections:?}"
                );
                let file_name_selection = &file_name_selections[0];
                assert_eq!(
                    file_name_selection.start,
                    MultiBufferOffset(0),
                    "Should select the file name from the start"
                );
                assert_eq!(
                    file_name_selection.end,
                    MultiBufferOffset("first".len()),
                    "Should not select file extension"
                );

                editor.set_text("renamed", cx);
            });
            panel.confirm_edit(true, window, cx).unwrap()
        });
        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            vec![
                String::from("v collection"),
                String::from("      [PROCESSING: 'renamed']  <== selected"),
                String::from("      second"),
            ]
        );

        confirm.await.unwrap();
        cx.run_until_parked();
        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            vec![
                String::from("v collection"),
                String::from("      renamed  <== selected"),
                String::from("      second"),
            ]
        );
    }

    #[gpui::test]
    async fn test_rename_conflict(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state, cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                "collection": {
                    "first.toml": "",
                    "second.toml": "",
                    "third.toml": "",
                },
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs, &project_path, cx).await;
        let (workspace, cx) = build_workspace(&project, cx);
        let panel = workspace.update_in(cx, ProjectPanel::new);
        cx.run_until_parked();

        toggle_expand_dir(&panel, "project/collection", cx);
        select_path(&panel, "project/collection/first.toml", cx);
        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            vec![
                String::from("v collection"),
                String::from("      first  <== selected"),
                String::from("      second"),
                String::from("      third"),
            ]
        );

        panel.update_in(cx, |panel, window, cx| {
            panel.rename(&actions::project_panel::Rename, window, cx);
        });
        cx.run_until_parked();
        panel.update_in(cx, |panel, window, cx| {
            assert!(panel.file_name_editor.read(cx).is_focused(window));
        });
        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            vec![
                String::from("v collection"),
                String::from("      [EDITOR: 'first']  <== selected"),
                String::from("      second"),
                String::from("      third"),
            ]
        );

        panel.update_in(cx, |panel, window, cx| {
            panel.file_name_editor.update(cx, |editor, cx| {
                editor.set_text("second", cx);
            });
            assert!(
                panel.confirm_edit(true, window, cx).is_none(),
                "Should not allow to confirm on conflicting file rename"
            );
        });
        cx.run_until_parked();
        panel.update_in(cx, |panel, window, cx| {
            assert!(
                panel.tree_state.edit_state.is_some(),
                "Edit state should not be None after conflicting file rename"
            );
            panel.cancel(&actions::menu::Cancel, window, cx);
        });
        cx.run_until_parked();
        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            vec![
                String::from("v collection"),
                String::from("      first  <== selected"),
                String::from("      second"),
                String::from("      third"),
            ],
            "File list should be unchanged after failed rename confirmation"
        );
    }

    #[gpui::test]
    async fn test_trash(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state, cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                "collection": {
                    "first.toml": "",
                    "second.toml": "",
                    "third.toml": "",
                },
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let (workspace, cx) = build_workspace(&project, cx);
        let panel = workspace.update_in(cx, ProjectPanel::new);
        cx.run_until_parked();

        toggle_expand_dir(&panel, "project/collection", cx);
        select_path_with_mark(&panel, "project/collection/first.toml", cx);
        select_path_with_mark(&panel, "project/collection/second.toml", cx);
        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            vec![
                String::from("v collection"),
                String::from("      first  <== marked"),
                String::from("      second  <== selected  <== marked"),
                String::from("      third"),
            ]
        );

        panel.update_in(cx, |panel, window, cx| {
            panel.trash(
                &actions::project_panel::Trash { skip_prompt: true },
                window,
                cx,
            );
        });

        panel
            .condition::<ProjectPanelEvent>(cx, |panel, cx| {
                let visible_entries = panel.visible_entries(cx);
                let contains_path = |path| {
                    visible_entries
                        .iter()
                        .any(|entry| entry.path.as_ref() == rel_path(path))
                };

                contains_path("collection/third.toml")
                    && !contains_path("collection/first.toml")
                    && !contains_path("collection/second.toml")
            })
            .await;

        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            vec![
                String::from("v collection"),
                String::from("      third  <== selected"),
            ]
        );
        assert!(
            temp_fs
                .metadata("project/collection/first.toml".as_ref())
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            temp_fs
                .metadata("project/collection/second.toml".as_ref())
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            temp_fs
                .metadata("project/collection/third.toml".as_ref())
                .await
                .unwrap()
                .is_some()
        );
    }

    #[gpui::test]
    async fn test_delete(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state, cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                "collection": {
                    "first.toml": "",
                    "second.toml": "",
                },
                "other.toml": "",
                "third.toml": "",
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let (workspace, cx) = build_workspace(&project, cx);
        let panel = workspace.update_in(cx, ProjectPanel::new);
        let pane = workspace.update_in(cx, |workspace, _, _| workspace.pane().clone());
        cx.run_until_parked();

        select_path_with_mark(&panel, "project/other.toml", cx);
        panel.update_in(cx, |panel, window, cx| {
            panel.open(&actions::project_panel::Open, window, cx);
        });

        pane.condition::<PaneEvent>(cx, |pane, cx| {
            pane.active_item()
                .and_then(|item| item.project_path(cx))
                .is_some_and(|project_path| project_path.path.as_ref() == rel_path("other.toml"))
        })
        .await;
        assert_eq!(pane.read_with(cx, |pane, _| pane.items_len()), 1);

        select_path_with_mark(&panel, "project/collection", cx);
        select_path_with_mark(&panel, "project/other.toml", cx);
        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            vec![
                String::from("> collection  <== marked"),
                String::from("  other  <== selected  <== marked"),
                String::from("  third"),
            ]
        );

        panel.update_in(cx, |panel, window, cx| {
            panel.delete(
                &actions::project_panel::Delete { skip_prompt: true },
                window,
                cx,
            );
        });

        panel
            .condition::<ProjectPanelEvent>(cx, |panel, cx| {
                let visible_entries = panel.visible_entries(cx);
                let contains_path = |path| {
                    visible_entries
                        .iter()
                        .any(|entry| entry.path.as_ref() == rel_path(path))
                };

                contains_path("third.toml")
                    && !contains_path("collection")
                    && !contains_path("other.toml")
            })
            .await;

        pane.condition::<PaneEvent>(cx, |pane, _| pane.items_len() == 0)
            .await;

        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            vec![String::from("  third  <== selected")]
        );
        assert_eq!(pane.read_with(cx, |pane, _| pane.items_len()), 0);
        assert!(
            temp_fs
                .metadata("project/collection".as_ref())
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            temp_fs
                .metadata("project/other.toml".as_ref())
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            temp_fs
                .metadata("project/third.toml".as_ref())
                .await
                .unwrap()
                .is_some()
        );
    }

    #[gpui::test]
    async fn test_non_request_files_are_hidden(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state, cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                ".gitignore": "",
                "README.md": "",
                "collection": {
                    "first.toml": "",
                    "nested": {
                        "config.json": "{}",
                    },
                    "scripts": {
                        "index.js": "",
                        "second.toml": "",
                    },
                },
                "request.toml": "",
                "settings.json": "{}",
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs, &project_path, cx).await;
        let (workspace, cx) = build_workspace(&project, cx);
        let panel = workspace.update_in(cx, ProjectPanel::new);
        cx.run_until_parked();

        let non_request_files_are_indexed = panel.update(cx, |panel, cx| {
            let worktree = panel.project.read(cx).root_worktree(cx).unwrap();
            let worktree = worktree.read(cx);
            let settings = worktree.entry_for_path(rel_path("settings.json")).unwrap();
            let script = worktree
                .entry_for_path(rel_path("collection/scripts/index.js"))
                .unwrap();

            !settings.is_request && !script.is_request
        });
        assert!(non_request_files_are_indexed);

        toggle_expand_dir(&panel, "project/collection", cx);
        toggle_expand_dir(&panel, "project/collection/nested", cx);
        toggle_expand_dir(&panel, "project/collection/scripts", cx);

        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            vec![
                String::from("v collection"),
                String::from("    v nested"),
                String::from("    v scripts"),
                String::from("          second"),
                String::from("      first"),
                String::from("  request"),
            ]
        );
    }
}
