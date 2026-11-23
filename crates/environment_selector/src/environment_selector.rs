use fuzzy_nucleo::{MaxResults, StringMatch, StringMatchCandidate, StringMatchOptions};
use gpui::{
    Action, AnyElement, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
    Rems, SharedString, Subscription, Task, Window, prelude::*,
};
use std::{iter, sync::Arc};

use picker::{Picker, PickerDelegate};
use project::{EnvironmentColor, Project, ProjectEvent, ProjectPath};
use ui::{
    ActiveTheme, Button, ButtonCommon, ButtonSize, Clickable, Color, DynamicSpacing,
    HighlightedText, Icon, IconAsset, IconSize, KeyBinding, ListItem, ListItemSpacing,
    ListSubHeader, Text, TextCommon, TextSize, Toggleable,
};
use workspace::{ModalView, Workspace};

pub fn init(cx: &mut App) {
    cx.observe_new(EnvironmentSelector::register).detach();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EnvironmentSelectorStyle {
    Modal,
    Popover,
}

pub struct EnvironmentSelector {
    pub picker: Entity<Picker<EnvironmentSelectorDelegate>>,
    _dismiss_subscriptions: Vec<Subscription>,
}

impl EnvironmentSelector {
    fn register(workspace: &mut Workspace, _: Option<&mut Window>, _: &mut Context<Workspace>) {
        workspace.register_action(
            |workspace, _: &actions::environment_selector::Toggle, window, cx| {
                Self::toggle(workspace, window, cx);
            },
        );
    }

    fn new(
        delegate: EnvironmentSelectorDelegate,
        width: Rems,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let style = delegate.style;
        let picker = cx.new(|cx| {
            Picker::list(delegate, window, cx)
                .list_measure_all()
                .initial_width(width)
                .height(gpui::rems(24.0))
                .no_vertical_padding()
        });

        let mut dismiss_subscriptions =
            vec![cx.subscribe(&picker, |_, _, _: &DismissEvent, cx| cx.emit(DismissEvent))];
        if style == EnvironmentSelectorStyle::Popover {
            let picker_focus = picker.focus_handle(cx);
            dismiss_subscriptions
                .push(cx.on_focus_out(&picker_focus, window, |_, _, _, cx| cx.emit(DismissEvent)));
        }

        Self {
            picker,
            _dismiss_subscriptions: dismiss_subscriptions,
        }
    }

    fn toggle(workspace: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
        let project = workspace.project().clone();

        workspace.toggle_modal(window, cx, |window, cx| {
            let delegate =
                EnvironmentSelectorDelegate::new(project, EnvironmentSelectorStyle::Modal, cx);
            Self::new(delegate, gpui::rems(34.0), window, cx)
        });
    }

    pub fn popover(project: Entity<Project>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            let delegate =
                EnvironmentSelectorDelegate::new(project, EnvironmentSelectorStyle::Popover, cx);
            let environment_selector = Self::new(delegate, gpui::rems(20.0), window, cx);
            environment_selector
                .picker
                .focus_handle(cx)
                .focus(window, cx);
            environment_selector
        })
    }
}

impl ModalView for EnvironmentSelector {}

impl EventEmitter<DismissEvent> for EnvironmentSelector {}

impl Focusable for EnvironmentSelector {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl Render for EnvironmentSelector {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui::div()
            .flex()
            .flex_col()
            .key_context("EnvironmentSelector")
            .child(self.picker.clone())
    }
}

struct Environment {
    name: Option<String>,
    project_path: Option<ProjectPath>,
    color: Option<EnvironmentColor>,
    is_missing: bool,
}

pub struct EnvironmentSelectorDelegate {
    project: Entity<Project>,
    environments: Vec<Environment>,
    matches: Vec<StringMatch>,
    selected_index: usize,
    style: EnvironmentSelectorStyle,
}

impl EnvironmentSelectorDelegate {
    fn new(
        project: Entity<Project>,
        style: EnvironmentSelectorStyle,
        cx: &mut Context<EnvironmentSelector>,
    ) -> Self {
        let worktree_id = project
            .read(cx)
            .root_worktree(cx)
            .map(|worktree| worktree.read(cx).id());
        let environment_entries = project
            .read(cx)
            .project_config_store()
            .read(cx)
            .environment_entries(cx)
            .cloned()
            .collect::<Vec<_>>();
        for entry in &environment_entries {
            project.update(cx, |project, cx| project.load_entry_metadata(entry, cx));
        }

        cx.subscribe(
            &project,
            |environment_selector, _, event: &ProjectEvent, cx| match event {
                ProjectEvent::EntryMetadataUpdated(_) => {
                    environment_selector.picker.update(cx, |_, cx| cx.notify());
                }
                ProjectEvent::ActiveEntryChanged(_)
                | ProjectEvent::WorktreeAdded(_)
                | ProjectEvent::WorktreeRemoved(_)
                | ProjectEvent::WorktreeUpdatedEntries(_, _)
                | ProjectEvent::DeletedEntry(_, _)
                | ProjectEvent::Toast { .. }
                | ProjectEvent::HideToast { .. } => {}
            },
        )
        .detach();

        let project_config_store = project.read(cx).project_config_store().read(cx);
        let mut environment_files = environment_entries
            .iter()
            .filter_map(|entry| {
                let project_path = ProjectPath {
                    worktree_id: worktree_id?,
                    path: entry.path.clone(),
                };
                Some((entry.path.file_stem()?, project_path))
            })
            .collect::<Vec<_>>();
        environment_files.sort_by(|(left, _), (right, _)| path::natural_sort(left, right));
        let missing_environment = project_config_store
            .active_environment()
            .filter(|_| project_config_store.is_active_environment_missing(cx))
            .map(|name| Environment {
                name: Some(name.to_string()),
                project_path: None,
                color: None,
                is_missing: true,
            });
        let environments = missing_environment
            .into_iter()
            .chain(iter::once(Environment {
                name: None,
                project_path: None,
                color: None,
                is_missing: false,
            }))
            .chain(environment_files.into_iter().map(|(name, project_path)| {
                Environment {
                    name: Some(name.to_string()),
                    project_path: Some(project_path),
                    color: project_config_store
                        .environment_file(name)
                        .and_then(|environment_file| environment_file.environment.color),
                    is_missing: false,
                }
            }))
            .collect();

        Self {
            project,
            environments,
            matches: Vec::new(),
            selected_index: 0,
            style,
        }
    }

    #[cfg(any(test, feature = "test"))]
    pub fn matched_environments(&self) -> Vec<(Option<String>, bool)> {
        self.matches
            .iter()
            .filter_map(|string_match| self.environments.get(string_match.candidate_id))
            .map(|environment| (environment.name.clone(), environment.is_missing))
            .collect()
    }
}

impl PickerDelegate for EnvironmentSelectorDelegate {
    type ListItem = ListItem;

    fn name() -> &'static str {
        "environment selector"
    }

    fn placeholder_text(&self, _: &mut Window, _: &mut App) -> Arc<str> {
        "Search environments…".into()
    }

    fn match_count(&self) -> usize {
        self.matches.len()
    }

    fn selected_index(&self) -> usize {
        self.selected_index
    }

    fn set_selected_index(&mut self, index: usize, _: &mut Window, _: &mut Context<Picker<Self>>) {
        self.selected_index = index;
    }

    fn update_matches(
        &mut self,
        query: String,
        _: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        let query = query.trim_start();
        if query.is_empty() {
            self.matches = (0..self.environments.len())
                .map(|candidate_id| StringMatch {
                    candidate_id,
                    score: 0.0,
                    positions: Vec::new(),
                    string: SharedString::default(),
                })
                .collect();
            let active_environment = self
                .project
                .read(cx)
                .project_config_store()
                .read(cx)
                .active_environment();
            self.selected_index = self
                .environments
                .iter()
                .position(|environment| environment.name.as_deref() == active_environment)
                .unwrap_or(0);
        } else {
            let candidates = self
                .environments
                .iter()
                .enumerate()
                .map(|(candidate_id, environment)| {
                    StringMatchCandidate::new(
                        candidate_id,
                        environment
                            .name
                            .clone()
                            .unwrap_or_else(|| "No Environment".to_string()),
                    )
                })
                .collect::<Vec<_>>();
            self.matches = fuzzy_nucleo::match_strings(
                &candidates,
                query,
                StringMatchOptions {
                    max_results: MaxResults::limit::<100>(),
                    ..Default::default()
                },
            );
            self.selected_index = 0;
        }

        Task::ready(())
    }

    fn confirm(&mut self, _: bool, _: &mut Window, cx: &mut Context<Picker<Self>>) {
        let Some(selected_match) = self.matches.get(self.selected_index) else {
            return;
        };
        let Some(Environment { name, .. }) = self.environments.get(selected_match.candidate_id)
        else {
            return;
        };

        self.project.read(cx).project_config_store().clone().update(
            cx,
            |project_config_store, cx| {
                project_config_store.activate_environment(name.clone(), cx);
            },
        );
        cx.emit(DismissEvent);
    }

    fn dismissed(&mut self, _: &mut Window, _: &mut Context<Picker<Self>>) {}

    fn render_match(
        &self,
        index: usize,
        selected: bool,
        _: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        let string_match = self.matches.get(index)?;
        let environment = self.environments.get(string_match.candidate_id)?;
        let project = self.project.read(cx);
        let is_invalid = environment
            .project_path
            .as_ref()
            .and_then(|project_path| project.entry_for_path(project_path, cx))
            .and_then(|entry| project.entry_metadata(entry))
            .is_some_and(|metadata| metadata.is_invalid);
        let active_environment = project.project_config_store().read(cx).active_environment();
        let is_active =
            !environment.is_missing && environment.name.as_deref() == active_environment;

        Some(
            ListItem::new(index)
                .inset(true)
                .spacing(ListItemSpacing::Sparse)
                .toggle_state(selected)
                .start_slot(if environment.is_missing {
                    Icon::new(IconAsset::Warning)
                        .size(IconSize::XSmall)
                        .color(Color::Warning)
                } else if environment.name.is_none() {
                    Icon::new(IconAsset::SquareDashed)
                        .size(IconSize::XSmall)
                        .color(Color::Default)
                } else if is_invalid {
                    Icon::new(IconAsset::WarningCircle)
                        .size(IconSize::Small)
                        .color(Color::Error)
                } else {
                    editor::environment_icon(environment.color, Color::Default)
                })
                .child(
                    HighlightedText::new(
                        environment
                            .name
                            .clone()
                            .unwrap_or_else(|| "No Environment".to_string()),
                        string_match.positions.clone(),
                    )
                    .single_line()
                    .truncate(),
                )
                .when(environment.is_missing, |this| {
                    this.end_slot(
                        Text::new("Not found")
                            .size(TextSize::Small)
                            .color(Color::Muted),
                    )
                })
                .when(is_active, |this| {
                    this.end_slot(
                        Icon::new(IconAsset::Check)
                            .size(IconSize::XSmall)
                            .color(Color::Accent),
                    )
                }),
        )
    }

    fn render_header(&self, _: &mut Window, cx: &mut Context<Picker<Self>>) -> Option<AnyElement> {
        let theme_colors = cx.theme().colors();

        Some(
            gpui::div()
                .flex_none()
                .pt(DynamicSpacing::Base04.rems(cx) * 2.0)
                .bg(theme_colors.elevated_surface_background)
                .child(ListSubHeader::new("Environments").inset(true))
                .into_any_element(),
        )
    }

    fn render_footer(&self, _: &mut Window, cx: &mut Context<Picker<Self>>) -> Option<AnyElement> {
        let theme_colors = cx.theme().colors();

        Some(
            gpui::div()
                .flex()
                .items_center()
                .justify_end()
                .w_full()
                .flex_none()
                .p_1p5()
                .border_t_1()
                .border_color(theme_colors.border_variant)
                .child(
                    Button::new("manage-environments", "Manage Environments")
                        .size(ButtonSize::Medium)
                        .key_binding(
                            KeyBinding::for_action(&actions::variables_panel::ToggleFocus, cx)
                                .size(ui::rems_from_px(12.0_f32)),
                        )
                        .on_click(cx.listener(|_, _, window, cx| {
                            window.dispatch_action(
                                actions::variables_panel::ToggleFocus.boxed_clone(),
                                cx,
                            );
                            cx.emit(DismissEvent);
                        })),
                )
                .into_any_element(),
        )
    }
}
