use fuzzy_nucleo::StringMatchCandidate;
use gpui::{
    Action, Anchor, AnyElement, AnyView, App, Context, DismissEvent, Div, Entity, EventEmitter,
    FocusHandle, Focusable, Global, Modifiers, ModifiersChangedEvent, PromptLevel, Rems,
    SharedString, Subscription, Task, WeakEntity, Window, prelude::*,
};
use jiff::Timestamp;
use std::sync::Arc;

use collections::HashSet;
use git::{SHORT_SHA_LENGTH, normalize_branch_name, repository::Branch};
use input::ErasedEditor;
use picker::{Picker, PickerDelegate};
use project::git_store::{Repository, RepositoryEvent};
use time_format::TimestampFormat;
use ui::{
    ActiveTheme, Banner, Button, ButtonCommon, ButtonSize, ButtonVariant, Clickable, Color,
    ContextMenu, HighlightedText, Icon, IconAsset, IconButton, IconPosition, IconSize, Indicator,
    KeyBinding, ListItem, ListItemSpacing, ListSubHeader, PopoverMenu, PopoverMenuHandle, Severity,
    Text, TextCommon, TextSize, Toggleable, Tooltip, VisibleOnHover,
};
use util::ResultExt;
use workspace::{
    ModalView, Toast, Workspace,
    notifications::{DetachAndPromptErr, NotificationId},
};

use crate::git_timestamp;

pub fn register(workspace: &mut Workspace) {
    workspace.register_action(open);
    workspace.register_action(switch);
    workspace.register_action(checkout_branch);
}

pub fn checkout_branch(
    workspace: &mut Workspace,
    _: &actions::git::CheckoutBranch,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    open(workspace, &actions::git::Branch, window, cx);
}

pub fn switch(
    workspace: &mut Workspace,
    _: &actions::git::Switch,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    open(workspace, &actions::git::Branch, window, cx);
}

pub fn open(
    workspace: &mut Workspace,
    _: &actions::git::Branch,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let workspace_handle = workspace.weak_handle();
    let repository = workspace
        .project()
        .read(cx)
        .git_store()
        .read(cx)
        .active_repository();

    workspace.toggle_modal(window, cx, |window, cx| {
        BranchList::new(
            workspace_handle,
            repository,
            BranchListStyle::Modal,
            gpui::rems(34.0),
            window,
            cx,
        )
    });
}

pub fn popover(
    workspace: WeakEntity<Workspace>,
    repository: Option<Entity<Repository>>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<BranchList> {
    cx.new(|cx| {
        let list = BranchList::new(
            workspace,
            repository,
            BranchListStyle::Popover,
            gpui::rems(20.0),
            window,
            cx,
        );
        list.focus_handle(cx).focus(window, cx);
        list
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BranchListStyle {
    Modal,
    Popover,
}

pub struct BranchList {
    pub picker: Entity<Picker<BranchListDelegate>>,
    picker_focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
    _load_default_branch: Option<Task<()>>,
}

impl BranchList {
    fn new(
        workspace: WeakEntity<Workspace>,
        repository: Option<Entity<Repository>>,
        style: BranchListStyle,
        width: Rems,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let all_branches = repository
            .as_ref()
            .map(|repository| process_branches(&repository.read(cx).branch_list))
            .unwrap_or_default();
        let branch_list_error = repository
            .as_ref()
            .and_then(|repository| repository.read(cx).branch_list_error.clone());

        let default_branch_request = repository
            .as_ref()
            .map(|repository| repository.update(cx, |repository, _| repository.default_branch()));

        let repository_subscription = repository.as_ref().map(|repository| {
            cx.subscribe_in(
                repository,
                window,
                move |this, repository, event, window, cx| {
                    if matches!(event, RepositoryEvent::BranchListChanged) {
                        let repository = repository.read(cx);
                        let branch_list = repository.branch_list.clone();
                        let branch_list_error = repository.branch_list_error.clone();
                        this.picker.update(cx, |picker, cx| {
                            picker.delegate.restore_selected_branch = picker
                                .delegate
                                .matches
                                .get(picker.delegate.selected_index)
                                .and_then(|entry| {
                                    entry.as_branch().map(|branch| branch.ref_name.clone())
                                });
                            picker.delegate.all_branches = process_branches(&branch_list);
                            picker.delegate.branch_list_error = branch_list_error;
                            picker.refresh(window, cx);
                        });
                    }
                },
            )
        });

        let mut delegate = BranchListDelegate::new(workspace, repository, style, cx);
        delegate.all_branches = all_branches;
        delegate.branch_list_error = branch_list_error;

        let picker = cx.new(|cx| {
            Picker::list(delegate, window, cx)
                .list_measure_all()
                .initial_width(width)
                .height(gpui::rems(24.0))
                .no_vertical_padding()
        });
        let picker_focus_handle = picker.focus_handle(cx);

        picker.update(cx, |picker, _| {
            picker.delegate.focus_handle = picker_focus_handle.clone();
        });

        let mut subscriptions = vec![cx.subscribe(&picker, |this, _, _: &DismissEvent, cx| {
            if !this.branch_filter_menu_open(cx) {
                cx.emit(DismissEvent);
            }
        })];
        if style == BranchListStyle::Popover {
            subscriptions.push(
                cx.on_focus_out(&picker_focus_handle, window, |this, _, _, cx| {
                    if !this.branch_filter_menu_open(cx) {
                        cx.emit(DismissEvent);
                    }
                }),
            );
        }
        subscriptions.extend(repository_subscription);

        let load_default_branch = default_branch_request.map(|default_branch_request| {
            cx.spawn(async move |this, cx| {
                let default_branch = match default_branch_request.await {
                    Ok(default_branch) => default_branch.log_err().flatten(),
                    Err(error) => {
                        log::error!("Failed to load default branch: {error}");
                        None
                    }
                };
                this.update(cx, |this, cx| {
                    this.picker.update(cx, |picker, _| {
                        picker.delegate.default_branch = default_branch;
                    });
                })
                .log_err();
            })
        });

        Self {
            picker,
            picker_focus_handle,
            _subscriptions: subscriptions,
            _load_default_branch: load_default_branch,
        }
    }

    fn handle_modifiers_changed(
        &mut self,
        event: &ModifiersChangedEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.picker.update(cx, |picker, cx| {
            picker.delegate.modifiers = event.modifiers;
            cx.notify();
        });
    }

    fn handle_delete(
        &mut self,
        _: &actions::branch_picker::DeleteBranch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.picker.update(cx, |picker, cx| {
            picker
                .delegate
                .delete_at(picker.delegate.selected_index, false, window, cx);
        });
    }

    fn handle_force_delete(
        &mut self,
        _: &actions::branch_picker::ForceDeleteBranch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.picker.update(cx, |picker, cx| {
            picker
                .delegate
                .delete_at(picker.delegate.selected_index, true, window, cx);
        });
    }

    fn set_branch_filter(
        &mut self,
        branch_filter: BranchFilter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.set_global(GlobalBranchFilter(branch_filter));
        self.picker.update(cx, |picker, cx| {
            if picker.delegate.branch_filter == branch_filter {
                return;
            }
            picker.delegate.branch_filter = branch_filter;
            picker.update_matches(picker.query(cx), window, cx);
        });
    }

    fn branch_filter_menu_open(&self, cx: &App) -> bool {
        self.picker
            .read(cx)
            .delegate
            .branch_filter_menu_handle
            .is_deployed()
    }

    fn cycle_branch_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let branch_filter = self.picker.read(cx).delegate.branch_filter.next();
        self.set_branch_filter(branch_filter, window, cx);
    }
}

impl ModalView for BranchList {}

impl EventEmitter<DismissEvent> for BranchList {}

impl Focusable for BranchList {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.picker_focus_handle.clone()
    }
}

impl Render for BranchList {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        gpui::div()
            .flex()
            .flex_col()
            .key_context("GitBranchSelector")
            .on_modifiers_changed(cx.listener(Self::handle_modifiers_changed))
            .on_action(cx.listener(Self::handle_delete))
            .on_action(cx.listener(Self::handle_force_delete))
            .on_action(cx.listener(
                |this, _: &actions::branch_picker::ShowAllBranches, window, cx| {
                    this.set_branch_filter(BranchFilter::All, window, cx);
                },
            ))
            .on_action(cx.listener(
                |this, _: &actions::branch_picker::ShowLocalBranches, window, cx| {
                    this.set_branch_filter(BranchFilter::Local, window, cx);
                },
            ))
            .on_action(cx.listener(
                |this, _: &actions::branch_picker::ShowRemoteBranches, window, cx| {
                    this.set_branch_filter(BranchFilter::Remote, window, cx);
                },
            ))
            .on_action(cx.listener(
                |this, _: &actions::branch_picker::CycleBranchFilter, window, cx| {
                    this.cycle_branch_filter(window, cx);
                },
            ))
            .on_action(cx.listener(
                |this, _: &actions::branch_picker::ToggleFilterMenu, window, cx| {
                    let menu_handle = this
                        .picker
                        .read(cx)
                        .delegate
                        .branch_filter_menu_handle
                        .clone();
                    menu_handle.toggle(window, cx);
                },
            ))
            .child(self.picker.clone())
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Entry {
    Branch {
        branch: Branch,
        positions: Vec<usize>,
    },
    NewBranch {
        name: String,
    },
}

impl Entry {
    fn as_branch(&self) -> Option<&Branch> {
        match self {
            Entry::Branch { branch, .. } => Some(branch),
            Entry::NewBranch { .. } => None,
        }
    }

    #[cfg(any(test, feature = "test"))]
    fn name(&self) -> &str {
        match self {
            Entry::Branch { branch, .. } => branch.name(),
            Entry::NewBranch { name } => name.as_str(),
        }
    }

    #[cfg(any(test, feature = "test"))]
    fn is_new_branch(&self) -> bool {
        matches!(self, Entry::NewBranch { .. })
    }
}

#[derive(Clone, Copy, PartialEq)]
enum BranchFilter {
    All,
    Local,
    Remote,
}

impl BranchFilter {
    fn next(self) -> Self {
        match self {
            Self::All => Self::Local,
            Self::Local => Self::Remote,
            Self::Remote => Self::All,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::All => "All Branches",
            Self::Local => "Local Branches",
            Self::Remote => "Remote Branches",
        }
    }
}

struct GlobalBranchFilter(BranchFilter);

impl Global for GlobalBranchFilter {}

fn branch_filter_menu(
    branch_filter: BranchFilter,
    focus_handle: &FocusHandle,
    window: &mut Window,
    cx: &mut App,
) -> Entity<ContextMenu> {
    ContextMenu::build(window, cx, |mut menu, _, _| {
        menu = menu.context(focus_handle.clone());

        let filter_actions: [(BranchFilter, Box<dyn Action>); 3] = [
            (
                BranchFilter::All,
                Box::new(actions::branch_picker::ShowAllBranches),
            ),
            (
                BranchFilter::Local,
                Box::new(actions::branch_picker::ShowLocalBranches),
            ),
            (
                BranchFilter::Remote,
                Box::new(actions::branch_picker::ShowRemoteBranches),
            ),
        ];

        for (filter, action) in filter_actions {
            let handler_focus = focus_handle.clone();
            let dispatched = action.boxed_clone();
            menu = menu.toggleable_entry(
                filter.label(),
                filter == branch_filter,
                IconPosition::End,
                Some(action),
                move |window, cx| {
                    window.focus(&handler_focus, cx);
                    window.dispatch_action(dispatched.boxed_clone(), cx);
                },
            );
        }
        menu
    })
}

#[derive(Debug)]
enum PickerState {
    List,
    NewBranch,
}

struct DeleteBranchErrorToast;

struct DeleteBranchTooltip {
    picker: WeakEntity<Picker<BranchListDelegate>>,
    focus_handle: FocusHandle,
    delete_index: usize,
    is_remote: bool,
    _subscription: Subscription,
}

impl DeleteBranchTooltip {
    fn new(
        picker: &Entity<Picker<BranchListDelegate>>,
        focus_handle: FocusHandle,
        delete_index: usize,
        is_remote: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscription = cx.observe(picker, |_, _, cx| cx.notify());
        Self {
            picker: picker.downgrade(),
            focus_handle,
            delete_index,
            is_remote,
            _subscription: subscription,
        }
    }
}

impl Render for DeleteBranchTooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.is_remote {
            return Tooltip::with_meta_in(
                "Delete Local Tracking Branch",
                Some(&actions::branch_picker::DeleteBranch),
                "Does not delete the branch on the remote",
                &self.focus_handle,
                cx,
            );
        }

        let force_delete = self
            .picker
            .read_with(cx, |picker, _| {
                picker
                    .delegate
                    .is_force_delete_hovering_index(self.delete_index)
            })
            .unwrap_or(false);
        if force_delete {
            Tooltip::for_action_in(
                "Force Delete Branch",
                &actions::branch_picker::ForceDeleteBranch,
                &self.focus_handle,
                cx,
            )
        } else {
            Tooltip::with_meta_in(
                "Delete Branch",
                Some(&actions::branch_picker::DeleteBranch),
                if cfg!(target_os = "macos") {
                    "Hold option to force delete"
                } else {
                    "Hold alt to force delete"
                },
                &self.focus_handle,
                cx,
            )
        }
    }
}

fn sort_branch_entries(matches: &mut [Entry]) {
    matches.sort_by_key(|entry| entry.as_branch().is_some_and(Branch::is_remote));
}

fn process_branches(branches: &Arc<[Branch]>) -> Vec<Branch> {
    let remote_upstreams: HashSet<_> = branches
        .iter()
        .filter_map(|branch| {
            branch
                .upstream
                .as_ref()
                .filter(|upstream| upstream.is_remote())
                .map(|upstream| upstream.ref_name.clone())
        })
        .collect();

    let mut result: Vec<Branch> = branches
        .iter()
        .filter(|branch| !remote_upstreams.contains(&branch.ref_name))
        .cloned()
        .collect();

    result.sort_by_key(|branch| {
        (
            !branch.is_head,
            branch
                .most_recent_commit
                .as_ref()
                .map(|commit| 0 - commit.commit_timestamp),
        )
    });

    result
}

pub struct BranchListDelegate {
    workspace: WeakEntity<Workspace>,
    matches: Vec<Entry>,
    all_branches: Vec<Branch>,
    branch_list_error: Option<SharedString>,
    default_branch: Option<SharedString>,
    repository: Option<Entity<Repository>>,
    style: BranchListStyle,
    selected_index: usize,
    modifiers: Modifiers,
    branch_filter: BranchFilter,
    branch_filter_menu_handle: PopoverMenuHandle<ContextMenu>,
    state: PickerState,
    focus_handle: FocusHandle,
    restore_selected_branch: Option<SharedString>,
    hovered_delete_index: Option<usize>,
}

impl BranchListDelegate {
    fn new(
        workspace: WeakEntity<Workspace>,
        repository: Option<Entity<Repository>>,
        style: BranchListStyle,
        cx: &mut Context<BranchList>,
    ) -> Self {
        let branch_filter = cx
            .try_global::<GlobalBranchFilter>()
            .map_or(BranchFilter::All, |filter| filter.0);

        Self {
            workspace,
            matches: Vec::new(),
            all_branches: Vec::new(),
            branch_list_error: None,
            default_branch: None,
            repository,
            style,
            selected_index: 0,
            modifiers: Modifiers::default(),
            branch_filter,
            branch_filter_menu_handle: PopoverMenuHandle::default(),
            state: PickerState::List,
            focus_handle: cx.focus_handle(),
            restore_selected_branch: None,
            hovered_delete_index: None,
        }
    }

    fn branch_filter_trigger(&self) -> IconButton {
        IconButton::new("branch-filter", IconAsset::Filter)
            .icon_size(IconSize::Small)
            .toggle_state(self.branch_filter != BranchFilter::All)
            .when(self.branch_filter != BranchFilter::All, |this| {
                this.indicator(
                    Indicator::dot()
                        .color(Color::Info)
                        .size(ui::rems_from_px(6.0_f32)),
                )
            })
    }

    fn branch_filter_tooltip(&self) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
        let focus_handle = self.focus_handle.clone();
        move |_, cx| {
            Tooltip::for_action_in(
                "Filter Branches",
                &actions::branch_picker::ToggleFilterMenu,
                &focus_handle,
                cx,
            )
        }
    }

    fn is_force_delete_hovering_index(&self, index: usize) -> bool {
        self.modifiers.alt && self.hovered_delete_index == Some(index)
    }

    fn create_branch(
        &self,
        from_branch: Option<SharedString>,
        new_branch_name: &str,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) {
        let Some(repository) = self.repository.clone() else {
            return;
        };
        let new_branch_name = normalize_branch_name(new_branch_name);
        let base_branch = from_branch.map(|branch| branch.to_string());
        cx.spawn(async move |_, cx| {
            repository
                .update(cx, |repository, _| {
                    repository.create_branch(new_branch_name, base_branch)
                })
                .await??;

            anyhow::Ok(())
        })
        .detach_and_prompt_err("Failed to create branch", window, cx, |error, _, _| {
            Some(error.to_string())
        });
        cx.emit(DismissEvent);
    }

    fn delete_at(
        &self,
        index: usize,
        force: bool,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) {
        let Some(entry) = self.matches.get(index).cloned() else {
            return;
        };
        let Some(repository) = self.repository.clone() else {
            return;
        };

        let workspace = self.workspace.clone();

        cx.spawn_in(window, async move |picker, cx| {
            let Entry::Branch { branch, .. } = &entry else {
                log::error!("Failed to delete entry: wrong entry to delete");
                return Ok(());
            };

            if branch.is_head {
                return Ok(());
            }

            let is_remote = branch.is_remote();
            let branch_name = branch.name().to_string();
            let initial_result = repository
                .update(cx, |repository, _| {
                    repository.delete_branch(is_remote, branch_name.clone(), force)
                })
                .await?;

            let result = match initial_result {
                Ok(()) => Ok(()),
                Err(error) => {
                    if is_remote {
                        log::error!("Failed to delete remote branch: {error}");
                    } else {
                        log::error!("Failed to delete branch: {error}");
                    }

                    if !force
                        && error
                            .to_string()
                            .to_lowercase()
                            .contains("not fully merged")
                    {
                        let answer = cx.update(|window, cx| {
                            window.prompt(
                                PromptLevel::Warning,
                                &format!(
                                    "Branch \"{branch_name}\" is not fully merged. Force delete it?"
                                ),
                                None,
                                &["Force Delete", "Cancel"],
                                cx,
                            )
                        })?;

                        if answer.await != Ok(0) {
                            return Ok(());
                        }

                        let retry = repository
                            .update(cx, |repository, _| {
                                repository.delete_branch(is_remote, branch_name, true)
                            })
                            .await?;

                        if let Err(error) = &retry {
                            log::error!("Failed to force delete branch: {error}");
                        }
                        retry
                    } else {
                        Err(error)
                    }
                }
            };

            if result.is_err() {
                workspace.update(cx, |workspace, cx| {
                    workspace.show_toast(
                        Toast::new(
                            NotificationId::unique::<DeleteBranchErrorToast>(),
                            "Failed to delete branch.",
                        )
                        .on_click("Open Logs", |window, cx| {
                            window.dispatch_action(Box::new(actions::zaku::OpenLogs), cx);
                        }),
                        cx,
                    );
                })?;

                return Ok(());
            }

            picker.update(cx, |picker, cx| {
                picker
                    .delegate
                    .matches
                    .retain(|match_entry| match_entry != &entry);
                picker
                    .delegate
                    .all_branches
                    .retain(|existing_branch| existing_branch.ref_name != branch.ref_name);

                if picker.delegate.matches.is_empty() {
                    picker.delegate.selected_index = 0;
                } else if picker.delegate.selected_index >= picker.delegate.matches.len() {
                    picker.delegate.selected_index = picker.delegate.matches.len() - 1;
                }

                picker.delegate.hovered_delete_index = None;

                cx.notify();
            })?;

            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    #[cfg(any(test, feature = "test"))]
    pub fn matched_entries(&self) -> Vec<(String, bool)> {
        self.matches
            .iter()
            .map(|entry| (entry.name().to_string(), entry.is_new_branch()))
            .collect()
    }
}

impl PickerDelegate for BranchListDelegate {
    type ListItem = AnyElement;

    fn name() -> &'static str {
        "branch picker"
    }

    fn placeholder_text(&self, _: &mut Window, _: &mut App) -> Arc<str> {
        "Switch or type to create a branch…".into()
    }

    fn no_matches_text(&self, _: &mut Window, _: &mut App) -> Option<SharedString> {
        None
    }

    fn render_editor(
        &self,
        editor: &Arc<dyn ErasedEditor>,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Div {
        let warning_banner = self.branch_list_error.as_deref().map(|error| {
            let message = format!("Some branches could not be loaded: {error}");
            gpui::div().p_1p5().child(
                Banner::new()
                    .severity(Severity::Warning)
                    .child(gpui::div().min_w_0().flex_1().child(Text::new(message))),
            )
        });
        let branch_filter = self.branch_filter;
        let focus_handle = self.focus_handle.clone();

        gpui::div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .child(
                gpui::div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_1()
                    .overflow_hidden()
                    .h_9()
                    .px_2p5()
                    .border_b_1()
                    .border_color(cx.theme().colors().border_variant)
                    .child(gpui::div().flex_1().child(editor.render(window, cx)))
                    .child(
                        PopoverMenu::new("branch-filter-menu")
                            .with_handle(self.branch_filter_menu_handle.clone())
                            .trigger_with_tooltip(
                                self.branch_filter_trigger(),
                                self.branch_filter_tooltip(),
                            )
                            .menu(move |window, cx| {
                                Some(branch_filter_menu(branch_filter, &focus_handle, window, cx))
                            })
                            .anchor(Anchor::TopRight)
                            .attach(Anchor::BottomRight)
                            .offset(gpui::point(gpui::px(1.0), gpui::px(1.0))),
                    ),
            )
            .children(warning_banner)
    }

    fn render_header(&self, _: &mut Window, _: &mut Context<Picker<Self>>) -> Option<AnyElement> {
        if self.branch_filter == BranchFilter::All
            || !self
                .matches
                .first()
                .is_some_and(|entry| entry.as_branch().is_some())
        {
            return None;
        }

        Some(
            gpui::div()
                .pt_1p5()
                .mb_neg_0p5()
                .child(ListSubHeader::new(self.branch_filter.label()).inset(true))
                .into_any_element(),
        )
    }

    fn has_another_open_menu(&self, window: &Window, cx: &App) -> bool {
        self.branch_filter_menu_handle.is_deployed()
            || self.branch_filter_menu_handle.is_focused(window, cx)
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
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        let all_branches = self.all_branches.clone();
        let branch_filter = self.branch_filter;

        cx.spawn_in(window, async move |picker, cx| {
            let branch_matches_filter = |branch: &Branch| match branch_filter {
                BranchFilter::All => true,
                BranchFilter::Local => !branch.is_remote(),
                BranchFilter::Remote => branch.is_remote(),
            };

            let mut matches: Vec<Entry> = if query.is_empty() {
                all_branches
                    .iter()
                    .filter(|branch| branch_matches_filter(branch))
                    .map(|branch| Entry::Branch {
                        branch: branch.clone(),
                        positions: Vec::new(),
                    })
                    .collect()
            } else {
                let branches = all_branches
                    .iter()
                    .filter(|branch| branch_matches_filter(branch))
                    .collect::<Vec<_>>();
                let candidates = branches
                    .iter()
                    .enumerate()
                    .map(|(index, branch)| StringMatchCandidate::new(index, branch.name()))
                    .collect::<Vec<StringMatchCandidate>>();
                fuzzy_nucleo::match_strings_async(
                    &candidates,
                    &query,
                    fuzzy_nucleo::StringMatchOptions {
                        max_results: fuzzy_nucleo::MaxResults::limit::<10_000>(),
                        ..Default::default()
                    },
                    cx.background_executor().clone(),
                )
                .await
                .into_iter()
                .filter_map(|candidate| {
                    Some(Entry::Branch {
                        branch: (*branches.get(candidate.candidate_id)?).clone(),
                        positions: candidate.positions,
                    })
                })
                .collect()
            };
            sort_branch_entries(&mut matches);

            picker
                .update(cx, |picker, _| {
                    if !query.is_empty()
                        && !all_branches.iter().any(|branch| branch.name() == query)
                    {
                        let query = normalize_branch_name(&query);
                        picker.delegate.state = if matches.is_empty() {
                            PickerState::NewBranch
                        } else {
                            PickerState::List
                        };
                        matches.push(Entry::NewBranch { name: query });
                    } else {
                        picker.delegate.state = PickerState::List;
                    }
                    let delegate = &mut picker.delegate;
                    delegate.matches = matches;
                    if delegate.matches.is_empty() {
                        delegate.selected_index = 0;
                    } else if let Some(ref_name) = delegate.restore_selected_branch.take() {
                        delegate.selected_index = delegate
                            .matches
                            .iter()
                            .position(|entry| {
                                entry.as_branch().is_some_and(|branch| {
                                    branch.ref_name == ref_name
                                        || branch.name() == ref_name.as_ref()
                                })
                            })
                            .unwrap_or(0);
                    } else {
                        delegate.selected_index =
                            delegate.selected_index.min(delegate.matches.len() - 1);
                    }
                })
                .log_err();
        })
    }

    fn confirm(&mut self, secondary: bool, window: &mut Window, cx: &mut Context<Picker<Self>>) {
        let Some(entry) = self.matches.get(self.selected_index()) else {
            return;
        };

        match entry {
            Entry::Branch { branch, .. } => {
                let current_branch = self.repository.as_ref().and_then(|repository| {
                    repository
                        .read(cx)
                        .branch
                        .as_ref()
                        .map(|branch| branch.ref_name.clone())
                });

                if current_branch.is_some_and(|current_branch| current_branch == branch.ref_name) {
                    cx.emit(DismissEvent);
                    return;
                }

                let Some(repository) = self.repository.clone() else {
                    return;
                };

                let branch = branch.clone();
                cx.spawn(async move |_, cx| {
                    repository
                        .update(cx, |repository, _| {
                            repository.change_branch(branch.name().to_string())
                        })
                        .await??;

                    anyhow::Ok(())
                })
                .detach_and_prompt_err(
                    "Failed to change branch",
                    window,
                    cx,
                    |_, _, _| None,
                );
            }
            Entry::NewBranch { name } => {
                let from_branch = if secondary {
                    self.default_branch.clone()
                } else {
                    None
                };
                self.create_branch(from_branch, name, window, cx);
            }
        }

        cx.emit(DismissEvent);
    }

    fn dismissed(&mut self, _: &mut Window, cx: &mut Context<Picker<Self>>) {
        self.state = PickerState::List;
        cx.emit(DismissEvent);
    }

    fn render_match(
        &self,
        index: usize,
        selected: bool,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        let entry = self.matches.get(index)?;

        let most_recent_commit = entry
            .as_branch()
            .and_then(|branch| branch.most_recent_commit.as_ref());
        let commit_timestamp = most_recent_commit
            .and_then(|commit| Timestamp::from_second(commit.commit_timestamp).log_err());
        let commit_time = commit_timestamp.map(|timestamp| {
            git_timestamp::format_git_timestamp(timestamp, TimestampFormat::Relative)
        });
        let absolute_time = commit_timestamp.map(|timestamp| {
            git_timestamp::format_git_timestamp(timestamp, TimestampFormat::EnhancedAbsolute)
        });
        let author_name = most_recent_commit.map(|commit| commit.author_name.clone());
        let short_sha = most_recent_commit.map(|commit| {
            commit
                .sha
                .chars()
                .take(SHORT_SHA_LENGTH)
                .collect::<String>()
        });
        let subject = most_recent_commit.map(|commit| commit.subject.clone());

        let is_head_branch = entry.as_branch().is_some_and(|branch| branch.is_head);

        let entry_icon = match entry {
            Entry::NewBranch { .. } => IconAsset::Plus,
            Entry::Branch { branch, .. } => {
                if branch.is_remote() {
                    IconAsset::Server
                } else {
                    IconAsset::GitBranch
                }
            }
        };

        let entry_title = match entry {
            Entry::NewBranch { name } => Text::new(format!("Create Branch: \"{name}\"…"))
                .single_line()
                .truncate()
                .into_any_element(),
            Entry::Branch { branch, positions } => gpui::div()
                .flex()
                .items_center()
                .min_w_0()
                .gap_1p5()
                .child(
                    HighlightedText::new(branch.name().to_string(), positions.clone())
                        .single_line()
                        .truncate(),
                )
                .when_some(commit_time, |this, commit_time| {
                    this.child(
                        gpui::div().flex_none().child(
                            Text::new(commit_time)
                                .color(Color::Muted)
                                .size(TextSize::Small),
                        ),
                    )
                })
                .into_any_element(),
        };

        let theme_colors = cx.theme().colors();
        let button_variant = ButtonVariant::Custom {
            background: gpui::transparent_black(),
            foreground: theme_colors.button_secondary_foreground,
            hover_background: theme_colors.text.opacity(0.15),
            border: gpui::transparent_black(),
        };

        let create_from_default_button = self
            .default_branch
            .as_ref()
            .filter(|_| matches!(entry, Entry::NewBranch { .. }))
            .map(|default_branch| {
                let tooltip_label: SharedString =
                    format!("Create New From: {default_branch}").into();
                let focus_handle = self.focus_handle.clone();

                gpui::div()
                    .flex()
                    .items_center()
                    .when(!selected, |this| this.visible_on_hover("list-item"))
                    .child(
                        IconButton::new("create-from-default", IconAsset::GitBranchPlus)
                            .icon_size(IconSize::Small)
                            .variant(button_variant)
                            .tooltip(move |_, cx| {
                                Tooltip::for_action_in(
                                    tooltip_label.clone(),
                                    &actions::menu::SecondaryConfirm,
                                    &focus_handle,
                                    cx,
                                )
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.delegate.confirm(true, window, cx);
                            })),
                    )
            });

        let delete_button = entry
            .as_branch()
            .filter(|branch| !branch.is_head)
            .map(|branch| {
                let is_remote = branch.is_remote();
                let picker = cx.entity();
                let focus_handle = self.focus_handle.clone();
                let force_delete = !is_remote && self.is_force_delete_hovering_index(index);

                gpui::div()
                    .id(("delete-hover", index))
                    .flex()
                    .items_center()
                    .when(!selected, |this| this.visible_on_hover("list-item"))
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        if *hovered {
                            this.delegate.hovered_delete_index = Some(index);
                        } else if this.delegate.hovered_delete_index == Some(index) {
                            this.delegate.hovered_delete_index = None;
                        }
                        cx.notify();
                    }))
                    .child(
                        IconButton::new(("delete", index), IconAsset::Trash)
                            .icon_size(IconSize::Small)
                            .variant(button_variant)
                            .when(force_delete, |this| this.icon_color(Color::Error))
                            .tooltip(move |_, cx| {
                                cx.new(|cx| {
                                    DeleteBranchTooltip::new(
                                        &picker,
                                        focus_handle.clone(),
                                        index,
                                        is_remote,
                                        cx,
                                    )
                                })
                                .into()
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.delegate.delete_at(
                                    index,
                                    this.delegate.modifiers.alt,
                                    window,
                                    cx,
                                );
                            })),
                    )
            });

        let message = match entry {
            Entry::NewBranch { .. } => Text::new(
                self.repository
                    .as_ref()
                    .and_then(|repository| {
                        repository
                            .read(cx)
                            .branch
                            .as_ref()
                            .map(|branch| format!("Based off {}", branch.name()))
                    })
                    .unwrap_or_else(|| "Based off the current branch".to_string()),
            )
            .size(TextSize::Small)
            .color(Color::Muted)
            .truncate()
            .into_any_element(),
            Entry::Branch { .. } => {
                let has_author = author_name.is_some();
                let has_commit = most_recent_commit.is_some();
                let dot = || {
                    Text::new("•")
                        .alpha(0.5)
                        .color(Color::Muted)
                        .size(TextSize::Small)
                };

                gpui::div()
                    .flex()
                    .items_center()
                    .w_full()
                    .min_w_0()
                    .gap_1p5()
                    .when_some(author_name, |this, author_name| {
                        this.child(
                            Text::new(author_name)
                                .color(Color::Muted)
                                .size(TextSize::Small),
                        )
                    })
                    .when_some(short_sha, |this, short_sha| {
                        this.when(has_author, |this| this.child(dot())).child(
                            Text::new(short_sha)
                                .color(Color::Muted)
                                .size(TextSize::Small),
                        )
                    })
                    .when_some(subject, |this, subject| {
                        this.when(has_commit, |this| this.child(dot())).child(
                            gpui::div().flex_1().min_w_0().child(
                                Text::new(subject)
                                    .color(Color::Muted)
                                    .size(TextSize::Small)
                                    .truncate(),
                            ),
                        )
                    })
                    .when(!has_commit, |this| {
                        this.child(
                            Text::new("No commits found")
                                .color(Color::Muted)
                                .size(TextSize::Small),
                        )
                    })
                    .into_any_element()
            }
        };

        let list_item = ListItem::new(index)
            .inset(true)
            .spacing(ListItemSpacing::Sparse)
            .toggle_state(selected)
            .child(
                gpui::div()
                    .flex()
                    .items_center()
                    .w_full()
                    .min_w_0()
                    .gap_2p5()
                    .flex_grow_1()
                    .child(
                        Icon::new(entry_icon)
                            .color(Color::Muted)
                            .size(IconSize::Small),
                    )
                    .child(
                        gpui::div()
                            .id("info_container")
                            .flex()
                            .flex_col()
                            .w_full()
                            .min_w_0()
                            .child(entry_title)
                            .child(message)
                            .when_some(
                                entry.as_branch().map(|branch| branch.name().to_string()),
                                |this, branch_name| {
                                    this.tooltip(Tooltip::element(move |_, _| {
                                        gpui::div()
                                            .flex()
                                            .flex_col()
                                            .child(Text::new(branch_name.clone()))
                                            .when(is_head_branch, |this| {
                                                this.child(
                                                    Text::new("Current Branch")
                                                        .size(TextSize::Small)
                                                        .color(Color::Muted),
                                                )
                                            })
                                            .when_some(absolute_time.clone(), |this, time| {
                                                this.child(
                                                    Text::new(time)
                                                        .size(TextSize::Small)
                                                        .color(Color::Muted),
                                                )
                                            })
                                            .into_any_element()
                                    }))
                                },
                            ),
                    ),
            )
            .when_some(
                create_from_default_button,
                |this, create_from_default_button| this.end_slot(create_from_default_button),
            )
            .when_some(delete_button, |this, delete_button| {
                this.end_slot(delete_button)
            })
            .when(is_head_branch, |this| {
                this.end_slot(
                    gpui::div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(IconSize::Small.square(window, cx))
                        .child(
                            Icon::new(IconAsset::Check)
                                .size(IconSize::XSmall)
                                .color(Color::Accent),
                        ),
                )
            });

        let section_header = (self.branch_filter == BranchFilter::All)
            .then(|| entry.as_branch())
            .flatten()
            .and_then(|branch| {
                let starts_section = index == 0
                    || self
                        .matches
                        .get(index - 1)
                        .and_then(Entry::as_branch)
                        .is_none_or(|previous_branch| {
                            previous_branch.is_remote() != branch.is_remote()
                        });
                starts_section.then(|| {
                    if branch.is_remote() {
                        ("Remote Branches", index != 0)
                    } else {
                        ("Local Branches", false)
                    }
                })
            });

        Some(
            gpui::div()
                .flex()
                .flex_col()
                .w_full()
                .when_some(section_header, |this, (section_header, show_divider)| {
                    this.pt_1p5()
                        .when(show_divider, |this| {
                            this.mt_1()
                                .border_t_1()
                                .border_color(cx.theme().colors().border_variant)
                        })
                        .child(ListSubHeader::new(section_header).inset(true))
                })
                .child(list_item)
                .into_any_element(),
        )
    }

    fn render_footer(&self, _: &mut Window, cx: &mut Context<Picker<Self>>) -> Option<AnyElement> {
        if self.style == BranchListStyle::Popover {
            return None;
        }
        let focus_handle = self.focus_handle.clone();
        let theme_colors = cx.theme().colors();

        let footer_container = gpui::div()
            .flex()
            .items_center()
            .justify_end()
            .w_full()
            .flex_none()
            .gap_1()
            .p_1p5()
            .border_t_1()
            .border_color(theme_colors.border_variant);

        let branch_from_default_button = |cx: &mut Context<Picker<Self>>| {
            self.default_branch.as_ref().map(|default_branch| {
                Button::new(
                    "branch-from-default",
                    format!("Create New From: {default_branch}"),
                )
                .size(ButtonSize::Medium)
                .key_binding(
                    KeyBinding::for_action_in(&actions::menu::SecondaryConfirm, &focus_handle, cx)
                        .size(ui::rems_from_px(12.0_f32)),
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.delegate.confirm(true, window, cx);
                }))
            })
        };

        let create_button = |cx: &mut Context<Picker<Self>>| {
            Button::new("create-new-branch", "Create")
                .size(ButtonSize::Medium)
                .key_binding(
                    KeyBinding::for_action_in(&actions::menu::Confirm, &focus_handle, cx)
                        .size(ui::rems_from_px(12.0_f32)),
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.delegate.confirm(false, window, cx);
                }))
        };

        let footer = match self.state {
            PickerState::List => {
                let is_new_branch_selected = matches!(
                    self.matches.get(self.selected_index),
                    Some(Entry::NewBranch { .. })
                );

                if is_new_branch_selected && self.default_branch.is_some() {
                    footer_container
                        .children(branch_from_default_button(cx))
                        .child(create_button(cx))
                } else {
                    let delete_button = self
                        .matches
                        .get(self.selected_index)
                        .and_then(Entry::as_branch)
                        .filter(|branch| !branch.is_head)
                        .map(|branch| {
                            let delete_label = if branch.is_remote() {
                                "Delete Local Tracking Branch"
                            } else {
                                "Delete"
                            };

                            Button::new("delete-branch", delete_label)
                                .size(ButtonSize::Medium)
                                .key_binding(
                                    KeyBinding::for_action_in(
                                        &actions::branch_picker::DeleteBranch,
                                        &focus_handle,
                                        cx,
                                    )
                                    .size(ui::rems_from_px(12.0_f32)),
                                )
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(
                                        Box::new(actions::branch_picker::DeleteBranch),
                                        cx,
                                    );
                                })
                        });

                    footer_container.children(delete_button).child(
                        Button::new("switch-branch", "Switch")
                            .size(ButtonSize::Medium)
                            .key_binding(
                                KeyBinding::for_action_in(
                                    &actions::menu::Confirm,
                                    &focus_handle,
                                    cx,
                                )
                                .size(ui::rems_from_px(12.0_f32)),
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.delegate.confirm(false, window, cx);
                            })),
                    )
                }
            }
            PickerState::NewBranch => footer_container
                .children(branch_from_default_button(cx))
                .child(create_button(cx)),
        };

        Some(footer.into_any_element())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_branch_filter_cycle() {
        assert!(BranchFilter::All.next() == BranchFilter::Local);
        assert!(BranchFilter::Local.next() == BranchFilter::Remote);
        assert!(BranchFilter::Remote.next() == BranchFilter::All);
    }
}
