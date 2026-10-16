use gpui::{
    Action, Anchor, App, ClickEvent, Context, DismissEvent, Div, Entity, FocusHandle, Focusable,
    KeyContext, MouseDownEvent, Pixels, Point, ScrollHandle, SharedString, Stateful, Subscription,
    Task, WeakEntity, Window, prelude::*,
};
use std::{iter, sync::Arc};
use strum::IntoEnumIterator;

use editor::{
    Editor, EditorEvent, MultiBufferOffset, SelectionEffects,
    items::{entry_git_aware_text_color, entry_text_color, git_status_indicator},
};
use path::RelPath;
use project::{
    ConfigFileType, EnvironmentColor, Project, ProjectEntryId, ProjectPath, WorktreeId,
    git_store::{GitStoreEvent, RepositoryEvent},
    project_config_store::ProjectConfigStoreEvent,
};
use settings::{GitSettings, Settings};
use theme::ActiveTheme;
use ui::{
    ButtonCommon, Clickable, Color, ContextMenu, DynamicSpacing, Icon, IconAsset, IconButton,
    IconButtonShape, IconPosition, IconSize, ListItem, ListItemSpacing, ScrollAxes, Scrollbars,
    Text, TextCommon, TextSize, Tooltip, TrackLayout, WithScrollbar,
};
use util::ResultExt;
use workspace::{Panel, Workspace, WorkspaceEvent, notifications::DetachAndPromptErr};

pub fn init(cx: &mut App) {
    cx.observe_new(
        |workspace: &mut Workspace, _window, _: &mut Context<Workspace>| {
            workspace.register_action(
                |workspace, _: &actions::variables_panel::ToggleFocus, window, cx| {
                    workspace.toggle_panel_focus::<VariablesPanel>(window, cx);
                },
            );
        },
    )
    .detach();
}

#[derive(Clone, Copy, PartialEq)]
enum SelectedEntry {
    ProjectVariables,
    Environment(ProjectEntryId),
}

impl SelectedEntry {
    fn for_path(project: &Entity<Project>, path: &RelPath, cx: &App) -> Option<Self> {
        match ConfigFileType::for_path(path)? {
            ConfigFileType::Project => Some(Self::ProjectVariables),
            ConfigFileType::Environment => {
                let worktree = project.read(cx).root_worktree(cx)?;
                let entry_id = worktree.read(cx).entry_for_path(path)?.id;
                Some(Self::Environment(entry_id))
            }
        }
    }
}

struct EnvironmentEntry {
    entry_id: ProjectEntryId,
    path: Arc<RelPath>,
    name: SharedString,
    color: Option<EnvironmentColor>,
}

enum VariablesListEntry {
    ProjectVariables,
    Environment(EnvironmentEntry),
}

impl VariablesListEntry {
    fn path(&self) -> Arc<RelPath> {
        match self {
            Self::ProjectVariables => path::project_config_file_relative_path().into_arc(),
            Self::Environment(environment) => environment.path.clone(),
        }
    }

    fn selected_entry(&self) -> SelectedEntry {
        match self {
            Self::ProjectVariables => SelectedEntry::ProjectVariables,
            Self::Environment(environment) => SelectedEntry::Environment(environment.entry_id),
        }
    }
}

#[derive(Debug, PartialEq)]
enum ValidationState {
    None,
    Error(String),
}

struct EditState {
    entry_id: Option<ProjectEntryId>,
    processing_name: Option<SharedString>,
    previously_focused: Option<SelectedEntry>,
    validation_state: ValidationState,
}

impl EditState {
    fn is_new_entry(&self) -> bool {
        self.entry_id.is_none()
    }
}

pub struct VariablesPanel {
    focus_handle: FocusHandle,
    project: Entity<Project>,
    workspace: WeakEntity<Workspace>,
    scroll_handle: ScrollHandle,
    selection: Option<SelectedEntry>,
    active_entry: Option<SelectedEntry>,
    context_menu: Option<(Entity<ContextMenu>, Point<Pixels>, Subscription)>,
    edit_state: Option<EditState>,
    environment_name_editor: Entity<Editor>,
}

impl VariablesPanel {
    const PANEL_KEY: &str = "VariablesPanel";
    const DEFAULT_SIZE: Pixels = gpui::px(250.0);
    const NEW_ENTRY_ID: ProjectEntryId = ProjectEntryId::MAX;

    pub fn new(
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Entity<Self> {
        let project = workspace.project().clone();
        let project_config_store = project.read(cx).project_config_store().clone();
        let git_store = project.read(cx).git_store().clone();
        let workspace_entity = cx.entity();
        let workspace_handle = workspace.weak_handle();
        let active_entry = workspace
            .active_item(cx)
            .and_then(|item| item.project_path(cx))
            .and_then(|project_path| SelectedEntry::for_path(&project, &project_path.path, cx));

        cx.new(|cx| {
            let environment_name_editor = cx.new(|cx| Editor::single_line(window, cx));
            cx.subscribe_in(
                &workspace_entity,
                window,
                |this: &mut VariablesPanel, workspace, event, _, cx| match event {
                    WorkspaceEvent::ActiveItemChanged => {
                        this.active_entry = workspace
                            .read(cx)
                            .active_item(cx)
                            .and_then(|item| item.project_path(cx))
                            .and_then(|project_path| {
                                SelectedEntry::for_path(&this.project, &project_path.path, cx)
                            });
                        if let Some(active_entry) = this.active_entry {
                            this.selection = Some(active_entry);
                            this.autoscroll(cx);
                        }
                        cx.notify();
                    }
                    WorkspaceEvent::PaneAdded(_) | WorkspaceEvent::PaneRestored(_) => {}
                },
            )
            .detach();

            cx.subscribe(
                &project_config_store,
                |_, _, event: &ProjectConfigStoreEvent, cx| match event {
                    ProjectConfigStoreEvent::ConfigFileUpdated(_)
                    | ProjectConfigStoreEvent::ConfigFilesLoaded
                    | ProjectConfigStoreEvent::ActiveEnvironmentChanged => cx.notify(),
                },
            )
            .detach();

            cx.subscribe_in(
                &environment_name_editor,
                window,
                |this: &mut VariablesPanel, _, editor_event, window, cx| match editor_event {
                    EditorEvent::BufferEdited => {
                        this.populate_validation_error(cx);
                    }
                    EditorEvent::Blurred if window.is_window_active() => {
                        if this
                            .edit_state
                            .as_ref()
                            .is_some_and(|edit_state| edit_state.processing_name.is_none())
                        {
                            match this.confirm_edit(false, window, cx) {
                                Some(task) => task.detach_and_log_err(cx),
                                None => this.discard_edit_state(cx),
                            }
                        }
                    }
                    EditorEvent::Blurred
                    | EditorEvent::DirtyChanged
                    | EditorEvent::FileHandleChanged
                    | EditorEvent::Saved
                    | EditorEvent::TitleChanged => {}
                },
            )
            .detach();

            cx.subscribe(&git_store, |_, _, event: &GitStoreEvent, cx| match event {
                GitStoreEvent::RepositoryUpdated(_, RepositoryEvent::StatusesChanged, _)
                | GitStoreEvent::RepositoryAdded
                | GitStoreEvent::RepositoryRemoved(_) => cx.notify(),
                _ => {}
            })
            .detach();

            Self {
                focus_handle: cx.focus_handle(),
                project,
                workspace: workspace_handle,
                scroll_handle: ScrollHandle::new(),
                selection: None,
                active_entry,
                context_menu: None,
                edit_state: None,
                environment_name_editor,
            }
        })
    }

    fn entries(&self, cx: &App) -> Vec<VariablesListEntry> {
        let project = self.project.read(cx);
        let environment_file_entries = project
            .root_worktree(cx)
            .map(|worktree| {
                worktree
                    .read(cx)
                    .child_entries(path::project_environments_folder_relative_path())
                    .filter(|entry| {
                        ConfigFileType::for_path(&entry.path) == Some(ConfigFileType::Environment)
                    })
                    .map(|entry| (entry.id, entry.path.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut environments = project
            .project_config_store()
            .read(cx)
            .environments()
            .filter_map(|(name, environment_file)| {
                let (entry_id, path) = environment_file_entries
                    .iter()
                    .find(|(_, path)| path.file_stem() == Some(name))?;
                Some(EnvironmentEntry {
                    entry_id: *entry_id,
                    path: path.clone(),
                    name: SharedString::from(name.to_string()),
                    color: environment_file.environment.color,
                })
            })
            .collect::<Vec<_>>();
        environments.sort_by(|left, right| path::natural_sort(&left.name, &right.name));

        iter::once(VariablesListEntry::ProjectVariables)
            .chain(
                environments
                    .into_iter()
                    .map(VariablesListEntry::Environment),
            )
            .collect()
    }

    fn index_for_selection(&self, cx: &App) -> Option<usize> {
        let selection = self.selection?;
        self.entries(cx)
            .iter()
            .position(|entry| entry.selected_entry() == selection)
    }

    fn dispatch_context(&self, window: &Window, cx: &Context<Self>) -> KeyContext {
        let mut dispatch_context = KeyContext::new_with_defaults();
        dispatch_context.add(Self::PANEL_KEY);
        dispatch_context.add("menu");

        let identifier = if self
            .environment_name_editor
            .focus_handle(cx)
            .is_focused(window)
        {
            "editing"
        } else {
            "not_editing"
        };

        dispatch_context.add(identifier);
        dispatch_context
    }

    fn autoscroll(&mut self, cx: &mut Context<Self>) {
        let Some(selection) = self.selection else {
            return;
        };
        if let Some(index) = self
            .entries(cx)
            .iter()
            .filter(|entry| matches!(entry, VariablesListEntry::Environment(_)))
            .position(|entry| entry.selected_entry() == selection)
        {
            self.scroll_handle.scroll_to_item(index);
            cx.notify();
        }
    }

    fn select_previous(
        &mut self,
        _: &actions::menu::SelectPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(current_index) = self.index_for_selection(cx) {
            let Some(target_index) = current_index.checked_sub(1) else {
                return;
            };
            let Some(entry) = self.entries(cx).into_iter().nth(target_index) else {
                return;
            };

            self.selection = Some(entry.selected_entry());
            window.focus(&self.focus_handle, cx);
            self.autoscroll(cx);
            cx.notify();
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
        if let Some(current_index) = self.index_for_selection(cx) {
            let Some(target_index) = current_index.checked_add(1) else {
                return;
            };
            let Some(entry) = self.entries(cx).into_iter().nth(target_index) else {
                return;
            };

            self.selection = Some(entry.selected_entry());
            window.focus(&self.focus_handle, cx);
            self.autoscroll(cx);
            cx.notify();
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
        if let Some(entry) = self.entries(cx).into_iter().next() {
            self.selection = Some(entry.selected_entry());
            window.focus(&self.focus_handle, cx);
            self.autoscroll(cx);
            cx.notify();
        }
    }

    fn select_last(
        &mut self,
        _: &actions::menu::SelectLast,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(entry) = self.entries(cx).pop() {
            self.selection = Some(entry.selected_entry());
            window.focus(&self.focus_handle, cx);
            self.autoscroll(cx);
            cx.notify();
        }
    }

    fn open(
        &mut self,
        _: &actions::variables_panel::Open,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self
            .index_for_selection(cx)
            .and_then(|index| self.entries(cx).into_iter().nth(index))
        else {
            return;
        };

        self.open_entry(entry.path(), false, true, window, cx);
        cx.notify();
    }

    fn open_entry(
        &mut self,
        path: Arc<RelPath>,
        focus_opened_item: bool,
        allow_preview: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(worktree) = self.project.read(cx).root_worktree(cx) else {
            return;
        };

        let project_path = ProjectPath {
            worktree_id: worktree.read(cx).id(),
            path,
        };
        self.workspace
            .update(cx, |workspace, cx| {
                workspace
                    .open_path_preview(
                        project_path,
                        None,
                        focus_opened_item,
                        allow_preview,
                        true,
                        window,
                        cx,
                    )
                    .detach_and_log_err(cx);
            })
            .log_err();

        if !focus_opened_item {
            window.focus(&self.focus_handle, cx);
        }
    }

    fn new_environment(
        &mut self,
        _: &actions::variables_panel::NewEnvironment,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.project.read(cx).root_worktree(cx).is_none() {
            return;
        }

        let previously_focused = self
            .selection
            .filter(|selection| *selection != SelectedEntry::Environment(Self::NEW_ENTRY_ID))
            .or_else(|| {
                self.edit_state
                    .as_ref()
                    .filter(|edit_state| edit_state.is_new_entry())
                    .and_then(|edit_state| edit_state.previously_focused)
            });
        self.selection = Some(SelectedEntry::Environment(Self::NEW_ENTRY_ID));
        self.edit_state = Some(EditState {
            entry_id: None,
            processing_name: None,
            previously_focused,
            validation_state: ValidationState::None,
        });
        self.environment_name_editor.update(cx, |editor, cx| {
            editor.clear(window, cx);
        });
        window.focus(&self.environment_name_editor.focus_handle(cx), cx);
        let environment_count = self
            .project
            .read(cx)
            .project_config_store()
            .read(cx)
            .environments()
            .count();
        self.scroll_handle.scroll_to_item(environment_count);
        cx.notify();
    }

    fn rename(
        &mut self,
        _: &actions::variables_panel::Rename,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(VariablesListEntry::Environment(environment)) = self
            .index_for_selection(cx)
            .and_then(|index| self.entries(cx).into_iter().nth(index))
        else {
            return;
        };

        let name = environment.name;
        self.edit_state = Some(EditState {
            entry_id: Some(environment.entry_id),
            processing_name: None,
            previously_focused: None,
            validation_state: ValidationState::None,
        });
        self.environment_name_editor.update(cx, |editor, cx| {
            editor.set_text(&name, cx);
            editor.change_selections(SelectionEffects::default(), cx, |selections| {
                selections.select_ranges([MultiBufferOffset(0)..MultiBufferOffset(name.len())]);
            });
        });
        window.focus(&self.environment_name_editor.focus_handle(cx), cx);
        self.autoscroll(cx);
        cx.notify();
    }

    fn deploy_context_menu(
        &mut self,
        position: Point<Pixels>,
        selected_entry: SelectedEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(worktree) = self.project.read(cx).root_worktree(cx) else {
            return;
        };
        let Some(VariablesListEntry::Environment(environment)) = self
            .entries(cx)
            .into_iter()
            .find(|entry| entry.selected_entry() == selected_entry)
        else {
            return;
        };

        self.selection = Some(selected_entry);
        let project = self.project.clone();
        let project_path = ProjectPath {
            worktree_id: worktree.read(cx).id(),
            path: environment.path,
        };
        let context_menu = ContextMenu::build(window, cx, |menu, _, _| {
            menu.context(self.focus_handle.clone())
                .action("Rename", Box::new(actions::variables_panel::Rename))
                .separator()
                .submenu("Color", move |mut menu, _, _| {
                    for color in iter::once(None).chain(EnvironmentColor::iter().map(Some)) {
                        let project = project.clone();
                        let project_path = project_path.clone();
                        let display_name = color.map_or("None", |color| color.display_name());
                        menu = menu.toggleable_entry(
                            display_name,
                            color == environment.color,
                            IconPosition::End,
                            None,
                            move |window, cx| {
                                project
                                    .update(cx, |project, cx| {
                                        project.set_environment_color(
                                            project_path.clone(),
                                            color,
                                            cx,
                                        )
                                    })
                                    .detach_and_prompt_err(
                                        "Failed to change color",
                                        window,
                                        cx,
                                        |_, _, _| None,
                                    );
                            },
                        );
                    }
                    menu
                })
        });

        window.focus(&context_menu.focus_handle(cx), cx);
        let subscription = cx.subscribe(&context_menu, |this, _, _: &DismissEvent, cx| {
            this.context_menu.take();
            cx.notify();
        });
        self.context_menu = Some((context_menu, position, subscription));
        cx.notify();
    }

    fn confirm(&mut self, _: &actions::menu::Confirm, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(task) = self.confirm_edit(true, window, cx) {
            task.detach_and_log_err(cx);
        }
    }

    fn cancel(&mut self, _: &actions::menu::Cancel, window: &mut Window, cx: &mut Context<Self>) {
        self.discard_edit_state(cx);
        window.focus(&self.focus_handle, cx);
    }

    fn populate_validation_error(&mut self, cx: &mut Context<Self>) {
        let name = self.environment_name_editor.read(cx).text(cx);
        let validation_state = self.environment_name_validation_state(&name, cx);
        let Some(edit_state) = self.edit_state.as_mut() else {
            return;
        };

        edit_state.validation_state = validation_state;
        cx.notify();
    }

    fn environment_name_validation_state(&self, name: &str, cx: &App) -> ValidationState {
        if name.is_empty() {
            return ValidationState::None;
        }
        if name.trim().is_empty() {
            return ValidationState::Error("Environment name must be provided.".to_string());
        }
        if !is_valid_environment_name(name) {
            return ValidationState::Error(
                "Environment name must start and end with a letter or digit and contain only letters, digits, '_', '-', '.' and single spaces."
                    .to_string(),
            );
        }

        let Some(worktree) = self.project.read(cx).root_worktree(cx) else {
            return ValidationState::None;
        };
        let edited_entry_id = self
            .edit_state
            .as_ref()
            .and_then(|edit_state| edit_state.entry_id);
        let existing_name = worktree
            .read(cx)
            .child_entries(path::project_environments_folder_relative_path())
            .filter(|entry| {
                Some(entry.id) != edited_entry_id
                    && ConfigFileType::for_path(&entry.path) == Some(ConfigFileType::Environment)
            })
            .filter_map(|entry| entry.path.file_stem())
            .find(|existing_name| existing_name.eq_ignore_ascii_case(name))
            .map(ToString::to_string);
        if let Some(existing_name) = existing_name {
            return ValidationState::Error(format!(
                "Environment '{existing_name}' already exists."
            ));
        }

        ValidationState::None
    }

    fn confirm_edit(
        &mut self,
        refocus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<anyhow::Result<()>>> {
        let edited_entry_id = self.edit_state.as_ref()?.entry_id;
        let name = self.environment_name_editor.read(cx).text(cx);
        if name.is_empty()
            || !matches!(
                self.environment_name_validation_state(&name, cx),
                ValidationState::None
            )
        {
            return None;
        }

        let worktree = self.project.read(cx).root_worktree(cx)?;
        let new_path = path::project_environments_folder_relative_path()
            .join(RelPath::from_unix_str(&format!("{name}.toml")).ok()?);
        let new_project_path = ProjectPath {
            worktree_id: worktree.read(cx).id(),
            path: new_path.clone(),
        };
        let edit_task = if let Some(entry_id) = edited_entry_id {
            if worktree.read(cx).entry_for_id(entry_id)?.path == new_path {
                if refocus {
                    window.focus(&self.focus_handle, cx);
                }
                return None;
            }

            self.project.update(cx, |project, cx| {
                project.rename_entry(entry_id, new_project_path, cx)
            })
        } else {
            self.project.update(cx, |project, cx| {
                project.create_entry(new_project_path, false, cx)
            })
        };

        if refocus {
            window.focus(&self.focus_handle, cx);
        }
        if let Some(edit_state) = self.edit_state.as_mut() {
            edit_state.processing_name = Some(SharedString::from(name));
        }
        cx.notify();

        Some(cx.spawn_in(window, async move |variables_panel, cx| {
            let new_entry = edit_task.await;
            variables_panel.update(cx, |variables_panel, cx| {
                variables_panel.edit_state = None;
                cx.notify();
            })?;
            let new_entry = new_entry?;

            variables_panel.update_in(cx, |variables_panel, window, cx| {
                let is_new_entry = edited_entry_id.is_none();
                let edited_entry_id = edited_entry_id.unwrap_or(Self::NEW_ENTRY_ID);
                if variables_panel.selection == Some(SelectedEntry::Environment(edited_entry_id)) {
                    variables_panel.selection = Some(SelectedEntry::Environment(new_entry.id));
                }
                if is_new_entry {
                    variables_panel.open_entry(new_path, true, false, window, cx);
                }
                cx.notify();
            })
        }))
    }

    fn discard_edit_state(&mut self, cx: &mut Context<Self>) {
        let Some(edit_state) = self.edit_state.take() else {
            return;
        };

        if let Some(previously_focused) = edit_state.previously_focused {
            self.selection = Some(previously_focused);
            self.autoscroll(cx);
        }
        cx.notify();
    }

    fn render_header(&self, title: SharedString, cx: &mut Context<Self>) -> Div {
        gpui::div()
            .flex_none()
            .flex()
            .items_center()
            .gap_1()
            .h(DynamicSpacing::Base36.px(cx))
            .w_full()
            .px(DynamicSpacing::Base12.px(cx))
            .bg(cx.theme().colors().panel_background)
            .child(
                gpui::div().flex_1().min_w_0().child(
                    Text::new(title)
                        .size(TextSize::Small)
                        .single_line()
                        .truncate(),
                ),
            )
    }

    fn render_entry(
        &self,
        index: usize,
        entry: VariablesListEntry,
        worktree_id: WorktreeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let path = entry.path();
        let project_path = ProjectPath {
            worktree_id,
            path: path.clone(),
        };
        if let VariablesListEntry::Environment(environment) = &entry
            && let Some(edit_state) = self.edit_state.as_ref()
            && edit_state.entry_id == Some(environment.entry_id)
        {
            return self
                .render_edit_entry(edit_state, environment.color, window, cx)
                .id(("variables-panel-entry", index));
        }

        let theme_colors = cx.theme().colors();
        let is_environment = matches!(entry, VariablesListEntry::Environment(_));
        let selected_entry = entry.selected_entry();
        let is_marked = self.active_entry == Some(selected_entry);
        let is_selected = self.selection == Some(selected_entry);
        let project = self.project.read(cx);
        let git_status = project
            .project_path_git_status(&project_path, cx)
            .map(|status| status.summary())
            .unwrap_or_default();
        let is_ignored = project
            .entry_for_path(&project_path, cx)
            .is_some_and(|entry| entry.is_ignored);
        let active_environment = project.project_config_store().read(cx).active_environment();
        let is_active = match &entry {
            VariablesListEntry::ProjectVariables => false,
            VariablesListEntry::Environment(environment) => {
                active_environment == Some(environment.name.as_ref())
            }
        };
        let git_settings = GitSettings::get_global(cx);
        let git_status_enabled = git_settings.is_git_status_enabled();
        let text_color = if git_status_enabled && git_settings.status.project_panel.colors {
            entry_git_aware_text_color(git_status, is_ignored, is_marked)
        } else {
            entry_text_color(is_marked)
        };
        let git_indicator = (git_status_enabled && git_settings.status.project_panel.indicators)
            .then(|| git_status_indicator(git_status))
            .flatten();
        let bg_color = if is_marked {
            theme_colors.element_selected
        } else if is_selected {
            theme_colors.element_selection_background
        } else {
            theme_colors.panel_background
        };
        let bg_hover_color = if is_marked {
            theme_colors.element_selected
        } else if is_selected {
            theme_colors.element_selection_background
        } else {
            theme_colors.element_hover
        };
        let icon_color = entry_text_color(is_marked);
        let (icon, name) = match entry {
            VariablesListEntry::ProjectVariables => (
                Icon::new(IconAsset::SquareInset)
                    .size(IconSize::XSmall)
                    .color(icon_color),
                SharedString::from("Project Variables"),
            ),
            VariablesListEntry::Environment(environment) => (
                environment_selector::environment_icon(environment.color, icon_color),
                environment.name,
            ),
        };
        let entry_prefix_slot_width = window.rem_size();

        gpui::div()
            .id(("variables-panel-entry", index))
            .relative()
            .cursor_pointer()
            .rounded_none()
            .bg(bg_color)
            .border_1()
            .border_color(gpui::transparent_black())
            .hover(move |style| style.bg(bg_hover_color))
            .on_click(
                cx.listener(move |variables_panel, event: &ClickEvent, window, cx| {
                    if event.is_right_click() {
                        return;
                    }

                    cx.stop_propagation();
                    variables_panel.selection = Some(selected_entry);
                    let click_count = event.click_count();
                    variables_panel.open_entry(
                        path.clone(),
                        click_count > 1,
                        click_count == 1,
                        window,
                        cx,
                    );
                    cx.notify();
                }),
            )
            .child(
                ListItem::new(("variables-panel-entry", index))
                    .spacing(ListItemSpacing::Dense)
                    .selectable(false)
                    .when(is_active || git_indicator.is_some(), |this| {
                        this.end_slot(
                            gpui::div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .flex_none()
                                .pr_3()
                                .when(is_active, |this| {
                                    this.child(
                                        Icon::new(IconAsset::Check)
                                            .size(IconSize::XSmall)
                                            .color(Color::Accent),
                                    )
                                })
                                .when_some(git_indicator, |this, (text, color)| {
                                    this.child(Text::new(text).size(TextSize::Small).color(color))
                                }),
                        )
                    })
                    .child(
                        gpui::div()
                            .w(entry_prefix_slot_width)
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icon),
                    )
                    .child(
                        gpui::div()
                            .flex()
                            .items_center()
                            .h_6()
                            .min_w_0()
                            .child(Text::new(name).color(text_color).single_line().truncate()),
                    )
                    .when(is_environment, |this| {
                        this.on_secondary_mouse_down(cx.listener(
                            move |variables_panel, event: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                variables_panel.deploy_context_menu(
                                    event.position,
                                    selected_entry,
                                    window,
                                    cx,
                                );
                            },
                        ))
                    }),
            )
    }

    fn render_edit_entry(
        &self,
        edit_state: &EditState,
        color: Option<EnvironmentColor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme_colors = cx.theme().colors();
        let text_color = entry_text_color(false);
        let show_editor = edit_state.processing_name.is_none();
        let validation_color_and_message = if show_editor {
            match &edit_state.validation_state {
                ValidationState::Error(message) => Some((Color::Error.color(cx), message.clone())),
                ValidationState::None => None,
            }
        } else {
            None
        };
        let entry_prefix_slot_width = window.rem_size();

        gpui::div()
            .relative()
            .border_1()
            .border_color(gpui::transparent_black())
            .child(
                ListItem::new("variables-panel-edit-entry")
                    .spacing(ListItemSpacing::Dense)
                    .selectable(false)
                    .child(
                        gpui::div()
                            .w(entry_prefix_slot_width)
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(environment_selector::environment_icon(color, text_color)),
                    )
                    .child(match edit_state.processing_name.clone() {
                        Some(name) => gpui::div()
                            .flex()
                            .items_center()
                            .h_6()
                            .min_w_0()
                            .child(Text::new(name).color(text_color).single_line().truncate()),
                        None => {
                            gpui::div()
                                .flex()
                                .items_center()
                                .h_6()
                                .w_full()
                                .mr(Pixels::ZERO - DynamicSpacing::Base06.px(cx) - gpui::px(1.0))
                                .border_1()
                                .border_color(validation_color_and_message.as_ref().map_or(
                                    theme_colors.border_focused.opacity(0.7),
                                    |(color, _)| *color,
                                ))
                                .child(self.environment_name_editor.clone())
                        }
                    })
                    .overflow_x(),
            )
            .when_some(validation_color_and_message, |this, (color, message)| {
                this.child(gpui::deferred(
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
                            Text::new(message)
                                .color(Color::from(color))
                                .size(TextSize::Small),
                        ),
                ))
            })
    }
}

impl Focusable for VariablesPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for VariablesPanel {
    fn persistent_name() -> &'static str {
        "Variables Panel"
    }

    fn panel_key() -> &'static str {
        Self::PANEL_KEY
    }

    fn default_size(&self, _: &Window, _: &App) -> Pixels {
        Self::DEFAULT_SIZE
    }

    fn icon(&self, _: &Window, _: &App) -> Option<IconAsset> {
        Some(IconAsset::SquareInset)
    }

    fn icon_tooltip(&self, _: &Window, _: &App) -> Option<&'static str> {
        Some("Variables Panel")
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        actions::variables_panel::ToggleFocus.boxed_clone()
    }

    fn activation_priority(&self) -> u32 {
        3
    }
}

impl Render for VariablesPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root_worktree = self.project.read(cx).root_worktree(cx);
        let worktree_id = root_worktree
            .as_ref()
            .map(|worktree| worktree.read(cx).id());
        let project_header = root_worktree.map(|worktree| {
            let root_name = worktree
                .read(cx)
                .root_name()
                .as_unix_str()
                .to_ascii_uppercase();
            self.render_header(SharedString::from(root_name), cx)
        });
        let mut entries = worktree_id
            .map(|worktree_id| {
                self.entries(cx)
                    .into_iter()
                    .enumerate()
                    .map(|(index, entry)| self.render_entry(index, entry, worktree_id, window, cx))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
            .into_iter();
        let project_entry = entries.next();
        let environment_entries = entries.collect::<Vec<_>>();
        let edit_entry = self
            .edit_state
            .as_ref()
            .filter(|edit_state| edit_state.is_new_entry())
            .map(|edit_state| self.render_edit_entry(edit_state, None, window, cx));
        let initial_load_completed = self
            .project
            .read(cx)
            .project_config_store()
            .read(cx)
            .initial_load_completed();
        let panel_background = cx.theme().colors().panel_background;
        let entry_prefix_slot_width = window.rem_size();
        let focus_handle = self.focus_handle.clone();

        gpui::div()
            .track_focus(&self.focus_handle)
            .key_context(self.dispatch_context(window, cx))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_first))
            .on_action(cx.listener(Self::select_last))
            .on_action(cx.listener(Self::new_environment))
            .on_action(cx.listener(Self::open))
            .on_action(cx.listener(Self::rename))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::cancel))
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(panel_background)
            .children(project_header)
            .children(project_entry)
            .child(
                self.render_header(SharedString::from("ENVIRONMENTS"), cx)
                    .child(
                        IconButton::new(
                            "variables-panel-new-environment",
                            IconAsset::SquareInsetPlus,
                        )
                        .shape(IconButtonShape::Square)
                        .icon_size(IconSize::Medium)
                        .icon_color(Color::Muted)
                        .tooltip(Tooltip::for_action_title_in(
                            "New Environment",
                            &actions::variables_panel::NewEnvironment,
                            &focus_handle,
                        ))
                        .on_click(cx.listener(
                            |variables_panel, _, window, cx| {
                                variables_panel.new_environment(
                                    &actions::variables_panel::NewEnvironment,
                                    window,
                                    cx,
                                );
                            },
                        )),
                    ),
            )
            .child(
                gpui::div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(
                        gpui::div()
                            .id("variables-panel-environments")
                            .flex()
                            .flex_col()
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll_handle)
                            .when(
                                initial_load_completed
                                    && environment_entries.is_empty()
                                    && edit_entry.is_none(),
                                |this| {
                                    this.child(
                                        ListItem::new("variables-panel-no-environments")
                                            .spacing(ListItemSpacing::Dense)
                                            .selectable(false)
                                            .child(
                                                gpui::div().w(entry_prefix_slot_width).flex_none(),
                                            )
                                            .child(
                                                gpui::div().flex().items_center().h_6().child(
                                                    Text::new("No environments")
                                                        .color(Color::Muted)
                                                        .single_line(),
                                                ),
                                            ),
                                    )
                                },
                            )
                            .children(environment_entries)
                            .children(edit_entry),
                    )
                    .custom_scrollbars(
                        Scrollbars::new(ScrollAxes::Vertical)
                            .tracked_scroll_handle(&self.scroll_handle)
                            .with_track_along(
                                ScrollAxes::Vertical,
                                panel_background,
                                TrackLayout::Overlay,
                            ),
                        window,
                        cx,
                    ),
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

fn is_valid_environment_name(name: &str) -> bool {
    name.starts_with(|character: char| character.is_ascii_alphanumeric())
        && name.ends_with(|character: char| character.is_ascii_alphanumeric())
        && !name.contains("  ")
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | ' ')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    use gpui::{Modifiers, TestAppContext, VisualTestContext};
    use indoc::indoc;
    use serde_json::json;

    use fs::{Fs, RemoveOptions, TempFs};
    use path::rel_path;
    use project_config_editor::ProjectConfigEditor;
    use settings::SettingsStore;
    use theme::LoadThemes;
    use util_macros::path;
    use workspace::{AppState, DockPosition, build_workspace, pane::PaneEvent};
    use worktree::WorktreeModelHandle;

    fn init_test(app_state: Arc<AppState>, cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test_new(cx);
            cx.set_global(settings_store);
            theme::init(LoadThemes::JustBase, cx);
            workspace::init(app_state, cx);
            crate::init(cx);
            editor::init(cx);
            request_editor::init(cx);
            project_config_editor::init(cx);
        });
    }

    fn visible_entries_as_strings(
        panel: &Entity<VariablesPanel>,
        cx: &mut VisualTestContext,
    ) -> Vec<String> {
        panel.read_with(cx, |panel, cx| {
            let edit_entry_name = |edit_state: &EditState| {
                if let Some(name) = &edit_state.processing_name {
                    format!("[PROCESSING: '{name}']")
                } else {
                    let name = panel.environment_name_editor.read(cx).text(cx);
                    format!("[EDITOR: '{name}']")
                }
            };
            let new_entry = panel
                .edit_state
                .as_ref()
                .filter(|edit_state| edit_state.is_new_entry())
                .map(|edit_state| {
                    let name = edit_entry_name(edit_state);
                    let selected = if panel.selection
                        == Some(SelectedEntry::Environment(VariablesPanel::NEW_ENTRY_ID))
                    {
                        "  <== selected"
                    } else {
                        ""
                    };
                    format!("{name}{selected}")
                });

            panel
                .entries(cx)
                .into_iter()
                .map(|entry| {
                    let selected_entry = Some(entry.selected_entry());
                    let edit_state = panel.edit_state.as_ref().filter(|edit_state| {
                        edit_state.entry_id.map(SelectedEntry::Environment) == selected_entry
                    });
                    let name = match (edit_state, entry) {
                        (Some(edit_state), _) => SharedString::from(edit_entry_name(edit_state)),
                        (None, VariablesListEntry::ProjectVariables) => {
                            SharedString::from("Project Variables")
                        }
                        (None, VariablesListEntry::Environment(environment)) => environment.name,
                    };
                    let selected = if panel.selection == selected_entry {
                        "  <== selected"
                    } else {
                        ""
                    };
                    let marked = if panel.active_entry == selected_entry {
                        "  <== marked"
                    } else {
                        ""
                    };

                    format!("{name}{selected}{marked}")
                })
                .chain(new_entry)
                .collect()
        })
    }

    #[gpui::test]
    async fn test_environment_entries(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state, cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                ".zaku": {
                    "environments": {
                        "Dev.toml": indoc! {r#"
                            [meta]
                            version = 1

                            [environment]
                            color = "green"
                            variables = [{ name = "base_url", value = "http://localhost:8000" }]
                        "#},
                        "Staging-9.toml": indoc! {r#"
                            [meta]
                            version = 1

                            [environment]
                            color = "blue"
                            variables = [{ name = "base_url", value = "https://staging-9.api.zaku.dev" }]
                        "#},
                        "Staging-10.toml": indoc! {r#"
                            [meta]
                            version = 1

                            [environment]
                            color = "purple"
                            variables = [{ name = "base_url", value = "https://staging-10.api.zaku.dev" }]
                        "#},
                    },
                },
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs, &project_path, cx).await;
        let (workspace, cx) = build_workspace(&project, cx);
        let panel = workspace.update_in(cx, VariablesPanel::new);
        cx.run_until_parked();

        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables", "Dev", "Staging-9", "Staging-10"]
        );
    }

    #[gpui::test]
    async fn test_select_entries(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state, cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                ".zaku": {
                    "environments": {
                        "Dev.toml": indoc! {r#"
                            [meta]
                            version = 1

                            [environment]
                            color = "green"
                            variables = [{ name = "base_url", value = "http://localhost:8000" }]
                        "#},
                        "Prod.toml": indoc! {r#"
                            [meta]
                            version = 1

                            [environment]
                            color = "red"
                            variables = [{ name = "base_url", value = "https://api.zaku.dev" }]
                        "#},
                    },
                },
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let worktree = cx.update(|cx| project.read(cx).root_worktree(cx).unwrap());
        let (workspace, cx) = build_workspace(&project, cx);
        let panel = workspace.update_in(cx, VariablesPanel::new);
        cx.run_until_parked();

        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables", "Dev", "Prod"]
        );

        panel.update_in(cx, |panel, window, cx| {
            panel.select_next(&actions::menu::SelectNext, window, cx);
        });
        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables  <== selected", "Dev", "Prod"]
        );

        panel.update_in(cx, |panel, window, cx| {
            panel.select_previous(&actions::menu::SelectPrevious, window, cx);
        });
        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables  <== selected", "Dev", "Prod"]
        );

        panel.update_in(cx, |panel, window, cx| {
            panel.select_last(&actions::menu::SelectLast, window, cx);
        });
        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables", "Dev", "Prod  <== selected"]
        );

        panel.update_in(cx, |panel, window, cx| {
            panel.select_next(&actions::menu::SelectNext, window, cx);
        });
        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables", "Dev", "Prod  <== selected"]
        );

        panel.update_in(cx, |panel, window, cx| {
            panel.select_previous(&actions::menu::SelectPrevious, window, cx);
        });
        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables", "Dev  <== selected", "Prod"]
        );

        panel.update_in(cx, |panel, window, cx| {
            panel.select_first(&actions::menu::SelectFirst, window, cx);
        });
        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables  <== selected", "Dev", "Prod"]
        );

        panel.update_in(cx, |panel, window, cx| {
            panel.select_next(&actions::menu::SelectNext, window, cx);
        });
        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables", "Dev  <== selected", "Prod"]
        );

        temp_fs
            .remove_file(
                &project_path.join(path!(".zaku/environments/Dev.toml")),
                RemoveOptions::default(),
            )
            .await
            .unwrap();
        worktree.flush_fs_events(cx).await;
        cx.run_until_parked();

        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables", "Prod"]
        );

        panel.update_in(cx, |panel, window, cx| {
            panel.select_previous(&actions::menu::SelectPrevious, window, cx);
        });
        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables  <== selected", "Prod"]
        );
    }

    #[gpui::test]
    async fn test_file_open_in_project_config_editor(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state, cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                ".zaku": {
                    "environments": {
                        "Dev.toml": indoc! {r#"
                            [meta]
                            version = 1

                            [environment]
                            color = "green"
                            variables = [{ name = "base_url", value = "http://localhost:8000" }]
                        "#},
                    },
                },
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs, &project_path, cx).await;
        let (workspace, cx) = build_workspace(&project, cx);
        let panel = workspace.update_in(cx, VariablesPanel::new);
        let pane = workspace.update_in(cx, |workspace, _, _| workspace.pane().clone());

        cx.run_until_parked();

        panel.update_in(cx, |panel, window, cx| {
            panel.select_first(&actions::menu::SelectFirst, window, cx);
        });
        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables  <== selected", "Dev"]
        );

        panel.update_in(cx, |panel, window, cx| {
            panel.open(&actions::variables_panel::Open, window, cx);
        });
        pane.condition::<PaneEvent>(cx, |pane, cx| {
            pane.active_item()
                .and_then(|item| item.project_path(cx))
                .is_some_and(|project_path| {
                    project_path.path.as_ref() == rel_path(".zaku/project.toml")
                })
        })
        .await;

        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables  <== selected  <== marked", "Dev"]
        );
        let project_variables_editor = workspace.update_in(cx, |workspace, _, cx| {
            workspace.active_item_as::<ProjectConfigEditor>(cx).unwrap()
        });

        panel.update_in(cx, |panel, window, cx| {
            panel.open(&actions::variables_panel::Open, window, cx);
        });
        cx.run_until_parked();

        workspace.update_in(cx, |workspace, _, cx| {
            assert_eq!(
                workspace.active_item_as::<ProjectConfigEditor>(cx),
                Some(project_variables_editor)
            );
        });

        panel.update_in(cx, |panel, window, cx| {
            panel.select_next(&actions::menu::SelectNext, window, cx);
        });
        panel.update_in(cx, |panel, window, cx| {
            panel.open(&actions::variables_panel::Open, window, cx);
        });
        pane.condition::<PaneEvent>(cx, |pane, cx| {
            pane.active_item()
                .and_then(|item| item.project_path(cx))
                .is_some_and(|project_path| {
                    project_path.path.as_ref() == rel_path(".zaku/environments/Dev.toml")
                })
        })
        .await;

        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables", "Dev  <== selected  <== marked"]
        );
        workspace.update_in(cx, |workspace, _, cx| {
            assert!(
                workspace
                    .active_item_as::<ProjectConfigEditor>(cx)
                    .is_some()
            );
        });
        assert_eq!(pane.read_with(cx, |pane, _| pane.items_len()), 1);
    }

    #[gpui::test]
    async fn test_autoreveal_active_entry(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state, cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                ".zaku": {
                    "environments": {
                        "Dev.toml": indoc! {r#"
                            [meta]
                            version = 1

                            [environment]
                            color = "green"
                            variables = [{ name = "base_url", value = "http://localhost:8000" }]
                        "#},
                        "Prod.toml": indoc! {r#"
                            [meta]
                            version = 1

                            [environment]
                            color = "red"
                            variables = [{ name = "base_url", value = "https://api.zaku.dev" }]
                        "#},
                    },
                },
                "foo": {
                    "req1.toml": indoc! {r#"
                        [meta]
                        version = 1

                        [http]
                        method = "GET"
                        url = "https://api.zaku.dev/req1"
                    "#},
                },
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs, &project_path, cx).await;
        let worktree_id = cx.update(|cx| project.read(cx).root_worktree(cx).unwrap().read(cx).id());
        let (workspace, cx) = build_workspace(&project, cx);
        let panel = workspace.update_in(cx, VariablesPanel::new);
        cx.run_until_parked();

        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables", "Dev", "Prod"]
        );

        workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.open_path(
                    ProjectPath::from((worktree_id, rel_path(".zaku/environments/Dev.toml"))),
                    None,
                    true,
                    window,
                    cx,
                )
            })
            .await
            .unwrap();
        cx.run_until_parked();

        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables", "Dev  <== selected  <== marked", "Prod"]
        );

        workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.open_path(
                    ProjectPath::from((worktree_id, rel_path(".zaku/environments/Prod.toml"))),
                    None,
                    true,
                    window,
                    cx,
                )
            })
            .await
            .unwrap();
        cx.run_until_parked();

        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables", "Dev", "Prod  <== selected  <== marked"]
        );

        workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.open_path(
                    ProjectPath::from((worktree_id, rel_path("foo/req1.toml"))),
                    None,
                    true,
                    window,
                    cx,
                )
            })
            .await
            .unwrap();
        cx.run_until_parked();

        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables", "Dev", "Prod  <== selected"]
        );

        let dev_tab_bounds = cx.debug_bounds("TAB-0").unwrap();
        cx.simulate_click(dev_tab_bounds.center(), Modifiers::default());

        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables", "Dev  <== selected  <== marked", "Prod"]
        );

        panel.update_in(cx, |panel, window, cx| {
            panel.select_next(&actions::menu::SelectNext, window, cx);
        });
        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables", "Dev  <== marked", "Prod  <== selected"]
        );
    }

    #[gpui::test]
    async fn test_new_environment(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state, cx);

        temp_fs.insert_tree(path!("project"), json!({}));

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let (workspace, cx) = build_workspace(&project, cx);
        let panel = workspace.update_in(cx, |workspace, window, cx| {
            let panel = VariablesPanel::new(workspace, window, cx);
            workspace.add_panel(panel.clone(), DockPosition::Left, window, cx);
            workspace.open_panel::<VariablesPanel>(window, cx);
            panel
        });
        let pane = workspace.update_in(cx, |workspace, _, _| workspace.pane().clone());
        cx.run_until_parked();

        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables"]
        );

        let new_environment_button_bounds = cx
            .debug_bounds("ICON_BUTTON-variables-panel-new-environment")
            .unwrap();
        cx.simulate_click(new_environment_button_bounds.center(), Modifiers::default());

        panel.update_in(cx, |panel, window, cx| {
            assert!(panel.environment_name_editor.read(cx).is_focused(window));
        });
        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables", "[EDITOR: '']  <== selected"]
        );

        panel
            .update_in(cx, |panel, window, cx| {
                panel.environment_name_editor.update(cx, |editor, cx| {
                    editor.set_text("Dev", cx);
                });
                panel.confirm_edit(true, window, cx).unwrap()
            })
            .await
            .unwrap();
        cx.run_until_parked();
        pane.condition::<PaneEvent>(cx, |pane, cx| {
            pane.active_item()
                .and_then(|item| item.project_path(cx))
                .is_some_and(|project_path| {
                    project_path.path.as_ref() == rel_path(".zaku/environments/Dev.toml")
                })
        })
        .await;

        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables", "Dev  <== selected  <== marked"]
        );
        workspace.update_in(cx, |workspace, _, cx| {
            assert!(
                workspace
                    .active_item_as::<ProjectConfigEditor>(cx)
                    .is_some()
            );
        });
        assert_eq!(
            temp_fs
                .load("project/.zaku/environments/Dev.toml".as_ref())
                .await
                .unwrap(),
            indoc! {"
                [meta]
                version = 1

                [environment]
            "}
        );
    }

    #[gpui::test]
    async fn test_new_environment_validation(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state, cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                ".zaku": {
                    "environments": {
                        "Dev.toml": indoc! {r#"
                            [meta]
                            version = 1

                            [environment]
                            color = "green"
                            variables = [{ name = "base_url", value = "http://localhost:8000" }]
                        "#},
                        "Prod.toml": "[environment",
                    },
                },
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs, &project_path, cx).await;
        let (workspace, cx) = build_workspace(&project, cx);
        let panel = workspace.update_in(cx, VariablesPanel::new);
        cx.run_until_parked();

        panel.update_in(cx, |panel, window, cx| {
            panel.select_last(&actions::menu::SelectLast, window, cx);
        });
        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables", "Dev  <== selected"]
        );

        panel.update_in(cx, |panel, window, cx| {
            panel.new_environment(&actions::variables_panel::NewEnvironment, window, cx);
        });

        panel.update_in(cx, |panel, _, cx| {
            panel.environment_name_editor.update(cx, |editor, cx| {
                editor.set_text("   ", cx);
            });
        });
        panel.read_with(cx, |panel, _| {
            assert_eq!(
                panel.edit_state.as_ref().unwrap().validation_state,
                ValidationState::Error("Environment name must be provided.".to_string())
            );
        });
        assert!(
            panel
                .update_in(cx, |panel, window, cx| panel.confirm_edit(true, window, cx))
                .is_none()
        );

        panel.update_in(cx, |panel, _, cx| {
            panel.environment_name_editor.update(cx, |editor, cx| {
                editor.set_text("Staging/2", cx);
            });
        });
        panel.read_with(cx, |panel, _| {
            assert_eq!(
                panel.edit_state.as_ref().unwrap().validation_state,
                ValidationState::Error(
                    "Environment name must start and end with a letter or digit and contain only letters, digits, '_', '-', '.' and single spaces."
                        .to_string(),
                )
            );
        });
        assert!(
            panel
                .update_in(cx, |panel, window, cx| panel.confirm_edit(true, window, cx))
                .is_none()
        );

        panel.update_in(cx, |panel, _, cx| {
            panel.environment_name_editor.update(cx, |editor, cx| {
                editor.set_text("dev", cx);
            });
        });
        panel.read_with(cx, |panel, _| {
            assert_eq!(
                panel.edit_state.as_ref().unwrap().validation_state,
                ValidationState::Error("Environment 'Dev' already exists.".to_string())
            );
        });
        assert!(
            panel
                .update_in(cx, |panel, window, cx| panel.confirm_edit(true, window, cx))
                .is_none()
        );

        panel.update_in(cx, |panel, _, cx| {
            panel.environment_name_editor.update(cx, |editor, cx| {
                editor.set_text("Prod", cx);
            });
        });
        panel.read_with(cx, |panel, _| {
            assert_eq!(
                panel.edit_state.as_ref().unwrap().validation_state,
                ValidationState::Error("Environment 'Prod' already exists.".to_string())
            );
        });
        assert!(
            panel
                .update_in(cx, |panel, window, cx| panel.confirm_edit(true, window, cx))
                .is_none()
        );

        panel.update_in(cx, |panel, _, cx| {
            panel.environment_name_editor.update(cx, |editor, cx| {
                editor.set_text("Staging", cx);
            });
        });
        panel.read_with(cx, |panel, _| {
            assert_eq!(
                panel.edit_state.as_ref().unwrap().validation_state,
                ValidationState::None
            );
        });

        panel.update_in(cx, |panel, window, cx| {
            panel.cancel(&actions::menu::Cancel, window, cx);
        });
        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables", "Dev  <== selected"]
        );
    }

    #[gpui::test]
    async fn test_rename_environment(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state, cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                ".zaku": {
                    "environments": {
                        "Dev.toml": indoc! {r#"
                            [meta]
                            version = 1

                            [environment]
                            color = "green"
                            variables = [{ name = "base_url", value = "http://localhost:8000" }]
                        "#},
                        "Prod.toml": indoc! {r#"
                            [meta]
                            version = 1

                            [environment]
                            color = "red"
                            variables = [{ name = "base_url", value = "https://api.zaku.dev" }]
                        "#},
                    },
                },
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let (workspace, cx) = build_workspace(&project, cx);
        let panel = workspace.update_in(cx, VariablesPanel::new);
        cx.run_until_parked();

        project.update(cx, |project, cx| {
            project
                .project_config_store()
                .clone()
                .update(cx, |project_config_store, cx| {
                    project_config_store.activate_environment(Some("Dev".to_string()), cx);
                });
        });
        panel.update_in(cx, |panel, window, cx| {
            panel.select_first(&actions::menu::SelectFirst, window, cx);
        });
        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables  <== selected", "Dev", "Prod"]
        );

        panel.update_in(cx, |panel, window, cx| {
            panel.select_next(&actions::menu::SelectNext, window, cx);
        });
        panel.update_in(cx, |panel, window, cx| {
            panel.rename(&actions::variables_panel::Rename, window, cx);
        });

        panel.update_in(cx, |panel, window, cx| {
            assert!(panel.environment_name_editor.read(cx).is_focused(window));
        });
        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables", "[EDITOR: 'Dev']  <== selected", "Prod"]
        );

        let confirm = panel.update_in(cx, |panel, window, cx| {
            panel.environment_name_editor.update(cx, |editor, cx| {
                let name_selections = editor
                    .selections
                    .all::<MultiBufferOffset>(&editor.display_snapshot(cx));
                assert_eq!(
                    name_selections.len(),
                    1,
                    "Name editing should have a single selection, but got: {name_selections:?}"
                );
                let name_selection = &name_selections[0];
                assert_eq!(name_selection.start, MultiBufferOffset(0));
                assert_eq!(name_selection.end, MultiBufferOffset("Dev".len()));

                editor.set_text("Dev 2", cx);
            });
            panel.confirm_edit(true, window, cx).unwrap()
        });
        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &[
                "Project Variables",
                "[PROCESSING: 'Dev 2']  <== selected",
                "Prod",
            ]
        );

        confirm.await.unwrap();
        cx.run_until_parked();
        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables", "Dev 2  <== selected", "Prod"]
        );
        project.read_with(cx, |project, cx| {
            assert_eq!(
                project.project_config_store().read(cx).active_environment(),
                Some("Dev 2")
            );
        });
        assert!(
            temp_fs
                .metadata("project/.zaku/environments/Dev.toml".as_ref())
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            temp_fs
                .load("project/.zaku/environments/Dev 2.toml".as_ref())
                .await
                .unwrap(),
            indoc! {r#"
                [meta]
                version = 1

                [environment]
                color = "green"
                variables = [{ name = "base_url", value = "http://localhost:8000" }]
            "#}
        );
    }

    #[gpui::test]
    async fn test_rename_environment_conflict(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state, cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                ".zaku": {
                    "environments": {
                        "Dev.toml": indoc! {r#"
                            [meta]
                            version = 1

                            [environment]
                            color = "green"
                            variables = [{ name = "base_url", value = "http://localhost:8000" }]
                        "#},
                        "Prod.toml": indoc! {r#"
                            [meta]
                            version = 1

                            [environment]
                            color = "red"
                            variables = [{ name = "base_url", value = "https://api.zaku.dev" }]
                        "#},
                    },
                },
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs, &project_path, cx).await;
        let (workspace, cx) = build_workspace(&project, cx);
        let panel = workspace.update_in(cx, VariablesPanel::new);
        cx.run_until_parked();

        panel.update_in(cx, |panel, window, cx| {
            panel.select_first(&actions::menu::SelectFirst, window, cx);
        });
        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables  <== selected", "Dev", "Prod"]
        );

        panel.update_in(cx, |panel, window, cx| {
            panel.select_next(&actions::menu::SelectNext, window, cx);
        });
        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables", "Dev  <== selected", "Prod"]
        );

        panel.update_in(cx, |panel, window, cx| {
            panel.rename(&actions::variables_panel::Rename, window, cx);
        });
        panel.update_in(cx, |panel, window, cx| {
            assert!(panel.environment_name_editor.read(cx).is_focused(window));
        });
        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables", "[EDITOR: 'Dev']  <== selected", "Prod"]
        );

        panel.update_in(cx, |panel, _, cx| {
            panel.environment_name_editor.update(cx, |editor, cx| {
                editor.set_text("prod", cx);
            });
        });
        panel.read_with(cx, |panel, _| {
            assert_eq!(
                panel.edit_state.as_ref().unwrap().validation_state,
                ValidationState::Error("Environment 'Prod' already exists.".to_string())
            );
        });
        panel.update_in(cx, |panel, window, cx| {
            assert!(
                panel.confirm_edit(true, window, cx).is_none(),
                "Should not allow to confirm on conflicting environment rename"
            );
        });
        panel.read_with(cx, |panel, _| {
            assert!(
                panel.edit_state.is_some(),
                "Edit state should not be None after conflicting environment rename"
            );
        });

        panel.update_in(cx, |panel, _, cx| {
            panel.environment_name_editor.update(cx, |editor, cx| {
                editor.set_text("Dev", cx);
            });
        });
        panel.read_with(cx, |panel, _| {
            assert_eq!(
                panel.edit_state.as_ref().unwrap().validation_state,
                ValidationState::None
            );
        });
        panel.update_in(cx, |panel, window, cx| {
            assert!(
                panel.confirm_edit(true, window, cx).is_none(),
                "Should not rename an environment to its own name"
            );
        });

        panel.update_in(cx, |panel, window, cx| {
            panel.cancel(&actions::menu::Cancel, window, cx);
        });
        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables", "Dev  <== selected", "Prod"]
        );
    }

    #[gpui::test]
    async fn test_rename_survives_window_deactivation(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state, cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                ".zaku": {
                    "environments": {
                        "Dev.toml": indoc! {r#"
                            [meta]
                            version = 1

                            [environment]
                            color = "green"
                            variables = [{ name = "base_url", value = "http://localhost:8000" }]
                        "#},
                    },
                },
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs, &project_path, cx).await;
        let (workspace, cx) = build_workspace(&project, cx);
        let panel = workspace.update_in(cx, |workspace, window, cx| {
            let panel = VariablesPanel::new(workspace, window, cx);
            workspace.add_panel(panel.clone(), DockPosition::Left, window, cx);
            workspace.open_panel::<VariablesPanel>(window, cx);
            panel
        });
        cx.run_until_parked();

        panel.update_in(cx, |panel, window, cx| {
            panel.select_last(&actions::menu::SelectLast, window, cx);
        });
        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables", "Dev  <== selected"]
        );

        panel.update_in(cx, |panel, window, cx| {
            panel.rename(&actions::variables_panel::Rename, window, cx);
        });
        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables", "[EDITOR: 'Dev']  <== selected"]
        );
        panel.update_in(cx, |panel, window, cx| {
            assert!(
                panel.environment_name_editor.read(cx).is_focused(window),
                "Name editor should be focused, otherwise deactivating the window blurs nothing"
            );
        });

        cx.deactivate_window();

        assert_eq!(
            visible_entries_as_strings(&panel, cx),
            &["Project Variables", "[EDITOR: 'Dev']  <== selected"]
        );
    }

    #[test]
    fn test_is_valid_environment_name() {
        assert!(is_valid_environment_name("Dev"));
        assert!(is_valid_environment_name("Staging 2"));
        assert!(is_valid_environment_name("Staging-10"));
        assert!(is_valid_environment_name("Prod_EU"));
        assert!(is_valid_environment_name("Prod.EU"));

        assert!(!is_valid_environment_name(""));
        assert!(!is_valid_environment_name(" Dev"));
        assert!(!is_valid_environment_name("Staging-"));
        assert!(!is_valid_environment_name("Staging  2"));
        assert!(!is_valid_environment_name("Staging/2"));
        assert!(!is_valid_environment_name("Dév"));
    }
}
