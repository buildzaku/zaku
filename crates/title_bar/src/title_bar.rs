mod application_menu;

pub use platform_title_bar::{self, PlatformTitleBar};

use gpui::{
    Anchor, AnyElement, App, Context, ElementId, Entity, MouseButton, SharedString, Subscription,
    WeakEntity, Window, WindowButton, prelude::*,
};
use smallvec::SmallVec;
use std::ffi::OsStr;

use environment_selector::EnvironmentSelector;
use path::PathStyle;
use project::{
    Project,
    git_store::{GitStoreEvent, RepositoryEvent},
    project_config_store::ProjectConfigStoreEvent,
    repo_identity_path,
};
use recent_projects::RecentProjects;
use ui::{
    ActiveTheme, ButtonLike, Color, Divider, DividerColor, DynamicSpacing, Icon, IconAsset,
    IconSize, PlatformStyle, PopoverMenu, PopoverMenuHandle, SelectableButton, Svg, SvgAsset, Text,
    TextCommon, TextSize, Tooltip,
};
use workspace::Workspace;

use crate::application_menu::ApplicationMenu;

const MAX_PROJECT_NAME_LENGTH: usize = 40;
const MAX_BRANCH_NAME_LENGTH: usize = 40;
const MAX_ENVIRONMENT_NAME_LENGTH: usize = 40;
const MAX_SHORT_SHA_LENGTH: usize = 8;

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, window, cx| {
        let Some(window) = window else {
            return;
        };

        let item = cx.new(|cx| TitleBar::new("title-bar", Some(workspace), window, cx));
        workspace.set_titlebar_item(item.into(), window, cx);
    })
    .detach();
}

pub struct TitleBar {
    platform_titlebar: Entity<PlatformTitleBar>,
    project: Option<Entity<Project>>,
    workspace: Option<WeakEntity<Workspace>>,
    application_menu: Option<Entity<ApplicationMenu>>,
    recent_projects_handle: PopoverMenuHandle<RecentProjects>,
    environment_selector_handle: PopoverMenuHandle<EnvironmentSelector>,
    _workspace_subscription: Option<Subscription>,
    _git_store_subscription: Option<Subscription>,
    _project_config_store_subscription: Option<Subscription>,
    _button_layout_subscription: Subscription,
}

impl TitleBar {
    pub fn new(
        id: impl Into<ElementId>,
        workspace: Option<&Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let project = workspace.map(|workspace| workspace.project().clone());
        let git_store = project
            .as_ref()
            .map(|project| project.read(cx).git_store().clone());
        let project_config_store = project
            .as_ref()
            .map(|project| project.read(cx).project_config_store().clone());
        let workspace = workspace.map(Workspace::weak_handle);
        let application_menu = Some(cx.new(|cx| ApplicationMenu::new(window, cx)));
        let workspace_subscription = workspace
            .as_ref()
            .and_then(WeakEntity::upgrade)
            .map(|workspace_entity| cx.observe(&workspace_entity, |_, _, cx| cx.notify()));
        let git_store_subscription = git_store.map(|git_store| {
            cx.subscribe(&git_store, |_, _, event, cx| match event {
                GitStoreEvent::ActiveRepositoryChanged(_)
                | GitStoreEvent::RepositoryUpdated(_, RepositoryEvent::HeadChanged, true) => {
                    cx.notify();
                }
                _ => {}
            })
        });
        let project_config_store_subscription = project_config_store.map(|project_config_store| {
            cx.subscribe(
                &project_config_store,
                |_, _, _: &ProjectConfigStoreEvent, cx| cx.notify(),
            )
        });
        let button_layout_subscription =
            cx.observe_button_layout_changed(window, |_, _, cx| cx.notify());
        let platform_titlebar = cx.new(|cx| PlatformTitleBar::new(id, cx));

        Self {
            platform_titlebar,
            project,
            workspace,
            application_menu,
            recent_projects_handle: PopoverMenuHandle::default(),
            environment_selector_handle: PopoverMenuHandle::default(),
            _workspace_subscription: workspace_subscription,
            _git_store_subscription: git_store_subscription,
            _project_config_store_subscription: project_config_store_subscription,
            _button_layout_subscription: button_layout_subscription,
        }
    }

    fn render_project_name(
        &self,
        name: Option<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let workspace = self.workspace.clone();
        let text_color = if self.recent_projects_handle.is_deployed() {
            Color::Accent
        } else if name.is_some() {
            Color::Default
        } else {
            Color::Muted
        };
        let display_name = if let Some(name) = name {
            util::truncate_and_trailoff(&name, MAX_PROJECT_NAME_LENGTH)
        } else {
            "Open Recent Project".to_string()
        };
        let selected_background = cx.theme().colors().ghost_element_hover;

        PopoverMenu::new("recent-projects-popover")
            .menu(move |window, cx| Some(RecentProjects::popover(workspace.clone()?, window, cx)))
            .offset(gpui::point(gpui::px(0.0), gpui::px(0.5)))
            .trigger_with_tooltip(
                ButtonLike::new("project-name-trigger")
                    .height(IconSize::Small.square(window, cx))
                    .tab_index(0)
                    .selected_background(selected_background)
                    .child(
                        gpui::div().px(DynamicSpacing::Base02.rems(cx)).child(
                            Text::new(
                                ui::utils::replace_control_characters(&display_name).into_owned(),
                            )
                            .size(TextSize::Small)
                            .color(text_color)
                            .single_line(),
                        ),
                    ),
                |_, cx| Tooltip::for_action("Recent Projects", &actions::projects::OpenRecent, cx),
            )
            .anchor(Anchor::TopLeft)
            .with_handle(self.recent_projects_handle.clone())
    }

    fn render_branch(
        &self,
        repository: &Entity<project::git_store::Repository>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let branch_name = {
            let repository = repository.read(cx).snapshot();

            repository
                .branch
                .as_ref()
                .map(|branch| branch.name())
                .map(|name| util::truncate_and_trailoff(name, MAX_BRANCH_NAME_LENGTH))
                .or_else(|| {
                    repository.head_commit.as_ref().map(|commit| {
                        commit
                            .sha
                            .chars()
                            .take(MAX_SHORT_SHA_LENGTH)
                            .collect::<String>()
                    })
                })
        };

        let branch_name = branch_name?;

        Some(
            gpui::div()
                .flex()
                .items_center()
                .gap_1()
                .pl(DynamicSpacing::Base06.rems(cx))
                .child(
                    Icon::new(IconAsset::GitBranch)
                        .size(IconSize::XSmall)
                        .color(Color::Muted),
                )
                .child(
                    Text::new(branch_name)
                        .size(TextSize::Small)
                        .color(Color::Muted)
                        .single_line()
                        .truncate(),
                )
                .into_any_element(),
        )
    }

    fn render_environment(
        &self,
        project: &Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let workspace = self.workspace.clone();
        let project = project.clone();
        let project_config_store = project.read(cx).project_config_store().read(cx);
        let active_environment = project_config_store.active_environment();
        let environment_file =
            active_environment.and_then(|name| project_config_store.environment_file(name));
        let environment_color =
            environment_file.and_then(|environment_file| environment_file.environment.color);
        let is_active_environment_missing = project_config_store.is_active_environment_missing();
        let missing_environment_message = active_environment
            .filter(|_| is_active_environment_missing)
            .map(|name| {
                format!(
                    "{}{}{name}.toml not found",
                    path::project_environments_folder_relative_path().display(PathStyle::local()),
                    PathStyle::local().primary_separator()
                )
            });
        let text_color = if self.environment_selector_handle.is_deployed() {
            Color::Accent
        } else if is_active_environment_missing {
            Color::Warning
        } else if environment_file.is_some() {
            Color::Default
        } else {
            Color::Muted
        };
        let icon = if is_active_environment_missing {
            Icon::new(IconAsset::Warning)
                .size(IconSize::XSmall)
                .color(text_color)
        } else if active_environment.is_none() {
            Icon::new(IconAsset::CircleDashed)
                .size(IconSize::XSmall)
                .color(text_color)
        } else {
            environment_selector::environment_icon(environment_color, text_color)
        };
        let display_name = util::truncate_and_trailoff(
            active_environment.unwrap_or("No Environment"),
            MAX_ENVIRONMENT_NAME_LENGTH,
        );
        let selected_background = cx.theme().colors().ghost_element_hover;

        PopoverMenu::new("environment-selector-popover")
            .menu(move |window, cx| {
                Some(EnvironmentSelector::popover(
                    workspace.clone()?,
                    project.clone(),
                    window,
                    cx,
                ))
            })
            .offset(gpui::point(gpui::px(0.0), gpui::px(0.5)))
            .trigger_with_tooltip(
                ButtonLike::new("environment-trigger")
                    .height(IconSize::Small.square(window, cx))
                    .tab_index(0)
                    .selected_background(selected_background)
                    .child(
                        gpui::div()
                            .flex()
                            .items_center()
                            .gap(DynamicSpacing::Base04.rems(cx))
                            .px(DynamicSpacing::Base02.rems(cx))
                            .child(icon)
                            .child(
                                Text::new(
                                    ui::utils::replace_control_characters(&display_name)
                                        .into_owned(),
                                )
                                .size(TextSize::Small)
                                .color(text_color)
                                .single_line(),
                            ),
                    ),
                move |_, cx| match &missing_environment_message {
                    Some(message) => Tooltip::with_meta(
                        "Environments",
                        Some(&actions::environment_selector::Toggle),
                        message.clone(),
                        cx,
                    ),
                    None => Tooltip::for_action(
                        "Environments",
                        &actions::environment_selector::Toggle,
                        cx,
                    ),
                },
            )
            .anchor(Anchor::TopLeft)
            .with_handle(self.environment_selector_handle.clone())
            .into_any_element()
    }
}

impl Render for TitleBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.upgrade().is_none())
        {
            self.application_menu = None;
        }

        let text_color = cx.theme().colors().text;
        let mut children = SmallVec::<[AnyElement; 2]>::new();
        let button_layout = cx.button_layout();
        let platform_style = PlatformStyle::platform();
        let mut project_name = self.project.as_ref().and_then(|project| {
            project.read(cx).root_worktree(cx).and_then(|worktree| {
                worktree
                    .read(cx)
                    .root_name()
                    .file_name()
                    .map(SharedString::from)
            })
        });
        let repository = self
            .project
            .as_ref()
            .and_then(|project| project.read(cx).git_store().read(cx).active_repository());
        if let Some(repository) = &repository {
            let repository = repository.read(cx).snapshot();
            let identity = repo_identity_path(repository.common_dir_abs_path.as_ref());

            let display_name = if identity.extension() == Some(OsStr::new("git")) {
                identity.file_stem()
            } else {
                identity.file_name()
            };

            if let Some(repo_name) = display_name.and_then(|name| name.to_str()) {
                project_name = Some(SharedString::from(repo_name));
            }
        }
        let branch = repository
            .as_ref()
            .and_then(|repository| self.render_branch(repository, cx));
        let environment = self
            .project
            .as_ref()
            .filter(|project| project.read(cx).root_worktree(cx).is_some())
            .map(|project| self.render_environment(project, window, cx));
        let show_divider = self.workspace.is_some() && (branch.is_some() || environment.is_some());
        let menu_controls_on_left = match platform_style {
            PlatformStyle::Linux => {
                let supported_controls = window.window_controls();

                button_layout.is_some_and(|button_layout| {
                    button_layout
                        .right
                        .iter()
                        .filter_map(|button| *button)
                        .any(|button| match button {
                            WindowButton::Minimize => supported_controls.minimize,
                            WindowButton::Maximize => supported_controls.maximize,
                            WindowButton::Close => true,
                        })
                })
            }
            PlatformStyle::Mac => false,
            PlatformStyle::Windows => true,
        };

        let project_items = gpui::div()
            .flex()
            .items_center()
            .h_full()
            .min_w_0()
            .overflow_x_hidden()
            .flex_1()
            .map(|this| match platform_style {
                PlatformStyle::Mac => this,
                PlatformStyle::Linux | PlatformStyle::Windows => this.pl_1(),
            })
            .child(
                gpui::div()
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .when(self.workspace.is_some(), |this| {
                        this.child(self.render_project_name(project_name, window, cx))
                    })
                    .when(show_divider, |this| {
                        this.child(Divider::vertical().color(DividerColor::Border))
                    })
                    .children(branch)
                    .children(environment)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
            )
            .into_any_element();

        let zaku = gpui::div()
            .flex()
            .items_center()
            .map(|this| match platform_style {
                PlatformStyle::Mac => this.pl(gpui::rems(0.5)).pr(gpui::rems(0.875)),
                PlatformStyle::Linux | PlatformStyle::Windows => this.px(gpui::rems(0.5)),
            })
            .child(
                Svg::with_height(SvgAsset::Zaku, IconSize::Small.rems())
                    .color(Color::Custom(text_color)),
            )
            .into_any_element();
        let application_menu = gpui::div()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base04.rems(cx))
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .children(self.application_menu.clone())
            .into_any_element();
        let mut title_bar_items = SmallVec::<[AnyElement; 3]>::new();

        if menu_controls_on_left {
            title_bar_items.push(zaku);
            title_bar_items.push(application_menu);
            title_bar_items.push(project_items);
        } else {
            title_bar_items.push(project_items);
            title_bar_items.push(application_menu);
            title_bar_items.push(zaku);
        }

        children.push(
            gpui::div()
                .flex()
                .items_center()
                .h_full()
                .w_full()
                .children(title_bar_items)
                .into_any_element(),
        );

        self.platform_titlebar.update(cx, |titlebar, _cx| {
            titlebar.set_button_layout(button_layout);
            titlebar.set_children(children);
        });

        self.platform_titlebar.clone().into_any_element()
    }
}
