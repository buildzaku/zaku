use fuzzy_nucleo::{Case, LengthPenalty, StringMatch, StringMatchCandidate};
use gpui::{
    AnyElement, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
    SharedString, Subscription, Task, TaskExt, WeakEntity, Window, prelude::*,
};
use std::{iter, sync::Arc};

use picker::{Picker, PickerDelegate};
use project::{EnvironmentColor, Project};
use ui::{
    ActiveTheme, Color, DynamicSpacing, HighlightedText, Icon, IconAsset, IconSize, ListItem,
    ListItemSpacing, ListSubHeader, Text, TextCommon, TextSize, Toggleable,
};
use workspace::{ModalView, Workspace, WorkspaceDb};

pub fn init(cx: &mut App) {
    cx.observe_new(EnvironmentSelector::register).detach();
}

pub fn environment_icon(environment_color: Option<EnvironmentColor>, text_color: Color) -> Icon {
    let Some(environment_color) = environment_color else {
        return Icon::new(IconAsset::CircleDashed)
            .size(IconSize::XSmall)
            .color(text_color);
    };

    Icon::new(IconAsset::Circle)
        .size(IconSize::XSmall)
        .color(match environment_color {
            EnvironmentColor::Accent => Color::Accent,
            EnvironmentColor::Info => Color::Info,
            EnvironmentColor::Success => Color::Success,
            EnvironmentColor::Warning => Color::Warning,
            EnvironmentColor::Error => Color::Error,
            EnvironmentColor::Hint => Color::Hint,
        })
}

pub struct EnvironmentSelector {
    picker: Entity<Picker<EnvironmentSelectorDelegate>>,
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
        rem_width: f32,
        is_popover: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let picker = cx.new(|cx| {
            Picker::list(delegate, window, cx)
                .list_measure_all()
                .initial_width(gpui::rems(rem_width))
                .height(gpui::rems(24.0))
                .no_vertical_padding()
        });

        let mut dismiss_subscriptions =
            vec![cx.subscribe(&picker, |_, _, _: &DismissEvent, cx| cx.emit(DismissEvent))];
        if is_popover {
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
        let workspace_handle = workspace.weak_handle();
        let project = workspace.project().clone();

        workspace.toggle_modal(window, cx, |window, cx| {
            let delegate = EnvironmentSelectorDelegate::new(workspace_handle, project, cx);
            Self::new(delegate, 34.0, false, window, cx)
        });
    }

    pub fn popover(
        workspace: WeakEntity<Workspace>,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| {
            let delegate = EnvironmentSelectorDelegate::new(workspace, project, cx);
            let environment_selector = Self::new(delegate, 20.0, true, window, cx);
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
    color: Option<EnvironmentColor>,
    is_missing: bool,
}

pub struct EnvironmentSelectorDelegate {
    workspace: WeakEntity<Workspace>,
    project: Entity<Project>,
    environments: Vec<Environment>,
    matches: Vec<StringMatch>,
    selected_index: usize,
}

impl EnvironmentSelectorDelegate {
    fn new(workspace: WeakEntity<Workspace>, project: Entity<Project>, cx: &App) -> Self {
        let project_config_store = project.read(cx).project_config_store().read(cx);
        let mut environment_files = project_config_store.environments().collect::<Vec<_>>();
        environment_files.sort_by(|(left, _), (right, _)| path::natural_sort(left, right));
        let missing_environment = project_config_store
            .active_environment()
            .filter(|_| project_config_store.is_active_environment_missing())
            .map(|name| Environment {
                name: Some(name.to_string()),
                color: None,
                is_missing: true,
            });
        let environments = missing_environment
            .into_iter()
            .chain(iter::once(Environment {
                name: None,
                color: None,
                is_missing: false,
            }))
            .chain(
                environment_files
                    .into_iter()
                    .map(|(name, environment_file)| Environment {
                        name: Some(name.to_string()),
                        color: environment_file.environment.color,
                        is_missing: false,
                    }),
            )
            .collect();

        Self {
            workspace,
            project,
            environments,
            matches: Vec::new(),
            selected_index: 0,
        }
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
                Case::smart_if_uppercase_in(query),
                LengthPenalty::On,
                100,
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
        if let Some(workspace_id) = self
            .workspace
            .upgrade()
            .and_then(|workspace| workspace.read(cx).database_id())
        {
            let workspace_db = WorkspaceDb::global(cx);
            let name = name.clone();
            cx.background_spawn(async move {
                workspace_db
                    .set_active_environment(workspace_id, name)
                    .await
            })
            .detach_and_log_err(cx);
        }
        cx.emit(DismissEvent);
    }

    fn dismissed(&mut self, _: &mut Window, _: &mut Context<Picker<Self>>) {}

    fn render_match(
        &self,
        index: usize,
        selected: bool,
        _: &mut Window,
        _: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        let string_match = self.matches.get(index)?;
        let environment = self.environments.get(string_match.candidate_id)?;

        Some(
            ListItem::new(index)
                .inset(true)
                .spacing(ListItemSpacing::Sparse)
                .toggle_state(selected)
                .start_slot(if environment.is_missing {
                    Icon::new(IconAsset::Warning)
                        .size(IconSize::XSmall)
                        .color(Color::Warning)
                } else {
                    environment_icon(environment.color, Color::Default)
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
}
