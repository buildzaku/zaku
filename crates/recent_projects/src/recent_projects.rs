use fuzzy_nucleo::{Case, LengthPenalty, StringMatch, StringMatchCandidate};
use gpui::{
    AnyElement, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
    SharedString, Subscription, Task, TaskExt, WeakEntity, Window, prelude::*,
};
use std::sync::Arc;

use fs::Fs;
use path::PathExt;
use picker::{Picker, PickerDelegate};
use ui::{
    ActiveTheme, DynamicSpacing, HighlightedText, ListItem, ListItemSpacing, ListSubHeader,
    TextCommon, Toggleable, Tooltip,
};
use workspace::{
    OpenMode, RecentWorkspace, Workspace, WorkspaceDb, notifications::DetachAndPromptErr,
};

pub struct RecentProjects {
    pub picker: Entity<Picker<RecentProjectsDelegate>>,
    _dismiss_subscriptions: Vec<Subscription>,
}

impl RecentProjects {
    fn new(
        delegate: RecentProjectsDelegate,
        fs: Option<Arc<dyn Fs>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let picker = cx.new(|cx| {
            Picker::list(delegate, window, cx)
                .list_measure_all()
                .initial_width(gpui::rems(20.0))
                .minimum_results_width(gpui::rems(20.0))
                .height(gpui::rems(24.0))
                .no_vertical_padding()
        });

        let picker_focus = picker.focus_handle(cx);
        let dismiss_subscriptions = vec![
            cx.subscribe(&picker, |_, _, _: &DismissEvent, cx| cx.emit(DismissEvent)),
            cx.on_focus_out(&picker_focus, window, |_, _, _, cx| cx.emit(DismissEvent)),
        ];

        let db = WorkspaceDb::global(cx);
        cx.spawn_in(window, async move |this, cx| {
            let Some(fs) = fs else {
                return anyhow::Ok(());
            };
            let workspaces = db.recent_project_workspaces(fs.as_ref()).await?;
            this.update_in(cx, move |this, window, cx| {
                this.picker.update(cx, move |picker, cx| {
                    picker.delegate.set_workspaces(workspaces);
                    picker.update_matches(picker.query(cx), window, cx);
                });
            })?;
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);

        Self {
            picker,
            _dismiss_subscriptions: dismiss_subscriptions,
        }
    }

    pub fn popover(
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        let fs = workspace
            .upgrade()
            .map(|workspace| workspace.read(cx).app_state().fs.clone());

        cx.new(|cx| {
            let delegate = RecentProjectsDelegate::new(workspace);
            let list = Self::new(delegate, fs, window, cx);
            list.picker.focus_handle(cx).focus(window, cx);
            list
        })
    }
}

impl EventEmitter<DismissEvent> for RecentProjects {}

impl Focusable for RecentProjects {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl Render for RecentProjects {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui::div()
            .flex()
            .flex_col()
            .key_context("RecentProjects")
            .child(self.picker.clone())
    }
}

pub struct RecentProjectsDelegate {
    workspace: WeakEntity<Workspace>,
    workspaces: Vec<RecentWorkspace>,
    matches: Vec<StringMatch>,
    selected_index: usize,
}

impl RecentProjectsDelegate {
    fn new(workspace: WeakEntity<Workspace>) -> Self {
        Self {
            workspace,
            workspaces: Vec::new(),
            matches: Vec::new(),
            selected_index: 0,
        }
    }

    pub fn set_workspaces(&mut self, workspaces: Vec<RecentWorkspace>) {
        self.workspaces = workspaces;
    }
}

impl PickerDelegate for RecentProjectsDelegate {
    type ListItem = AnyElement;

    fn name() -> &'static str {
        "recent projects"
    }

    fn placeholder_text(&self, _: &mut Window, _: &mut App) -> Arc<str> {
        "Search projects…".into()
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
        let current_workspace_id = self
            .workspace
            .upgrade()
            .and_then(|workspace| workspace.read(cx).database_id());
        let recent_candidates = self
            .workspaces
            .iter()
            .enumerate()
            .filter(|(_, workspace)| Some(workspace.workspace_id) != current_workspace_id)
            .map(|(id, workspace)| {
                StringMatchCandidate::new(
                    id,
                    workspace.location.compact().to_string_lossy().into_owned(),
                )
            })
            .collect::<Vec<_>>();

        self.matches = if query.is_empty() {
            recent_candidates
                .into_iter()
                .map(|candidate| StringMatch {
                    candidate_id: candidate.id,
                    score: 0.0,
                    positions: Vec::new(),
                    string: SharedString::default(),
                })
                .collect()
        } else {
            fuzzy_nucleo::match_strings(
                &recent_candidates,
                query,
                Case::smart_if_uppercase_in(query),
                LengthPenalty::On,
                100,
            )
        };

        self.selected_index = 0;
        Task::ready(())
    }

    fn confirm(&mut self, _: bool, window: &mut Window, cx: &mut Context<Picker<Self>>) {
        let Some(selected_match) = self.matches.get(self.selected_index) else {
            return;
        };
        let Some(recent_workspace) = self.workspaces.get(selected_match.candidate_id) else {
            return;
        };
        if let Some(workspace) = self.workspace.upgrade() {
            workspace.update(cx, |workspace, cx| {
                workspace
                    .open_workspace_for_path(
                        recent_workspace.location.clone(),
                        OpenMode::Activate,
                        window,
                        cx,
                    )
                    .detach_and_prompt_err("Failed to open project", window, cx, |_, _, _| None);
            });
        }
        cx.emit(DismissEvent);
    }

    fn dismissed(&mut self, _: &mut Window, _: &mut Context<Picker<Self>>) {}

    fn no_matches_text(&self, _: &mut Window, _: &mut App) -> Option<SharedString> {
        Some(if self.workspaces.is_empty() {
            "Recently opened projects will show up here".into()
        } else {
            "No matches".into()
        })
    }

    fn render_match(
        &self,
        index: usize,
        selected: bool,
        _: &mut Window,
        _: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        let hit = self.matches.get(index)?;
        let workspace = self.workspaces.get(hit.candidate_id)?;
        let path = workspace.location.compact();
        let path_string = path.to_string_lossy().into_owned();
        let name = path.file_name().map_or_else(
            || path_string.clone(),
            |name| name.to_string_lossy().into_owned(),
        );
        let name_start_byte = path_string.len() - name.len();
        let positions = hit
            .positions
            .iter()
            .copied()
            .skip_while(|position| *position < name_start_byte)
            .take_while(|position| *position < path_string.len())
            .map(|position| position - name_start_byte)
            .collect();

        Some(
            ListItem::new(index)
                .inset(true)
                .toggle_state(selected)
                .spacing(ListItemSpacing::Sparse)
                .child(
                    gpui::div()
                        .id("project-info-container")
                        .flex()
                        .items_center()
                        .w_full()
                        .min_w_0()
                        .flex_grow_1()
                        .child(
                            HighlightedText::new(name, positions)
                                .single_line()
                                .truncate(),
                        )
                        .tooltip(move |_, cx| {
                            Tooltip::with_meta(
                                "Open Project in This Window",
                                None,
                                ui::utils::replace_control_characters(&path_string).into_owned(),
                                cx,
                            )
                        }),
                )
                .into_any_element(),
        )
    }

    fn render_header(&self, _: &mut Window, cx: &mut Context<Picker<Self>>) -> Option<AnyElement> {
        let theme_colors = cx.theme().colors();

        Some(
            gpui::div()
                .flex_none()
                .pt(DynamicSpacing::Base04.rems(cx))
                .bg(theme_colors.panel_tab_bar_background)
                .border_b_1()
                .border_color(theme_colors.border_variant)
                .child(ListSubHeader::new("Recent Projects").inset(true))
                .into_any_element(),
        )
    }
}
