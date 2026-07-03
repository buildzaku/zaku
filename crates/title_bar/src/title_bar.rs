mod application_menu;

pub use platform_title_bar::{self, PlatformTitleBar};

use gpui::{
    AnyElement, App, Context, ElementId, Entity, MouseButton, SharedString, Subscription,
    WeakEntity, Window, WindowButton, prelude::*,
};
use smallvec::SmallVec;
use std::ffi::OsStr;

use project::{
    Project,
    git_store::{GitStoreEvent, RepositoryEvent},
    repo_identity_path,
};
use ui::{
    ActiveTheme, Color, DynamicSpacing, Graphic, GraphicName, Icon, IconName, IconSize,
    PlatformStyle, Text, TextCommon, TextSize,
};
use workspace::Workspace;

use crate::application_menu::ApplicationMenu;

const MAX_PROJECT_NAME_LENGTH: usize = 40;
const MAX_BRANCH_NAME_LENGTH: usize = 40;
const MAX_SHORT_SHA_LENGTH: usize = 8;

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, window, cx| {
        let Some(window) = window else {
            return;
        };

        let item = cx.new(|cx| TitleBar::new("title-bar", workspace, window, cx));
        workspace.set_titlebar_item(item.into(), window, cx);
    })
    .detach();
}

pub struct TitleBar {
    platform_titlebar: Entity<PlatformTitleBar>,
    project: Entity<Project>,
    workspace: WeakEntity<Workspace>,
    application_menu: Option<Entity<ApplicationMenu>>,
    _workspace_subscription: Option<Subscription>,
    _git_store_subscription: Subscription,
    _button_layout_subscription: Subscription,
}

impl TitleBar {
    pub fn new(
        id: impl Into<ElementId>,
        workspace: &Workspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let project = workspace.project().clone();
        let git_store = project.read(cx).git_store().clone();
        let workspace = workspace.weak_handle();
        let application_menu = Some(cx.new(|cx| ApplicationMenu::new(window, cx)));
        let workspace_subscription = workspace
            .upgrade()
            .map(|workspace_entity| cx.observe(&workspace_entity, |_, _, cx| cx.notify()));
        let git_store_subscription = cx.subscribe(&git_store, |_, _, event, cx| match event {
            GitStoreEvent::ActiveRepositoryChanged(_)
            | GitStoreEvent::RepositoryUpdated(_, RepositoryEvent::HeadChanged, true) => {
                cx.notify();
            }
            _ => {}
        });
        let button_layout_subscription =
            cx.observe_button_layout_changed(window, |_, _, cx| cx.notify());
        let platform_titlebar = cx.new(|cx| PlatformTitleBar::new(id, cx));

        Self {
            platform_titlebar,
            project,
            workspace,
            application_menu,
            _workspace_subscription: workspace_subscription,
            _git_store_subscription: git_store_subscription,
            _button_layout_subscription: button_layout_subscription,
        }
    }

    fn render_project_name(
        &self,
        name: Option<SharedString>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> impl IntoElement {
        let display_name = if let Some(name) = name {
            util::truncate_and_trailoff(&name, MAX_PROJECT_NAME_LENGTH)
        } else {
            String::new()
        };

        Text::new(display_name)
            .size(TextSize::Small)
            .color(Color::Muted)
            .single_line()
            .truncate()
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
                .gap_px()
                .child(
                    Icon::new(IconName::GitBranch)
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
}

impl Render for TitleBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.workspace.upgrade().is_none() {
            self.application_menu = None;
        }

        let text_color = cx.theme().colors().text;
        let mut children = SmallVec::<[AnyElement; 2]>::new();
        let button_layout = cx.button_layout();
        let platform_style = PlatformStyle::platform();
        let mut project_name = self
            .project
            .read(cx)
            .root_worktree(cx)
            .and_then(|worktree| {
                worktree
                    .read(cx)
                    .root_name()
                    .file_name()
                    .map(SharedString::from)
            });
        let git_store = self.project.read(cx).git_store().clone();
        let repository = git_store.read(cx).active_repository();
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
            .pl_1()
            .gap_2p5()
            .when_some(project_name, |this, project_name| {
                this.child(self.render_project_name(Some(project_name), window, cx))
            })
            .when_some(branch, |this, branch| this.child(branch))
            .into_any_element();

        let zaku = gpui::div()
            .flex()
            .items_center()
            .map(|this| match platform_style {
                PlatformStyle::Mac => this.pl(gpui::rems(0.5)).pr(gpui::rems(0.875)),
                PlatformStyle::Linux | PlatformStyle::Windows => this.px(gpui::rems(0.5)),
            })
            .child(
                Graphic::with_height(GraphicName::Zaku, IconSize::Small.rems())
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
