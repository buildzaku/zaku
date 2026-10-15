use gpui::{
    Action, App, ClickEvent, Context, Div, Entity, FocusHandle, Focusable, KeyContext, Pixels,
    ScrollHandle, SharedString, Stateful, WeakEntity, Window, prelude::*,
};
use std::{iter, sync::Arc};

use editor::items::{entry_git_aware_text_color, entry_text_color, git_status_indicator};
use path::RelPath;
use project::{
    ConfigFileType, EnvironmentColor, Project, ProjectPath, WorktreeId,
    git_store::{GitStoreEvent, RepositoryEvent},
    project_config_store::ProjectConfigStoreEvent,
};
use settings::{GitSettings, Settings};
use theme::ActiveTheme;
use ui::{
    Color, DynamicSpacing, Icon, IconAsset, IconSize, ListItem, ListItemSpacing, ScrollAxes,
    Scrollbars, Text, TextCommon, TextSize, TrackLayout, WithScrollbar,
};
use util::ResultExt;
use workspace::{Panel, Workspace, WorkspaceEvent};

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

struct EnvironmentEntry {
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
            Self::Environment(environment) => {
                let name = &environment.name;
                path::project_environments_folder_relative_path().join(
                    RelPath::from_unix_str(&format!("{name}.toml"))
                        .expect("environment file name should be a valid path"),
                )
            }
        }
    }
}

pub struct VariablesPanel {
    focus_handle: FocusHandle,
    project: Entity<Project>,
    workspace: WeakEntity<Workspace>,
    scroll_handle: ScrollHandle,
    selection: Option<Arc<RelPath>>,
    active_item_path: Option<ProjectPath>,
}

impl VariablesPanel {
    const PANEL_KEY: &str = "VariablesPanel";
    const DEFAULT_SIZE: Pixels = gpui::px(250.0);

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
        let active_item_path = workspace
            .active_item(cx)
            .and_then(|item| item.project_path(cx));

        cx.new(|cx| {
            cx.subscribe_in(
                &workspace_entity,
                window,
                |this: &mut VariablesPanel, workspace, event, _, cx| match event {
                    WorkspaceEvent::ActiveItemChanged => {
                        this.active_item_path = workspace
                            .read(cx)
                            .active_item(cx)
                            .and_then(|item| item.project_path(cx));
                        if let Some(project_path) = this.active_item_path.as_ref()
                            && ConfigFileType::for_path(&project_path.path).is_some()
                        {
                            this.selection = Some(project_path.path.clone());
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
                active_item_path,
            }
        })
    }

    fn entries(&self, cx: &App) -> Vec<VariablesListEntry> {
        let mut environments = self
            .project
            .read(cx)
            .project_config_store()
            .read(cx)
            .environments()
            .map(|(name, environment_file)| EnvironmentEntry {
                name: SharedString::from(name.to_string()),
                color: environment_file.environment.color,
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
        let selection = self.selection.as_ref()?;
        self.entries(cx)
            .iter()
            .position(|entry| entry.path() == *selection)
    }

    fn dispatch_context(&self) -> KeyContext {
        let mut dispatch_context = KeyContext::new_with_defaults();
        dispatch_context.add(Self::PANEL_KEY);
        dispatch_context.add("menu");
        dispatch_context
    }

    fn autoscroll(&mut self, cx: &mut Context<Self>) {
        let Some(selection) = self.selection.as_ref() else {
            return;
        };
        if let Some(index) = self
            .entries(cx)
            .iter()
            .filter(|entry| matches!(entry, VariablesListEntry::Environment(_)))
            .position(|entry| entry.path() == *selection)
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

            self.selection = Some(entry.path());
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

            self.selection = Some(entry.path());
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
            self.selection = Some(entry.path());
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
            self.selection = Some(entry.path());
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
        let Some(selection) = self.selection.clone() else {
            return;
        };
        if self.index_for_selection(cx).is_none() {
            return;
        }

        self.open_entry(selection, false, true, window, cx);
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
        let theme_colors = cx.theme().colors();
        let path = entry.path();
        let project_path = ProjectPath {
            worktree_id,
            path: path.clone(),
        };
        let is_marked = self.active_item_path.as_ref() == Some(&project_path);
        let is_selected = self.selection.as_ref() == Some(&path);
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
        let (icon, name) = match entry {
            VariablesListEntry::ProjectVariables => (
                Icon::new(IconAsset::SquareInset)
                    .size(IconSize::XSmall)
                    .color(text_color),
                SharedString::from("Project Variables"),
            ),
            VariablesListEntry::Environment(environment) => (
                environment_selector::environment_icon(environment.color, text_color),
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
                    variables_panel.selection = Some(path.clone());
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
                    ),
            )
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
        let initial_load_completed = self
            .project
            .read(cx)
            .project_config_store()
            .read(cx)
            .initial_load_completed();
        let panel_background = cx.theme().colors().panel_background;
        let entry_prefix_slot_width = window.rem_size();

        gpui::div()
            .track_focus(&self.focus_handle)
            .key_context(self.dispatch_context())
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_first))
            .on_action(cx.listener(Self::select_last))
            .on_action(cx.listener(Self::open))
            .flex()
            .flex_col()
            .size_full()
            .bg(panel_background)
            .children(project_header)
            .children(project_entry)
            .child(self.render_header(SharedString::from("ENVIRONMENTS"), cx))
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
                                initial_load_completed && environment_entries.is_empty(),
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
                            .children(environment_entries),
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
    }
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
    use workspace::{AppState, build_workspace, pane::PaneEvent};
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
            let worktree = panel.project.read(cx).root_worktree(cx).unwrap();
            let worktree_id = worktree.read(cx).id();

            panel
                .entries(cx)
                .into_iter()
                .map(|entry| {
                    let path = entry.path();
                    let project_path = ProjectPath {
                        worktree_id,
                        path: path.clone(),
                    };
                    let name = match entry {
                        VariablesListEntry::ProjectVariables => {
                            SharedString::from("Project Variables")
                        }
                        VariablesListEntry::Environment(environment) => environment.name,
                    };
                    let selected = if panel.selection.as_ref() == Some(&path) {
                        "  <== selected"
                    } else {
                        ""
                    };
                    let marked = if panel.active_item_path.as_ref() == Some(&project_path) {
                        "  <== marked"
                    } else {
                        ""
                    };

                    format!("{name}{selected}{marked}")
                })
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
}
