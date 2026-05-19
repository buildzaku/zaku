pub mod dock;
pub mod item;
pub mod notifications;
pub mod pane;
mod persistence;
pub mod status_bar;
pub mod welcome;

pub use dock::{DockPosition, DraggedDock, Panel, PanelHandle};
pub use item::{
    Item, ItemBufferKind, ItemEvent, ItemHandle, ProjectItem, TabContentParams, TabTooltipContent,
    WeakItemHandle,
};
pub use persistence::{
    WorkspaceDb,
    model::{SerializedWorkspace, SerializedWorkspaceLocation, SessionWorkspace},
};

use futures::channel::oneshot;
use gpui::{
    Action, App, Bounds, Context, Div, DragMoveEvent, Entity, FocusHandle, Focusable, Global,
    KeyContext, MouseButton, MouseDownEvent, PathPromptOptions, Pixels, Point, PromptLevel, Size,
    Subscription, Task, Window, WindowBounds, WindowHandle, WindowId, WindowOptions, prelude::*,
};
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

#[cfg(any(test, feature = "test-support"))]
use fs::TempFs;

use http_client::HttpClient;
use metadata::ZAKU_IDENTIFIER;
use project::{Project, ProjectEntryId, ProjectEvent, ProjectPath};
use session::AppSession;
use theme::{ActiveTheme, GlobalTheme, SystemAppearance};
use ui::{StyledTypography, h_flex};
use util::ResultExt;

#[cfg(any(test, feature = "test-support"))]
use session::Session;

#[cfg(test)]
use uuid::Uuid;

use crate::{
    dock::{Dock, PanelButtons},
    notifications::{DetachAndPromptErr, NotificationId, Notifications},
    pane::Pane,
    status_bar::StatusBar,
};

const KEY_CONTEXT: &str = "Workspace";
const MIN_CENTER_PANE_HEIGHT: Pixels = gpui::px(180.0);
const MIN_RESPONSE_PANE_HEIGHT: Pixels = gpui::px(110.0);
const DEFAULT_WINDOW_SIZE: Size<Pixels> = gpui::size(gpui::px(1180.0), gpui::px(760.0));
pub const SERIALIZATION_THROTTLE_TIME: Duration = Duration::from_millis(200);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceId(i64);

impl From<i64> for WorkspaceId {
    fn from(value: i64) -> Self {
        Self(value)
    }
}

impl From<WorkspaceId> for i64 {
    fn from(value: WorkspaceId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OpenMode {
    NewWindow,
    #[default]
    Activate,
}

#[derive(PartialEq, Eq, Debug)]
pub enum CloseIntent {
    Quit,
    CloseWindow,
    ReplaceWindow,
}

#[derive(Clone)]
pub struct Toast {
    id: NotificationId,
    msg: Cow<'static, str>,
    autohide: bool,
    on_click: Option<(Cow<'static, str>, Arc<dyn Fn(&mut Window, &mut App)>)>,
}

impl Toast {
    pub fn new<I: Into<Cow<'static, str>>>(id: NotificationId, msg: I) -> Self {
        Toast {
            id,
            msg: msg.into(),
            on_click: None,
            autohide: false,
        }
    }

    pub fn on_click<F, M>(mut self, message: M, on_click: F) -> Self
    where
        M: Into<Cow<'static, str>>,
        F: Fn(&mut Window, &mut App) + 'static,
    {
        self.on_click = Some((message.into(), Arc::new(on_click)));
        self
    }

    pub fn autohide(mut self) -> Self {
        self.autohide = true;
        self
    }
}

impl PartialEq for Toast {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.msg == other.msg
            && self.on_click.is_some() == other.on_click.is_some()
    }
}

pub struct OpenResult {
    pub window: WindowHandle<Root>,
    pub workspace: Entity<Workspace>,
}

#[derive(Clone)]
pub struct SharedState {
    pub fs: Arc<dyn fs::Fs>,
    pub http_client: Arc<dyn HttpClient>,
    pub session: Entity<AppSession>,
}

struct GlobalSharedState(Arc<SharedState>);

impl Global for GlobalSharedState {}

impl SharedState {
    pub fn new(
        fs: Arc<dyn fs::Fs>,
        http_client: Arc<dyn HttpClient>,
        session: Entity<AppSession>,
    ) -> Self {
        Self {
            fs,
            http_client,
            session,
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test(cx: &mut App) -> Arc<Self> {
        use http_client::FakeHttpClient;

        let fs = TempFs::new(cx.background_executor().clone());
        let http_client = FakeHttpClient::with_404_response();
        let session = cx.new(|cx| AppSession::new(Session::test_new(), cx));

        Arc::new(Self {
            fs,
            http_client,
            session,
        })
    }

    #[track_caller]
    pub fn global(cx: &App) -> Arc<Self> {
        cx.global::<GlobalSharedState>().0.clone()
    }

    pub fn try_global(cx: &App) -> Option<Arc<Self>> {
        cx.try_global::<GlobalSharedState>()
            .map(|shared_state| shared_state.0.clone())
    }

    pub fn set_global(shared_state: Arc<SharedState>, cx: &mut App) {
        cx.set_global(GlobalSharedState(shared_state));
    }
}

pub fn init(shared_state: Arc<SharedState>, cx: &mut App) {
    SharedState::set_global(shared_state.clone(), cx);
    smol::block_on(WorkspaceDb::global(cx).initialize_schema())
        .expect("workspace persistence schema should initialize");

    cx.observe_new({
        move |workspace: &mut Workspace, window, cx| {
            let Some(window) = window else {
                return;
            };
            register_actions(shared_state.clone(), workspace, window, cx);
        }
    })
    .detach();
}

type WorkspaceItemBuilder =
    Box<dyn FnOnce(&mut Pane, &mut Window, &mut Context<Pane>) -> Box<dyn ItemHandle>>;

type BuildProjectItemForPathFn =
    fn(
        &Entity<Project>,
        &ProjectPath,
        &mut Window,
        &mut App,
    ) -> Option<Task<anyhow::Result<(Option<ProjectEntryId>, WorkspaceItemBuilder)>>>;

#[derive(Clone, Default)]
pub struct ProjectItemRegistry {
    build_project_item_for_path_fns: Vec<BuildProjectItemForPathFn>,
}

impl ProjectItemRegistry {
    fn register<I: ProjectItem>(&mut self) {
        self.build_project_item_for_path_fns.push(
            |project: &Entity<Project>,
             project_path: &ProjectPath,
             window: &mut Window,
             cx: &mut App| {
                let project_path = project_path.clone();
                let project_item =
                    <I::Item as project::ProjectItem>::try_open(project, &project_path, cx)?;
                let project = project.clone();

                Some(window.spawn(cx, async move |cx| {
                    let project_item = project_item.await?;
                    let project_entry_id =
                        project_item.read_with(cx, project::ProjectItem::entry_id);
                    let build_workspace_item = Box::new(
                        move |pane: &mut Pane, window: &mut Window, cx: &mut Context<Pane>| {
                            Box::new(cx.new(|cx| {
                                I::for_project_item(project, Some(pane), project_item, window, cx)
                            })) as Box<dyn ItemHandle>
                        },
                    ) as WorkspaceItemBuilder;

                    Ok((project_entry_id, build_workspace_item))
                }))
            },
        );
    }

    fn open_path(
        &self,
        project: &Entity<Project>,
        path: &ProjectPath,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<(Option<ProjectEntryId>, WorkspaceItemBuilder)>> {
        let Some(open_project_item) = self
            .build_project_item_for_path_fns
            .iter()
            .rev()
            .find_map(|open_project_item| open_project_item(project, path, window, cx))
        else {
            return Task::ready(Err(anyhow::anyhow!("cannot open file {:?}", path.path)));
        };

        open_project_item
    }
}

impl Global for ProjectItemRegistry {}

pub fn register_project_item<I: ProjectItem>(cx: &mut App) {
    cx.default_global::<ProjectItemRegistry>().register::<I>();
}

fn register_actions(
    shared_state: Arc<SharedState>,
    workspace: &mut Workspace,
    _: &mut Window,
    _: &mut Context<Workspace>,
) {
    workspace
        .register_action(|workspace, action: &actions::workspace::Open, window, cx| {
            prompt_and_open_workspace(
                workspace,
                PathPromptOptions {
                    files: false,
                    directories: true,
                    multiple: false,
                    prompt: None,
                },
                action.create_new_window,
                window,
                cx,
            );
        })
        .register_action({
            move |_, _: &actions::workspace::NewWindow, _, cx| {
                cx.activate(true);
                let shared_state = shared_state.clone();
                let workspace_db = WorkspaceDb::global(cx);

                cx.spawn(async move |_, cx| {
                    let workspace_id = match workspace_db.next_id().await {
                        Ok(workspace_id) => workspace_id,
                        Err(error) => {
                            log::error!("Failed to allocate workspace id: {error}");
                            return;
                        }
                    };

                    if let Err(error) = cx.update(|cx| {
                        let window_options = default_window_options(cx);
                        cx.open_window(window_options, move |window, cx| {
                            window.activate_window();

                            cx.new(|cx| {
                                Root::new(Workspace::create(workspace_id, shared_state, window, cx))
                            })
                        })
                    }) {
                        log::error!("Failed to open workspace window: {error}");
                    }
                })
                .detach();
            }
        })
        .register_action(|_, _: &actions::zaku::Minimize, window, _| {
            window.minimize_window();
        })
        .register_action(|_, _: &actions::zaku::Zoom, window, _| {
            window.zoom_window();
        });
}

pub fn default_window_options(cx: &mut App) -> WindowOptions {
    let mut bounds = Bounds::centered(None, DEFAULT_WINDOW_SIZE, cx);
    bounds.origin.y -= gpui::px(36.0);

    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        app_id: Some(ZAKU_IDENTIFIER.to_owned()),
        ..WindowOptions::default()
    }
}

pub async fn last_session_workspace_locations(
    db: &WorkspaceDb,
    last_session_id: &str,
    last_session_window_stack: Option<Vec<WindowId>>,
    fs: &dyn fs::Fs,
) -> Option<Vec<SessionWorkspace>> {
    db.last_session_workspace_locations(last_session_id, last_session_window_stack, fs)
        .await
        .log_err()
}

fn find_existing_workspace_window(
    path: &Path,
    cx: &App,
) -> Option<(WindowHandle<Root>, Entity<Workspace>)> {
    for window in cx
        .windows()
        .into_iter()
        .filter_map(|window| window.downcast::<Root>())
    {
        if let Ok(root) = window.read(cx) {
            let workspace = root.workspace().clone();
            let is_match = workspace
                .read(cx)
                .root(cx)
                .is_some_and(|root_path| root_path.as_path() == path);

            if is_match {
                return Some((window, workspace));
            }
        }
    }

    None
}

fn prompt_and_open_workspace(
    workspace: &mut Workspace,
    options: PathPromptOptions,
    create_new_window: bool,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let selected_path = workspace.prompt_open_path(options, window, cx);

    cx.spawn_in(window, async move |this, cx| {
        let Some(selected_path) = selected_path.await.log_err().flatten() else {
            return;
        };

        let open_mode = if create_new_window {
            OpenMode::NewWindow
        } else {
            OpenMode::Activate
        };

        if let Some(open_task) = this
            .update_in(cx, |workspace, window, cx| {
                workspace.open_workspace_for_path(selected_path, open_mode, window, cx)
            })
            .log_err()
        {
            open_task.await.log_err();
        }
    })
    .detach();
}

pub struct Root {
    workspace: Entity<Workspace>,
}

impl Root {
    pub fn new(workspace: Entity<Workspace>) -> Self {
        Self { workspace }
    }

    pub fn workspace(&self) -> &Entity<Workspace> {
        &self.workspace
    }

    pub fn replace_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let workspace = self.workspace.clone();
        let shared_state = workspace.read(cx).shared_state().clone();
        let workspace_db = WorkspaceDb::global(cx);

        cx.spawn_in(window, async move |this, cx| {
            let should_replace = workspace
                .update_in(cx, |workspace, window, cx| {
                    workspace.prepare_to_close(CloseIntent::ReplaceWindow, window, cx)
                })?
                .await?;

            if should_replace {
                let workspace_id = workspace_db.next_id().await?;
                this.update_in(cx, |root, window, cx| {
                    root.workspace = Workspace::create(workspace_id, shared_state, window, cx);
                    cx.notify();
                })?;
            }

            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    fn close_project(
        &mut self,
        _: &actions::workspace::CloseProject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace_workspace(window, cx);
    }

    pub fn close_window(
        &mut self,
        _: &actions::workspace::CloseWindow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.spawn_in(window, async move |this, cx| {
            let workspace = this.update(cx, |root, _cx| root.workspace().clone())?;
            let should_close = workspace
                .update_in(cx, |workspace, window, cx| {
                    workspace.prepare_to_close(CloseIntent::CloseWindow, window, cx)
                })?
                .await?;

            if should_close {
                cx.update(|window, _cx| {
                    window.remove_window();
                })?;
            }

            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }
}

impl Focusable for Root {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.workspace.read(cx).focus_handle(cx)
    }
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = self.workspace().clone();
        let workspace_key_context = workspace.update(cx, |workspace, cx| workspace.key_context(cx));
        let root = workspace.update(cx, |workspace, cx| workspace.actions(h_flex(), window, cx));

        root.key_context(workspace_key_context)
            .size_full()
            .on_action(cx.listener(Self::close_project))
            .on_action(cx.listener(Self::close_window))
            .child(
                gpui::div()
                    .flex()
                    .flex_1()
                    .size_full()
                    .child(self.workspace().clone()),
            )
    }
}

pub struct Workspace {
    shared_state: Arc<SharedState>,
    registered_actions: Vec<Box<dyn Fn(Div, &Workspace, &mut Window, &mut Context<Self>) -> Div>>,
    database_id: Option<WorkspaceId>,
    session_id: Option<String>,
    project: Entity<Project>,
    left_dock: Entity<Dock>,
    bottom_dock: Entity<Dock>,
    pane: Entity<Pane>,
    status_bar: Entity<StatusBar>,
    notifications: Notifications,
    suppressed_notifications: HashSet<NotificationId>,
    bounds: Bounds<Pixels>,
    previous_dock_drag_coordinates: Option<Point<Pixels>>,
    scheduled_serialization_task: Option<Task<()>>,
    serialization_task: Option<Task<()>>,
    _project_subscription: Subscription,
    _window_activation_subscription: Subscription,
    _window_appearance_subscription: Subscription,
}

impl Workspace {
    pub fn create<V>(
        workspace_id: WorkspaceId,
        shared_state: Arc<SharedState>,
        window: &mut Window,
        cx: &mut Context<V>,
    ) -> Entity<Self>
    where
        V: 'static,
    {
        let project = cx.new({
            let fs = shared_state.fs.clone();
            move |cx| Project::new(fs.clone(), cx)
        });

        cx.new(|cx| {
            Self::new(
                Some(workspace_id),
                Some(shared_state.session.read(cx).id().to_string()),
                shared_state,
                project,
                window,
                cx,
            )
        })
    }

    pub fn close_window(cx: &mut App) {
        cx.defer(|cx| {
            let Some(root) = cx
                .active_window()
                .and_then(|window| window.downcast::<Root>())
            else {
                return;
            };

            root.update(cx, |root, window, cx| {
                root.close_window(&actions::workspace::CloseWindow, window, cx);
            })
            .log_err();
        });
    }

    pub fn prepare_to_close(
        &mut self,
        close_intent: CloseIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<bool>> {
        cx.spawn_in(window, async move |this, cx| {
            let flush_task =
                this.update_in(cx, |this, window, cx| this.flush_serialization(window, cx))?;
            flush_task.await;

            if close_intent != CloseIntent::Quit {
                this.update_in(cx, |this, window, cx| this.remove_from_session(window, cx))?
                    .await;
            }

            Ok(true)
        })
    }

    fn dock_at_position(&self, position: DockPosition) -> &Entity<Dock> {
        match position {
            DockPosition::Left => &self.left_dock,
            DockPosition::Bottom => &self.bottom_dock,
        }
    }

    pub fn shared_state(&self) -> &Arc<SharedState> {
        &self.shared_state
    }

    pub fn database_id(&self) -> Option<WorkspaceId> {
        self.database_id
    }

    #[cfg(test)]
    pub(crate) fn set_random_database_id(&mut self) {
        self.database_id = Some(WorkspaceId(Uuid::new_v4().as_u64_pair().0.cast_signed()));
    }

    pub fn prompt_open_path(
        &mut self,
        options: PathPromptOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> oneshot::Receiver<Option<PathBuf>> {
        let (tx, rx) = oneshot::channel();
        let path_prompt = cx.prompt_for_paths(options);

        cx.spawn_in(window, async move |workspace, cx| {
            let Ok(selection) = path_prompt.await else {
                return Ok::<(), anyhow::Error>(());
            };

            match selection {
                Ok(selected_paths) => {
                    let selected_path = selected_paths.and_then(|paths| paths.into_iter().next());

                    if tx.send(selected_path).is_err() {
                        return Ok::<(), anyhow::Error>(());
                    }
                }
                Err(error) => {
                    let error_message = error.to_string();
                    let prompt = workspace.update_in(cx, |_, window, cx| {
                        window.prompt(
                            PromptLevel::Critical,
                            "Failed to open project",
                            Some(&error_message),
                            &["Ok"],
                            cx,
                        )
                    })?;

                    if prompt.await.is_err() {
                        return Ok::<(), anyhow::Error>(());
                    }
                }
            }

            Ok::<(), anyhow::Error>(())
        })
        .detach();

        rx
    }

    pub fn open_local(
        path: PathBuf,
        shared_state: Arc<SharedState>,
        requesting_window: Option<WindowHandle<Root>>,
        open_mode: OpenMode,
        cx: &mut App,
    ) -> Task<anyhow::Result<OpenResult>> {
        let window_to_replace = match open_mode {
            OpenMode::NewWindow => None,
            OpenMode::Activate => requesting_window,
        };
        let workspace_db = WorkspaceDb::global(cx);

        cx.spawn(async move |cx| {
            let project = cx
                .update(|cx| Project::open_local(shared_state.fs.clone(), path.clone(), cx))
                .await?;
            let workspace_id = workspace_db.next_id().await?;

            let (window, workspace) = if let Some(window) = window_to_replace {
                let workspace = window.update(cx, |root: &mut Root, window, cx| {
                    let session_id = shared_state.session.read(cx).id().to_string();
                    let project = project.clone();
                    let shared_state = shared_state.clone();
                    let workspace = cx.new(|cx| {
                        Workspace::new(
                            Some(workspace_id),
                            Some(session_id),
                            shared_state,
                            project,
                            window,
                            cx,
                        )
                    });
                    root.workspace = workspace.clone();
                    workspace.update(cx, |workspace: &mut Workspace, cx| {
                        workspace.pane.update(cx, |pane, _cx| {
                            pane.set_should_display_welcome_page(false);
                        });
                        workspace.left_dock.update(cx, |dock, cx| {
                            if let Ok(panel_index) = dock.first_enabled_panel_idx(cx) {
                                dock.activate_panel(panel_index, window, cx);
                                dock.set_open(true, window, cx);
                            }
                        });

                        let focus_handle = workspace.pane.read(cx).focus_handle(cx);
                        window.focus(&focus_handle, cx);
                        workspace.serialize_workspace(window, cx);
                        cx.notify();
                    });
                    cx.notify();
                    workspace
                })?;

                (window, workspace)
            } else {
                let window_options = cx.update(default_window_options);
                let window = cx.open_window(window_options, move |window, cx| {
                    let session_id = shared_state.session.read(cx).id().to_string();
                    let workspace = cx.new(|cx| {
                        Workspace::new(
                            Some(workspace_id),
                            Some(session_id),
                            shared_state,
                            project,
                            window,
                            cx,
                        )
                    });
                    cx.new(|_| Root::new(workspace))
                })?;

                let workspace = window.update(cx, |root: &mut Root, window, cx| {
                    let workspace = root.workspace().clone();
                    workspace.update(cx, |workspace: &mut Workspace, cx| {
                        workspace.pane.update(cx, |pane, _cx| {
                            pane.set_should_display_welcome_page(false);
                        });
                        workspace.left_dock.update(cx, |dock, cx| {
                            if let Ok(panel_index) = dock.first_enabled_panel_idx(cx) {
                                dock.activate_panel(panel_index, window, cx);
                                dock.set_open(true, window, cx);
                            }
                        });

                        let focus_handle = workspace.pane.read(cx).focus_handle(cx);
                        window.focus(&focus_handle, cx);
                        workspace.serialize_workspace(window, cx);
                        cx.notify();
                    });
                    workspace
                })?;

                (window, workspace)
            };

            if let Some(database_id) =
                workspace.read_with(cx, |workspace, _| workspace.database_id())
                && let Err(error) = workspace_db.update_activation_order(database_id).await
            {
                log::error!("Failed to update workspace activation order: {error}");
            }

            Ok(OpenResult { window, workspace })
        })
    }

    pub fn open_workspace_for_path(
        &mut self,
        path: PathBuf,
        mut open_mode: OpenMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Entity<Workspace>>> {
        let requesting_window = window.window_handle().downcast::<Root>();
        let current_workspace = cx.entity();

        let has_worktree = self.project.read(cx).worktree(cx).is_some();
        let has_dirty_items = self.pane.read(cx).items().any(|item| item.is_dirty(cx));
        let is_empty_workspace = !has_worktree && !has_dirty_items;
        if is_empty_workspace {
            open_mode = OpenMode::Activate;
        }

        let shared_state = self.shared_state().clone();

        cx.spawn_in(window, async move |workspace, cx| {
            let path = shared_state.fs.canonicalize(&path).await.unwrap_or(path);
            let existing = cx.update(|_, cx| find_existing_workspace_window(path.as_path(), cx))?;

            if let Some((window, workspace)) = existing {
                window
                    .update(cx, {
                        let workspace = workspace.clone();
                        move |_, window, cx| {
                            window.activate_window();
                            let focus_handle = workspace.read(cx).focus_handle(cx);
                            window.focus(&focus_handle, cx);
                        }
                    })
                    .log_err();

                return Ok(workspace);
            }

            if open_mode == OpenMode::Activate {
                let should_continue = workspace
                    .update_in(cx, |workspace, window, cx| {
                        workspace.prepare_to_close(CloseIntent::ReplaceWindow, window, cx)
                    })?
                    .await?;

                if !should_continue {
                    return Ok(current_workspace);
                }
            }

            let OpenResult { window, workspace } = cx
                .update(|_, cx| {
                    Workspace::open_local(path, shared_state, requesting_window, open_mode, cx)
                })?
                .await?;

            window
                .update(cx, |_, window, _cx| {
                    window.activate_window();
                })
                .log_err();

            Ok(workspace)
        })
    }

    pub fn open_path(
        &mut self,
        path: ProjectPath,
        pane: Option<Entity<Pane>>,
        focus_item: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Box<dyn ItemHandle>>> {
        self.open_path_preview(path, pane, focus_item, false, true, window, cx)
    }

    pub fn open_path_preview(
        &mut self,
        project_path: ProjectPath,
        pane: Option<Entity<Pane>>,
        focus_item: bool,
        allow_preview: bool,
        activate: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Box<dyn ItemHandle>>> {
        let pane = pane.unwrap_or_else(|| self.pane.clone());
        let load_path = self.load_path(&project_path, window, cx);

        cx.spawn_in(window, async move |workspace, cx| {
            let (project_entry_id, build_item) = load_path.await?;
            workspace.update_in(cx, |_, window, cx| {
                pane.update(cx, |pane, cx| {
                    pane.open_item(
                        project_entry_id,
                        &project_path,
                        focus_item,
                        allow_preview,
                        activate,
                        None,
                        window,
                        cx,
                        build_item,
                    )
                })
            })
        })
    }

    fn load_path(
        &self,
        path: &ProjectPath,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<(Option<ProjectEntryId>, WorkspaceItemBuilder)>> {
        let project = self.project();
        let registry = cx.default_global::<ProjectItemRegistry>().clone();
        registry.open_path(project, path, window, cx)
    }

    pub fn project(&self) -> &Entity<Project> {
        &self.project
    }

    pub fn pane(&self) -> &Entity<Pane> {
        &self.pane
    }

    pub fn active_item(&self, cx: &App) -> Option<Box<dyn ItemHandle>> {
        self.pane.read(cx).active_item()
    }

    pub fn active_item_as<I: 'static>(&self, cx: &App) -> Option<Entity<I>> {
        self.active_item(cx)?.to_any_view().downcast::<I>().ok()
    }

    pub fn save_active_item(
        &mut self,
        save_intent: actions::pane::SaveIntent,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<()>> {
        let project = self.project.clone();
        let pane = self.pane.clone();
        let item = pane.read(cx).active_item();
        let pane = pane.downgrade();

        window.spawn(cx, async move |cx| {
            if let Some(item) = item {
                Pane::save_item(project, &pane, item.as_ref(), save_intent, cx)
                    .await
                    .map(|_| ())
            } else {
                Ok(())
            }
        })
    }

    pub fn left_dock(&self) -> &Entity<Dock> {
        &self.left_dock
    }

    pub fn bottom_dock(&self) -> &Entity<Dock> {
        &self.bottom_dock
    }

    pub fn add_panel<T: Panel>(
        &mut self,
        panel: Entity<T>,
        position: DockPosition,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dock = self.dock_at_position(position).clone();
        dock.update(cx, move |dock, cx| dock.add_panel(&panel, window, cx));
    }

    fn root(&self, cx: &App) -> Option<PathBuf> {
        self.project().read(cx).root(cx)
    }

    pub fn worktree_scan_complete(&self, cx: &App) -> impl Future<Output = ()> + 'static + use<> {
        let scan_complete = self.project().read(cx).worktree(cx).and_then(|worktree| {
            worktree
                .read(cx)
                .as_local()
                .map(|worktree| worktree.scan_complete())
        });

        async move {
            if let Some(scan_complete) = scan_complete {
                scan_complete.await;
            }
        }
    }

    pub fn flush_serialization(&mut self, window: &mut Window, cx: &mut App) -> Task<()> {
        self.scheduled_serialization_task.take();
        self.serialization_task.take();

        let serialize_task = self.serialize_workspace_internal(window, cx);
        cx.spawn(async move |_| serialize_task.await)
    }

    fn remove_from_session(&mut self, window: &mut Window, cx: &mut App) -> Task<()> {
        self.session_id.take();
        self.serialize_workspace_internal(window, cx)
    }

    fn serialize_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.scheduled_serialization_task.is_none() {
            self.scheduled_serialization_task = Some(cx.spawn_in(window, async move |this, cx| {
                cx.background_executor()
                    .timer(SERIALIZATION_THROTTLE_TIME)
                    .await;
                if let Err(error) = this.update_in(cx, |this, window, cx| {
                    this.serialization_task = Some(this.serialize_workspace_internal(window, cx));
                    this.scheduled_serialization_task.take();
                }) {
                    log::debug!("Failed to schedule workspace serialization: {error}");
                }
            }));
        }
    }

    fn serialize_workspace_internal(&self, window: &mut Window, cx: &mut App) -> Task<()> {
        let Some(database_id) = self.database_id() else {
            return Task::ready(());
        };

        match self.root(cx) {
            Some(root_path) => {
                let serialized_workspace = SerializedWorkspace {
                    id: database_id,
                    location: SerializedWorkspaceLocation::Local(root_path),
                    session_id: self.session_id.clone(),
                    window_id: self
                        .session_id
                        .as_ref()
                        .map(|_| window.window_handle().window_id().as_u64()),
                };

                let workspace_db = WorkspaceDb::global(cx);
                window.spawn(cx, async move |_| {
                    workspace_db.save_workspace(serialized_workspace).await;
                })
            }
            None => Task::ready(()),
        }
    }

    fn toggle_dock(&mut self, position: DockPosition, window: &mut Window, cx: &mut Context<Self>) {
        let dock = self.dock_at_position(position).clone();
        let was_visible = dock.read(cx).is_open();
        if was_visible
            && !window
                .bindings_for_action(&actions::menu::Cancel)
                .is_empty()
        {
            // Move focus back to the center so dismissing a menu does not focus a hidden dock element.
            let focus_handle = self.pane.read(cx).focus_handle(cx);
            window.focus(&focus_handle, cx);
        }
        window.dispatch_action(actions::menu::Cancel.boxed_clone(), cx);

        let mut focus_center = false;

        dock.update(cx, |dock, cx| {
            if !was_visible {
                let needs_enabled_panel = dock
                    .active_panel()
                    .is_none_or(|active_panel| !active_panel.enabled(cx));

                if needs_enabled_panel {
                    let Ok(panel_index) = dock.first_enabled_panel_idx(cx) else {
                        return;
                    };
                    dock.activate_panel(panel_index, window, cx);
                }
            }

            if was_visible && dock.focus_handle(cx).contains_focused(window, cx) {
                focus_center = true;
            }
            dock.set_open(!was_visible, window, cx);

            if let Some(active_panel) = dock.active_panel()
                && !was_visible
            {
                let focus_handle = active_panel.panel_focus_handle(cx);
                window.focus(&focus_handle, cx);
            }
        });

        if focus_center {
            let focus_handle = self.pane.read(cx).focus_handle(cx);
            window.focus(&focus_handle, cx);
        }

        cx.notify();
    }

    pub fn new(
        database_id: Option<WorkspaceId>,
        session_id: Option<String>,
        shared_state: Arc<SharedState>,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let window_activation_subscription =
            cx.observe_window_activation(window, |workspace, window, cx| {
                if window.is_window_active()
                    && let Some(database_id) = workspace.database_id
                {
                    let workspace_db = WorkspaceDb::global(cx);
                    cx.background_spawn(async move {
                        if let Err(error) = workspace_db.update_activation_order(database_id).await
                        {
                            log::error!("Failed to update workspace activation order: {error}");
                        }
                    })
                    .detach();
                }
            });

        let window_appearance_subscription =
            cx.observe_window_appearance(window, |_, window, cx| {
                let window_appearance = window.appearance();
                *SystemAppearance::global_mut(cx) = SystemAppearance(window_appearance.into());
                GlobalTheme::reload_theme(cx);
            });

        let workspace = cx.entity();
        let pane = cx.new(|cx| Pane::new(workspace.downgrade(), &project, window, cx));
        let project_subscription = cx.subscribe_in(
            &project,
            window,
            |workspace: &mut Workspace, _, event, window, cx| match event {
                ProjectEvent::WorktreeAdded
                | ProjectEvent::WorktreeRemoved
                | ProjectEvent::WorktreeUpdatedEntries(_) => {
                    workspace.serialize_workspace(window, cx);
                }
                ProjectEvent::DeletedEntry(entry_id) => {
                    workspace.pane.update(cx, |pane, cx| {
                        pane.handle_deleted_project_item(*entry_id, window, cx);
                    });
                }
            },
        );

        let left_dock = cx.new(|cx| Dock::new(DockPosition::Left, window, cx));
        let bottom_dock = cx.new(|cx| Dock::new(DockPosition::Bottom, window, cx));

        let left_dock_buttons = cx.new(|cx| PanelButtons::new(left_dock.clone(), cx));
        let bottom_dock_buttons = cx.new(|cx| PanelButtons::new(bottom_dock.clone(), cx));

        pane.update(cx, |pane, _| {
            pane.set_should_display_welcome_page(true);
        });

        let status_bar = cx.new(|cx| StatusBar::new(&pane, window, cx));
        status_bar.update(cx, |status_bar, cx| {
            status_bar.add_left_item(left_dock_buttons, window, cx);
            status_bar.add_right_item(bottom_dock_buttons, window, cx);
        });

        let this = Self {
            shared_state,
            registered_actions: Vec::default(),
            database_id,
            session_id,
            project,
            left_dock,
            bottom_dock,
            pane,
            status_bar,
            notifications: Notifications::default(),
            suppressed_notifications: HashSet::default(),
            bounds: Bounds::default(),
            previous_dock_drag_coordinates: None,
            scheduled_serialization_task: None,
            serialization_task: None,
            _project_subscription: project_subscription,
            _window_activation_subscription: window_activation_subscription,
            _window_appearance_subscription: window_appearance_subscription,
        };

        let pane_focus_handle = this.pane.read(cx).focus_handle(cx);
        window.focus(&pane_focus_handle, cx);

        this
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test_new(project: Entity<Project>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let shared_state = SharedState::global(cx);
        window.activate_window();
        let workspace = Self::new(
            None,
            Some(shared_state.session.read(cx).id().to_string()),
            shared_state,
            project,
            window,
            cx,
        );
        workspace
            .pane
            .update(cx, |pane, cx| window.focus(&pane.focus_handle(cx), cx));
        workspace
    }

    fn resize_left_dock(&mut self, size: Pixels, window: &mut Window, cx: &mut App) {
        let max_size =
            (self.bounds.size.width - dock::RESIZE_HANDLE_SIZE).max(dock::RESIZE_HANDLE_SIZE);
        let size = size.min(max_size);
        self.left_dock.update(cx, |dock, cx| {
            dock.resize_active_panel(Some(size), window, cx);
        });
    }

    fn resize_bottom_dock(&mut self, size: Pixels, window: &mut Window, cx: &mut App) {
        let max_size =
            (self.bounds.size.height - MIN_CENTER_PANE_HEIGHT).max(dock::RESIZE_HANDLE_SIZE);
        let size = size
            .min(max_size)
            .max(MIN_RESPONSE_PANE_HEIGHT.min(max_size));
        self.bottom_dock.update(cx, |dock, cx| {
            dock.resize_active_panel(Some(size), window, cx);
        });
    }

    pub fn toggle_panel_focus<T: Panel>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut did_focus_panel = false;
        let docks = [self.left_dock.clone(), self.bottom_dock.clone()];
        let mut toggled_panel = false;

        for dock in docks {
            if let Some(panel_index) = dock.read(cx).panel_index_for_type::<T>() {
                let is_enabled = dock
                    .read(cx)
                    .panel::<T>()
                    .is_some_and(|panel| panel.read(cx).enabled(cx));
                if !is_enabled {
                    break;
                }

                let mut focus_center = false;
                dock.update(cx, |dock, cx| {
                    dock.activate_panel(panel_index, window, cx);

                    let Some(panel) = dock.active_panel() else {
                        return;
                    };
                    let focus_handle = panel.panel_focus_handle(cx);
                    did_focus_panel = !focus_handle.contains_focused(window, cx);

                    if did_focus_panel {
                        dock.set_open(true, window, cx);
                        focus_handle.focus(window, cx);
                    } else {
                        focus_center = true;
                    }
                });

                if focus_center {
                    let focus_handle = self.pane.read(cx).focus_handle(cx);
                    window.focus(&focus_handle, cx);
                }

                toggled_panel = true;
                break;
            }
        }

        if toggled_panel {
            cx.notify();
        }

        did_focus_panel
    }

    pub fn open_panel<T: Panel>(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for dock in [self.left_dock.clone(), self.bottom_dock.clone()] {
            if let Some(panel_index) = dock.read(cx).panel_index_for_type::<T>() {
                dock.update(cx, |dock, cx| {
                    dock.activate_panel(panel_index, window, cx);
                    dock.set_open(true, window, cx);
                });
            }
        }
    }

    pub fn panel<T: Panel>(&self, cx: &App) -> Option<Entity<T>> {
        [self.left_dock.clone(), self.bottom_dock.clone()]
            .into_iter()
            .find_map(|dock| dock.read(cx).panel::<T>())
    }

    pub fn key_context(&self, cx: &App) -> KeyContext {
        let mut context = KeyContext::new_with_defaults();
        context.add(KEY_CONTEXT);
        context.set("keyboard_layout", cx.keyboard_layout().name().to_string());

        if self.left_dock.read(cx).is_open()
            && let Some(active_panel) = self.left_dock.read(cx).active_panel()
        {
            context.set("left_dock", active_panel.panel_key());
        }

        if self.bottom_dock.read(cx).is_open()
            && let Some(active_panel) = self.bottom_dock.read(cx).active_panel()
        {
            context.set("bottom_dock", active_panel.panel_key());
        }

        context
    }

    pub fn actions(&self, div: Div, window: &mut Window, cx: &mut Context<Self>) -> Div {
        self.with_registered_actions(div, window, cx)
            .on_action(cx.listener(
                |_workspace, action_sequence: &settings::ActionSequence, window, cx| {
                    for action in &action_sequence.0 {
                        window.dispatch_action(action.boxed_clone(), cx);
                    }
                },
            ))
            .on_action(cx.listener(
                |workspace, _: &actions::workspace::ToggleLeftDock, window, cx| {
                    workspace.toggle_dock(DockPosition::Left, window, cx);
                },
            ))
            .on_action(cx.listener(
                |workspace, _: &actions::workspace::ToggleBottomDock, window, cx| {
                    workspace.toggle_dock(DockPosition::Bottom, window, cx);
                },
            ))
            .on_action(
                cx.listener(|workspace, _: &actions::workspace::Save, window, cx| {
                    workspace
                        .save_active_item(actions::pane::SaveIntent::Save, window, cx)
                        .detach_and_prompt_err("Failed to save", window, cx, |_, _, _| None);
                }),
            )
            .on_action(cx.listener(
                |workspace: &mut Workspace, _: &actions::workspace::SuppressNotification, _, cx| {
                    if let Some((notification_id, _)) = workspace.notifications.pop() {
                        workspace.suppress_notification(&notification_id, cx);
                    }
                },
            ))
    }

    pub fn register_action<A: Action>(
        &mut self,
        callback: impl Fn(&mut Self, &A, &mut Window, &mut Context<Self>) + 'static,
    ) -> &mut Self {
        let callback = Arc::new(callback);

        self.registered_actions.push(Box::new(move |div, _, _, cx| {
            let callback = callback.clone();
            div.on_action(cx.listener(move |workspace, event, window, cx| {
                (callback)(workspace, event, window, cx);
            }))
        }));
        self
    }

    pub fn register_action_renderer(
        &mut self,
        callback: impl Fn(Div, &Workspace, &mut Window, &mut Context<Self>) -> Div + 'static,
    ) -> &mut Self {
        self.registered_actions.push(Box::new(callback));
        self
    }

    fn with_registered_actions(
        &self,
        mut div: Div,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        for action in &self.registered_actions {
            div = (action)(div, self, window, cx);
        }
        div
    }

    fn render_notifications(&self, _window: &mut Window, _cx: &mut Context<Self>) -> Option<Div> {
        if self.notifications.is_empty() {
            None
        } else {
            Some(
                gpui::div()
                    .absolute()
                    .right_3()
                    .bottom_3()
                    .w_112()
                    .h_full()
                    .flex()
                    .flex_col()
                    .justify_end()
                    .gap_2()
                    .children(
                        self.notifications
                            .iter()
                            .map(|(_, notification)| notification.clone().into_any()),
                    ),
            )
        }
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui_font = theme::setup_ui_font(window, cx);
        let theme_colors = cx.theme().colors();
        let notification_entities = self
            .notifications
            .iter()
            .map(|(_, notification)| notification.entity_id())
            .collect::<Vec<_>>();

        gpui::div()
            .flex()
            .flex_col()
            .bg(theme_colors.background)
            .text_color(theme_colors.text)
            .font(ui_font)
            .text_ui(cx)
            .size_full()
            .on_modifiers_changed(move |_, _, cx| {
                for &id in &notification_entities {
                    cx.notify(id);
                }
            })
            .on_drag_move(
                cx.listener(|workspace, e: &DragMoveEvent<DraggedDock>, window, cx| {
                    if workspace.previous_dock_drag_coordinates != Some(e.event.position) {
                        workspace.previous_dock_drag_coordinates = Some(e.event.position);
                        match e.drag(cx).0 {
                            DockPosition::Left => {
                                workspace.resize_left_dock(
                                    e.event.position.x - workspace.bounds.left(),
                                    window,
                                    cx,
                                );
                            }
                            DockPosition::Bottom => {
                                workspace.resize_bottom_dock(
                                    workspace.bounds.bottom() - e.event.position.y,
                                    window,
                                    cx,
                                );
                            }
                        }
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|workspace, _: &MouseDownEvent, window, cx| {
                    if !window.default_prevented() {
                        let focus_handle = workspace.focus_handle(cx);
                        window.focus(&focus_handle, cx);
                    }
                }),
            )
            .child(
                gpui::div()
                    .relative()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .overflow_hidden()
                    .child({
                        let this = cx.entity();
                        gpui::canvas(
                            move |bounds, window, cx| {
                                this.update(cx, |this, cx| {
                                    let bounds_changed = this.bounds != bounds;
                                    this.bounds = bounds;

                                    if bounds_changed {
                                        let max_left_dock_size = (bounds.size.width
                                            - dock::RESIZE_HANDLE_SIZE)
                                            .max(dock::RESIZE_HANDLE_SIZE);
                                        this.left_dock.update(cx, |dock, cx| {
                                            dock.clamp_panel_size(max_left_dock_size, window, cx);
                                        });

                                        let max_bottom_dock_size = (bounds.size.height
                                            - MIN_CENTER_PANE_HEIGHT)
                                            .max(dock::RESIZE_HANDLE_SIZE);
                                        this.bottom_dock.update(cx, |dock, cx| {
                                            dock.clamp_panel_size(max_bottom_dock_size, window, cx);
                                        });
                                    }
                                });
                            },
                            |_, (), _, _| {},
                        )
                        .absolute()
                        .size_full()
                    })
                    .child(
                        gpui::div()
                            .flex_none()
                            .overflow_hidden()
                            .child(self.left_dock.clone()),
                    )
                    .child(
                        gpui::div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .h_full()
                            .overflow_hidden()
                            .child(
                                gpui::div()
                                    .flex_1()
                                    .overflow_hidden()
                                    .child(self.pane.clone()),
                            )
                            .child(self.bottom_dock.clone()),
                    )
                    .children(self.render_notifications(window, cx)),
            )
            .child(self.status_bar.clone())
    }
}

impl Focusable for Workspace {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.pane.read(cx).focus_handle(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use gpui::{Modifiers, MouseMoveEvent, MouseUpEvent, TestAppContext};
    use indoc::indoc;
    use serde_json::{Value, json};
    use std::sync::Arc;

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    use fs::Fs;

    use settings::SettingsStore;
    use theme::LoadThemes;
    use util_macros::path;
    use worktree::WorktreeModelHandle;

    use crate::{
        dock::test::TestPanel,
        item::test::{TestItem, TestProjectItem},
        pane::{
            DraggedTab,
            test::{add_labeled_item, assert_item_labels},
        },
    };

    pub fn init_test(shared_state: Arc<SharedState>, cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
            theme::init(LoadThemes::JustBase, cx);
            crate::init(shared_state, cx);
            editor::init(cx);
        });
    }

    #[gpui::test]
    async fn test_concurrent_equivalent_workspace_opens_coalesce_to_canonical_root(
        cx: &mut TestAppContext,
    ) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state.clone(), cx);

        let project = cx.new(|cx| Project::new(temp_fs.clone(), cx));
        let (root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(cx.new(|cx| Workspace::test_new(project, window, cx)))
        });
        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());

        temp_fs.insert_tree(
            path!("project"),
            json!({
                "collection": {
                    "request.toml": indoc! {"
                        [meta]
                        version = 1
                    "}
                }
            }),
        );

        let canonical_project_path = temp_fs.path().join(path!("project"));
        let alternate_project_path = canonical_project_path.join("..").join("project");

        workspace.update_in(cx, |workspace, _, _| workspace.set_random_database_id());

        let first_open = workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_workspace_for_path(
                alternate_project_path.clone(),
                OpenMode::Activate,
                window,
                cx,
            )
        });
        let second_open = workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_workspace_for_path(
                canonical_project_path.clone(),
                OpenMode::Activate,
                window,
                cx,
            )
        });

        first_open
            .await
            .expect("equivalent older workspace open should still succeed");
        second_open
            .await
            .expect("equivalent newer workspace open should succeed");

        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());
        workspace
            .read_with(cx, |workspace, cx| workspace.worktree_scan_complete(cx))
            .await;
        let worktree = workspace.update_in(cx, |workspace, _, cx| {
            workspace.project().read(cx).worktree(cx).unwrap()
        });
        worktree.flush_fs_events(cx).await;

        workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.flush_serialization(window, cx)
            })
            .await;

        let current_root = workspace.update_in(cx, |workspace, _, cx| workspace.root(cx));
        let workspace_db = cx.update(|_, cx| WorkspaceDb::global(cx));
        let recent_workspaces = workspace_db
            .recent_workspaces_on_disk(temp_fs.as_ref())
            .await
            .expect("recent workspace query should succeed");

        assert_eq!(current_root, Some(canonical_project_path.clone()));
        assert!(
            recent_workspaces
                .iter()
                .any(|(_, location, _)| location.path() == canonical_project_path)
        );
        assert!(
            recent_workspaces
                .iter()
                .all(|(_, location, _)| location.path() != alternate_project_path)
        );
    }

    #[gpui::test]
    async fn test_remove_worktree_invalidates_pending_direct_project_open(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state.clone(), cx);
        let project = cx.new(|cx| Project::new(temp_fs.clone(), cx));
        let (root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(cx.new(|cx| Workspace::test_new(project, window, cx)))
        });
        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());

        temp_fs.insert_tree(
            path!("first"),
            json!({
                "collection": {
                    "request.toml": indoc! {"
                        [meta]
                        version = 1
                    "}
                }
            }),
        );
        temp_fs.insert_tree(
            path!("second"),
            json!({
                "collection": {
                    "request.toml": indoc! {"
                        [meta]
                        version = 1
                    "}
                }
            }),
        );

        let first_path = temp_fs.path().join(path!("first"));
        let second_path = temp_fs.path().join(path!("second"));

        let first_open = workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_workspace_for_path(first_path.clone(), OpenMode::Activate, window, cx)
        });
        let workspace = first_open
            .await
            .expect("first workspace open should succeed");

        workspace
            .read_with(cx, |workspace, cx| workspace.worktree_scan_complete(cx))
            .await;
        let worktree = workspace.update_in(cx, |workspace, _, cx| {
            workspace.project().read(cx).worktree(cx).unwrap()
        });
        worktree.flush_fs_events(cx).await;

        let second_open = workspace.update_in(cx, |workspace, _, cx| {
            workspace.project().update(cx, |project, cx| {
                project.find_or_create_worktree(&second_path, cx)
            })
        });

        workspace.update_in(cx, |workspace, _, cx| {
            workspace
                .project()
                .update(cx, |project, cx| project.remove_worktree(cx));
        });

        cx.run_until_parked();

        assert!(
            second_open.await.is_err(),
            "pending open should be invalidated once the current worktree is removed"
        );

        let current_worktree = workspace.update_in(cx, |workspace, _, cx| {
            workspace.project().read(cx).worktree(cx)
        });
        assert!(current_worktree.is_none());
    }

    #[gpui::test]
    fn test_docks_are_disabled_on_welcome_page(cx: &mut TestAppContext) {
        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state.clone(), cx);
        let project = cx.new(|cx| Project::new(temp_fs.clone(), cx));
        let (root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(cx.new(|cx| Workspace::test_new(project, window, cx)))
        });
        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());

        workspace.update_in(cx, |workspace, window, cx| {
            assert!(workspace.pane.read(cx).should_display_welcome_page());
            assert!(!workspace.left_dock.read(cx).is_open());
            assert!(!workspace.bottom_dock.read(cx).is_open());

            workspace.toggle_dock(DockPosition::Left, window, cx);
            workspace.toggle_dock(DockPosition::Bottom, window, cx);

            assert!(!workspace.left_dock.read(cx).is_open());
            assert!(!workspace.bottom_dock.read(cx).is_open());
        });
    }

    #[gpui::test]
    async fn test_open_workspace_hides_welcome_page(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state.clone(), cx);
        let project = cx.new(|cx| Project::new(temp_fs.clone(), cx));
        let (root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(cx.new(|cx| Workspace::test_new(project, window, cx)))
        });
        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());

        temp_fs.insert_tree(
            path!("project"),
            json!({
                ".gitignore": indoc! {"
                    .DS_Store
                "},
                "collection": {
                    "request.toml": indoc! {"
                        [meta]
                        version = 1
                    "}
                }
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));

        workspace.update_in(cx, |workspace, _, cx| {
            assert!(workspace.pane.read(cx).should_display_welcome_page());
            workspace.set_random_database_id();
        });
        let open_workspace = workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_workspace_for_path(project_path.clone(), OpenMode::Activate, window, cx)
        });
        let workspace = open_workspace.await.expect("workspace open should succeed");

        workspace
            .read_with(cx, |workspace, cx| workspace.worktree_scan_complete(cx))
            .await;
        let worktree = workspace.update_in(cx, |workspace, _, cx| {
            workspace.project().read(cx).worktree(cx).unwrap()
        });
        worktree.flush_fs_events(cx).await;

        workspace
            .update_in(cx, |workspace, window, cx| {
                assert!(!workspace.pane.read(cx).should_display_welcome_page());
                workspace.flush_serialization(window, cx)
            })
            .await;

        let workspace_db = cx.update(|_, cx| WorkspaceDb::global(cx));
        let recent_workspaces = workspace_db
            .recent_workspaces_on_disk(temp_fs.as_ref())
            .await
            .expect("recent workspace query should succeed");
        assert!(
            recent_workspaces
                .iter()
                .any(|(_, location, _)| { location.path() == project_path })
        );
    }

    #[gpui::test]
    fn test_toggle_docks_and_panels(cx: &mut TestAppContext) {
        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state.clone(), cx);

        let project = cx.new(|cx| Project::new(temp_fs.clone(), cx));
        let (root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(cx.new(|cx| Workspace::test_new(project, window, cx)))
        });
        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());

        let panel = workspace.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| TestPanel::new(100, cx));
            workspace.add_panel(panel.clone(), DockPosition::Left, window, cx);
            workspace.left_dock.update(cx, |left_dock, cx| {
                left_dock.set_open(true, window, cx);
            });
            panel
        });
        let pane = workspace.update_in(cx, |workspace, _, _| workspace.pane.clone());

        workspace.update_in(cx, |workspace, window, cx| {
            assert!(workspace.left_dock.read(cx).is_open());
            assert!(pane.read(cx).focus_handle(cx).contains_focused(window, cx));
        });

        workspace.update_in(cx, |workspace, window, cx| {
            workspace.toggle_panel_focus::<TestPanel>(window, cx);
        });

        workspace.update_in(cx, |workspace, window, cx| {
            let active_panel_id = workspace
                .left_dock
                .read(cx)
                .active_panel()
                .map(|panel| panel.panel_id());

            assert!(workspace.left_dock.read(cx).is_open());
            assert_eq!(active_panel_id, Some(Entity::entity_id(&panel)));
            assert!(panel.read(cx).focus_handle(cx).contains_focused(window, cx));
        });

        workspace.update_in(cx, |workspace, window, cx| {
            workspace.toggle_panel_focus::<TestPanel>(window, cx);
        });

        workspace.update_in(cx, |workspace, window, cx| {
            assert!(workspace.left_dock.read(cx).is_open());
            assert!(!panel.read(cx).focus_handle(cx).contains_focused(window, cx));
            assert!(pane.read(cx).focus_handle(cx).contains_focused(window, cx));
        });

        workspace.update_in(cx, |workspace, window, cx| {
            workspace.toggle_dock(DockPosition::Left, window, cx);
        });

        workspace.update_in(cx, |workspace, window, cx| {
            assert!(!workspace.left_dock.read(cx).is_open());
            assert!(!panel.read(cx).focus_handle(cx).contains_focused(window, cx));
            assert!(pane.read(cx).focus_handle(cx).contains_focused(window, cx));
        });

        workspace.update_in(cx, |workspace, window, cx| {
            workspace.toggle_dock(DockPosition::Left, window, cx);
        });

        workspace.update_in(cx, |workspace, window, cx| {
            let active_panel_id = workspace
                .left_dock
                .read(cx)
                .active_panel()
                .map(|panel| panel.panel_id());

            assert!(workspace.left_dock.read(cx).is_open());
            assert_eq!(active_panel_id, Some(Entity::entity_id(&panel)));
            assert!(panel.read(cx).focus_handle(cx).contains_focused(window, cx));
        });
    }

    #[gpui::test]
    fn test_remove_last_item_refocuses_pane(cx: &mut TestAppContext) {
        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state, cx);

        let project = cx.new(|cx| Project::new(temp_fs.clone(), cx));
        let (root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(cx.new(|cx| Workspace::test_new(project, window, cx)))
        });
        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());
        let pane = workspace.update_in(cx, |workspace, _, _| workspace.pane().clone());
        let item = cx.new(TestItem::new);
        let item_id = Entity::entity_id(&item);

        pane.update_in(cx, |pane, window, cx| {
            pane.add_item(Box::new(item), true, true, true, None, window, cx);
        });

        pane.update_in(cx, |pane, window, cx| {
            assert!(pane.has_focus(window, cx));
            pane.remove_item(item_id, true, true, window, cx);
            assert!(pane.focus_handle(cx).contains_focused(window, cx));
        });

        root.update_in(cx, |_, window, cx| {
            assert!(window.is_action_available(&actions::workspace::NewWindow, cx));
            assert!(window.is_action_available(&actions::workspace::Open::default(), cx));
            assert!(window.is_action_available(&actions::workspace::CloseProject, cx));
        });
    }

    #[gpui::test]
    async fn test_close_all_items(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state, cx);

        temp_fs.insert_tree(path!("project"), Value::default());

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let (root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(cx.new(|cx| Workspace::test_new(project, window, cx)))
        });
        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());
        let pane = workspace.update_in(cx, |workspace, _, _| workspace.pane().clone());

        add_labeled_item(&pane, "First", false, cx);
        add_labeled_item(&pane, "Second", false, cx);
        add_labeled_item(&pane, "Third", false, cx);
        assert_item_labels(&pane, ["First", "Second", "Third*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.close_all_items(
                &actions::pane::CloseAllItems { save_intent: None },
                window,
                cx,
            )
        })
        .await
        .unwrap();
        assert_item_labels(&pane, [], cx);

        add_labeled_item(&pane, "First", true, cx).update(cx, |item, cx| {
            item.project_items
                .push(TestProjectItem::new_dirty(1, "first.toml", cx));
        });
        add_labeled_item(&pane, "Second", true, cx).update(cx, |item, cx| {
            item.project_items
                .push(TestProjectItem::new_dirty(2, "second.toml", cx));
        });
        add_labeled_item(&pane, "Third", true, cx).update(cx, |item, cx| {
            item.project_items
                .push(TestProjectItem::new_dirty(3, "third.toml", cx));
        });
        assert_item_labels(&pane, ["First^", "Second^", "Third*^"], cx);

        let save = pane.update_in(cx, |pane, window, cx| {
            pane.close_all_items(
                &actions::pane::CloseAllItems { save_intent: None },
                window,
                cx,
            )
        });

        cx.executor().run_until_parked();
        cx.simulate_prompt_answer("Save all");
        save.await.unwrap();
        assert_item_labels(&pane, [], cx);

        add_labeled_item(&pane, "First", true, cx);
        add_labeled_item(&pane, "Second", true, cx);
        add_labeled_item(&pane, "Third", true, cx);
        assert_item_labels(&pane, ["First^", "Second^", "Third*^"], cx);

        let save = pane.update_in(cx, |pane, window, cx| {
            pane.close_all_items(
                &actions::pane::CloseAllItems { save_intent: None },
                window,
                cx,
            )
        });

        cx.executor().run_until_parked();
        cx.simulate_prompt_answer("Discard all");
        save.await.unwrap();
        assert_item_labels(&pane, [], cx);

        add_labeled_item(&pane, "First", false, cx);
        add_labeled_item(&pane, "Second", true, cx).update(cx, |item, cx| {
            item.project_items
                .push(TestProjectItem::new_dirty(1, "second.toml", cx));
        });
        add_labeled_item(&pane, "Third", false, cx);
        assert_item_labels(&pane, ["First", "Second^", "Third*"], cx);

        let close_task = pane.update_in(cx, |pane, window, cx| {
            pane.close_all_items(
                &actions::pane::CloseAllItems { save_intent: None },
                window,
                cx,
            )
        });

        cx.executor().run_until_parked();
        cx.simulate_prompt_answer("Cancel");
        close_task.await.unwrap();
        assert_item_labels(&pane, ["Second*^"], cx);
    }

    #[gpui::test]
    async fn test_discard_all_reloads_from_disk(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state, cx);

        temp_fs.insert_tree(path!("project"), Value::default());

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let (root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(cx.new(|cx| Workspace::test_new(project, window, cx)))
        });
        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());
        let pane = workspace.update_in(cx, |workspace, _, _| workspace.pane().clone());

        let first_item = add_labeled_item(&pane, "First", true, cx);
        first_item.update(cx, |item, cx| {
            item.project_items
                .push(TestProjectItem::new_dirty(1, "first.toml", cx));
        });
        let second_item = add_labeled_item(&pane, "Second", true, cx);
        second_item.update(cx, |item, cx| {
            item.project_items
                .push(TestProjectItem::new_dirty(2, "second.toml", cx));
        });
        assert_item_labels(&pane, ["First^", "Second*^"], cx);

        let close_task = pane.update_in(cx, |pane, window, cx| {
            pane.close_all_items(
                &actions::pane::CloseAllItems { save_intent: None },
                window,
                cx,
            )
        });

        cx.executor().run_until_parked();
        cx.simulate_prompt_answer("Discard all");
        close_task.await.unwrap();
        assert_item_labels(&pane, [], cx);

        first_item.read_with(cx, |item, _| {
            assert_eq!(item.reload_count, 1, "first item should have been reloaded");
            assert!(
                !item.is_dirty,
                "first item should no longer be dirty after reload"
            );
        });
        second_item.read_with(cx, |item, _| {
            assert_eq!(
                item.reload_count, 1,
                "second item should have been reloaded"
            );
            assert!(
                !item.is_dirty,
                "second item should no longer be dirty after reload"
            );
        });
    }

    #[gpui::test]
    async fn test_dont_save_single_file_reloads_from_disk(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state, cx);

        temp_fs.insert_tree(path!("project"), Value::default());

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let (root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(cx.new(|cx| Workspace::test_new(project, window, cx)))
        });
        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());
        let pane = workspace.update_in(cx, |workspace, _, _| workspace.pane().clone());

        let item = add_labeled_item(&pane, "First", true, cx);
        item.update(cx, |item, cx| {
            item.project_items
                .push(TestProjectItem::new_dirty(1, "first.toml", cx));
        });
        assert_item_labels(&pane, ["First*^"], cx);

        let close_task = pane.update_in(cx, |pane, window, cx| {
            pane.close_item_by_id(item.item_id(), actions::pane::SaveIntent::Close, window, cx)
        });

        cx.executor().run_until_parked();
        cx.simulate_prompt_answer("Don't Save");
        close_task.await.unwrap();
        assert_item_labels(&pane, [], cx);

        item.read_with(cx, |item, _| {
            assert_eq!(item.reload_count, 1, "item should have been reloaded");
            assert!(
                !item.is_dirty,
                "item should no longer be dirty after reload"
            );
        });
    }

    #[gpui::test]
    async fn test_close_with_save_intent(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state, cx);

        temp_fs.insert_tree(path!("project"), Value::default());

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let (root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(cx.new(|cx| Workspace::test_new(project, window, cx)))
        });
        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());
        let pane = workspace.update_in(cx, |workspace, _, _| workspace.pane().clone());

        let first = cx.update(|_, cx| TestProjectItem::new_dirty(1, "first.toml", cx));
        let second = cx.update(|_, cx| TestProjectItem::new_dirty(2, "second.toml", cx));
        let third = cx.update(|_, cx| TestProjectItem::new_dirty(3, "third.toml", cx));

        add_labeled_item(&pane, "First", true, cx).update(cx, |item, _| {
            item.project_items.push(first.clone());
            item.project_items.push(second.clone());
        });
        add_labeled_item(&pane, "Second", true, cx)
            .update(cx, |item, _| item.project_items.push(third.clone()));
        assert_item_labels(&pane, ["First^", "Second*^"], cx);

        pane.update_in(cx, |pane, window, cx| {
            pane.close_all_items(
                &actions::pane::CloseAllItems {
                    save_intent: Some(actions::pane::SaveIntent::Save),
                },
                window,
                cx,
            )
        })
        .await
        .unwrap();

        assert_item_labels(&pane, [], cx);
        cx.update(|_, cx| {
            assert!(!first.read(cx).is_dirty);
            assert!(!second.read(cx).is_dirty);
            assert!(!third.read(cx).is_dirty);
        });
    }

    #[gpui::test]
    async fn test_drag_first_tab_to_last_position(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state, cx);

        temp_fs.insert_tree(path!("project"), Value::default());

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let (root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(cx.new(|cx| Workspace::test_new(project, window, cx)))
        });
        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());
        let pane = workspace.update_in(cx, |workspace, _, _| workspace.pane().clone());

        let first_item = add_labeled_item(&pane, "First", false, cx);
        add_labeled_item(&pane, "Second", false, cx);
        add_labeled_item(&pane, "Third", false, cx);
        assert_item_labels(&pane, ["First", "Second", "Third*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            let dragged_tab = DraggedTab {
                pane: cx.entity(),
                item: first_item.boxed_clone(),
                index: 0,
                detail: 0,
                is_active: true,
            };
            pane.handle_tab_drop(&dragged_tab, 2, window, cx);
        });

        assert_item_labels(&pane, ["Second", "Third", "First*"], cx);
    }

    #[gpui::test]
    async fn test_drag_last_tab_to_first_position(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state, cx);

        temp_fs.insert_tree(path!("project"), Value::default());

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let (root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(cx.new(|cx| Workspace::test_new(project, window, cx)))
        });
        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());
        let pane = workspace.update_in(cx, |workspace, _, _| workspace.pane().clone());

        add_labeled_item(&pane, "First", false, cx);
        add_labeled_item(&pane, "Second", false, cx);
        let third_item = add_labeled_item(&pane, "Third", false, cx);
        assert_item_labels(&pane, ["First", "Second", "Third*"], cx);

        pane.update_in(cx, |pane, window, cx| {
            let dragged_tab = DraggedTab {
                pane: cx.entity(),
                item: third_item.boxed_clone(),
                index: 2,
                detail: 0,
                is_active: true,
            };
            pane.handle_tab_drop(&dragged_tab, 0, window, cx);
        });

        assert_item_labels(&pane, ["Third*", "First", "Second"], cx);
    }

    #[gpui::test]
    async fn test_drag_tab_to_middle_tab_with_mouse_events(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state, cx);

        temp_fs.insert_tree(path!("project"), Value::default());

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let (root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(cx.new(|cx| Workspace::test_new(project, window, cx)))
        });
        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());
        let pane = workspace.update_in(cx, |workspace, _, _| workspace.pane().clone());

        add_labeled_item(&pane, "First", false, cx);
        add_labeled_item(&pane, "Second", false, cx);
        add_labeled_item(&pane, "Third", false, cx);
        add_labeled_item(&pane, "Fourth", false, cx);
        assert_item_labels(&pane, ["First", "Second", "Third", "Fourth*"], cx);
        cx.run_until_parked();

        let first_tab_bounds = cx
            .debug_bounds("TAB-0")
            .expect("First tab should have debug bounds");
        let third_tab_bounds = cx
            .debug_bounds("TAB-2")
            .expect("Third tab should have debug bounds");

        cx.simulate_event(MouseDownEvent {
            position: first_tab_bounds.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        });
        cx.run_until_parked();
        cx.simulate_event(MouseMoveEvent {
            position: third_tab_bounds.center(),
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::default(),
        });
        cx.run_until_parked();
        cx.simulate_event(MouseUpEvent {
            position: third_tab_bounds.center(),
            button: MouseButton::Left,
            modifiers: Modifiers::default(),
            click_count: 1,
        });
        cx.run_until_parked();

        assert_item_labels(&pane, ["Second", "Third", "First*", "Fourth"], cx);
    }

    #[gpui::test]
    async fn test_opening_same_workspace_reuses_current_worktree(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state.clone(), cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                "collection": {
                    "request.toml": indoc! {"
                        [meta]
                        version = 1
                    "}
                }
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let (root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(cx.new(|cx| Workspace::test_new(project, window, cx)))
        });
        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());

        let first_worktree_id = workspace
            .update_in(cx, |workspace, _, cx| {
                workspace
                    .project()
                    .read(cx)
                    .worktree(cx)
                    .map(|worktree| worktree.read(cx).id())
            })
            .expect("first open should create a worktree");

        let second_open = workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_workspace_for_path(project_path.clone(), OpenMode::Activate, window, cx)
        });
        second_open
            .await
            .expect("second workspace open should succeed");

        workspace
            .read_with(cx, |workspace, cx| workspace.worktree_scan_complete(cx))
            .await;
        let worktree = workspace.update_in(cx, |workspace, _, cx| {
            workspace.project().read(cx).worktree(cx).unwrap()
        });
        worktree.flush_fs_events(cx).await;

        let (second_worktree_id, current_root) = workspace.update_in(cx, |workspace, _, cx| {
            let project = workspace.project().read(cx);
            (
                project.worktree(cx).map(|worktree| worktree.read(cx).id()),
                project.root(cx),
            )
        });

        assert_eq!(Some(first_worktree_id), second_worktree_id);
        assert_eq!(current_root, Some(project_path));
    }

    #[gpui::test]
    async fn test_opening_same_workspace_in_new_window_with_activate(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state.clone(), cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                "collection": {
                    "request.toml": indoc! {"
                        [meta]
                        version = 1
                    "}
                }
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let workspace_db = cx.update(|cx| WorkspaceDb::global(cx));
        let workspace_id = workspace_db.next_id().await.unwrap();
        let (root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(Workspace::create(workspace_id, shared_state, window, cx))
        });
        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());

        let first_open = workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_workspace_for_path(project_path.clone(), OpenMode::Activate, window, cx)
        });
        let first_workspace = first_open.await.unwrap();

        first_workspace
            .read_with(cx, |workspace, cx| workspace.worktree_scan_complete(cx))
            .await;
        let worktree = first_workspace.update_in(cx, |workspace, _, cx| {
            workspace.project().read(cx).worktree(cx).unwrap()
        });
        worktree.flush_fs_events(cx).await;

        assert_eq!(cx.windows().len(), 1);
        let root_window_id = cx.update(|window, _| window.window_handle().window_id());

        let workspace_db = cx.update(|_, cx| WorkspaceDb::global(cx));
        let workspace_id = workspace_db.next_id().await.unwrap();
        let shared_state = cx.update(|_, cx| SharedState::global(cx));
        let (empty_root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(Workspace::create(workspace_id, shared_state, window, cx))
        });
        let empty_workspace = empty_root.update_in(cx, |root, _, _| root.workspace().clone());
        assert_eq!(cx.windows().len(), 2);

        let second_open = empty_workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_workspace_for_path(project_path.clone(), OpenMode::Activate, window, cx)
        });
        let second_workspace = second_open.await.unwrap();

        assert_eq!(cx.windows().len(), 2);
        assert_eq!(
            Entity::entity_id(&first_workspace),
            Entity::entity_id(&second_workspace)
        );
        assert_eq!(
            root.read_with(cx, |root, _| Entity::entity_id(root.workspace())),
            Entity::entity_id(&first_workspace)
        );
        assert_eq!(
            cx.update(|_, cx| cx.active_window().map(|window| window.window_id())),
            Some(root_window_id)
        );
    }

    #[gpui::test]
    async fn test_opening_same_workspace_in_new_window(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state.clone(), cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                "collection": {
                    "request.toml": indoc! {"
                        [meta]
                        version = 1
                    "}
                }
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let workspace_db = cx.update(|cx| WorkspaceDb::global(cx));
        let workspace_id = workspace_db.next_id().await.unwrap();
        let (root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(Workspace::create(workspace_id, shared_state, window, cx))
        });
        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());

        let first_open = workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_workspace_for_path(project_path.clone(), OpenMode::Activate, window, cx)
        });
        let first_workspace = first_open.await.unwrap();

        first_workspace
            .read_with(cx, |workspace, cx| workspace.worktree_scan_complete(cx))
            .await;
        let worktree = first_workspace.update_in(cx, |workspace, _, cx| {
            workspace.project().read(cx).worktree(cx).unwrap()
        });
        worktree.flush_fs_events(cx).await;

        assert_eq!(cx.windows().len(), 1);

        let second_open = first_workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_workspace_for_path(project_path.clone(), OpenMode::NewWindow, window, cx)
        });
        let second_workspace = second_open.await.unwrap();

        second_workspace
            .read_with(cx, |workspace, cx| workspace.worktree_scan_complete(cx))
            .await;
        let second_worktree = second_workspace.update_in(cx, |workspace, _, cx| {
            workspace.project().read(cx).worktree(cx).unwrap()
        });
        second_worktree.flush_fs_events(cx).await;

        assert_eq!(
            root.read_with(cx, |root, _| Entity::entity_id(root.workspace())),
            Entity::entity_id(&first_workspace)
        );
        assert_eq!(cx.windows().len(), 2);
        assert_ne!(
            Entity::entity_id(&first_workspace),
            Entity::entity_id(&second_workspace)
        );
        let active_workspace_id = cx.update(|_, cx| {
            let active_window = cx.active_window().unwrap().downcast::<Root>().unwrap();
            active_window.read(cx).unwrap().workspace().entity_id()
        });
        assert_eq!(active_workspace_id, Entity::entity_id(&second_workspace));
    }

    #[gpui::test]
    async fn test_opening_equivalent_workspace_path_reuses_current_worktree(
        cx: &mut TestAppContext,
    ) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state.clone(), cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                "collection": {
                    "request.toml": indoc! {"
                        [meta]
                        version = 1
                    "}
                }
            }),
        );

        let canonical_project_path = temp_fs.path().join(path!("project"));
        let alternate_project_path = canonical_project_path.join("..").join("project");
        let project = Project::test_new(temp_fs.clone(), &canonical_project_path, cx).await;
        let (root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(cx.new(|cx| Workspace::test_new(project, window, cx)))
        });
        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());

        let first_worktree_id = workspace
            .update_in(cx, |workspace, _, cx| {
                workspace
                    .project()
                    .read(cx)
                    .worktree(cx)
                    .map(|worktree| worktree.read(cx).id())
            })
            .expect("first open should create a worktree");

        let second_open = workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_workspace_for_path(
                alternate_project_path.clone(),
                OpenMode::Activate,
                window,
                cx,
            )
        });
        second_open
            .await
            .expect("second workspace open should succeed");

        workspace
            .read_with(cx, |workspace, cx| workspace.worktree_scan_complete(cx))
            .await;
        let worktree = workspace.update_in(cx, |workspace, _, cx| {
            workspace.project().read(cx).worktree(cx).unwrap()
        });
        worktree.flush_fs_events(cx).await;

        let (second_worktree_id, current_root) = workspace.update_in(cx, |workspace, _, cx| {
            let project = workspace.project().read(cx);
            (
                project.worktree(cx).map(|worktree| worktree.read(cx).id()),
                project.root(cx),
            )
        });

        assert_eq!(Some(first_worktree_id), second_worktree_id);
        assert_eq!(current_root, Some(canonical_project_path));
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[gpui::test]
    async fn test_opening_symlinked_workspace_path_reuses_current_worktree(
        cx: &mut TestAppContext,
    ) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state.clone(), cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                "collection": {
                    "request.toml": indoc! {"
                        [meta]
                        version = 1
                    "}
                }
            }),
        );

        let canonical_project_path = temp_fs.path().join(path!("project"));
        let alias_project_path = temp_fs.path().join(path!("project-alias"));
        temp_fs
            .create_symlink(&alias_project_path, canonical_project_path.clone())
            .await
            .unwrap();
        let project = Project::test_new(temp_fs.clone(), &canonical_project_path, cx).await;
        let (root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(cx.new(|cx| Workspace::test_new(project, window, cx)))
        });
        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());

        workspace.update_in(cx, |workspace, _, _| workspace.set_random_database_id());

        let first_open = workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_workspace_for_path(
                alias_project_path.clone(),
                OpenMode::Activate,
                window,
                cx,
            )
        });
        first_open
            .await
            .expect("first workspace open should succeed");

        let (first_worktree_id, first_root) = workspace.update_in(cx, |workspace, _, cx| {
            let project = workspace.project().read(cx);
            (
                project.worktree(cx).map(|worktree| worktree.read(cx).id()),
                project.root(cx),
            )
        });

        assert_eq!(first_root, Some(canonical_project_path.clone()));

        let second_open = workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_workspace_for_path(
                canonical_project_path.clone(),
                OpenMode::Activate,
                window,
                cx,
            )
        });
        second_open
            .await
            .expect("second workspace open should succeed");

        workspace
            .read_with(cx, |workspace, cx| workspace.worktree_scan_complete(cx))
            .await;
        let worktree = workspace.update_in(cx, |workspace, _, cx| {
            workspace.project().read(cx).worktree(cx).unwrap()
        });
        worktree.flush_fs_events(cx).await;

        workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.flush_serialization(window, cx)
            })
            .await;

        let (second_worktree_id, current_root) = workspace.update_in(cx, |workspace, _, cx| {
            let project = workspace.project().read(cx);
            (
                project.worktree(cx).map(|worktree| worktree.read(cx).id()),
                project.root(cx),
            )
        });

        let workspace_db = cx.update(|_, cx| WorkspaceDb::global(cx));
        let recent_workspaces = workspace_db
            .recent_workspaces_on_disk(temp_fs.as_ref())
            .await
            .expect("recent workspace query should succeed");

        assert_eq!(first_worktree_id, second_worktree_id);
        assert_eq!(current_root, Some(canonical_project_path.clone()));
        assert!(
            recent_workspaces
                .iter()
                .any(|(_, location, _)| location.path() == canonical_project_path)
        );
        assert!(
            recent_workspaces
                .iter()
                .all(|(_, location, _)| location.path() != alias_project_path)
        );
    }

    #[gpui::test]
    async fn test_opening_different_workspace_replaces_current_worktree(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state.clone(), cx);

        temp_fs.insert_tree(
            path!("first"),
            json!({
                "collection": {
                    "request.toml": indoc! {"
                        [meta]
                        version = 1
                    "}
                }
            }),
        );
        temp_fs.insert_tree(
            path!("second"),
            json!({
                "collection": {
                    "request.toml": indoc! {"
                        [meta]
                        version = 1
                    "}
                }
            }),
        );

        let first_path = temp_fs.path().join(path!("first"));
        let second_path = temp_fs.path().join(path!("second"));
        let project = Project::test_new(temp_fs.clone(), &first_path, cx).await;
        let (root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(cx.new(|cx| Workspace::test_new(project, window, cx)))
        });
        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());

        let first_worktree_id = workspace
            .update_in(cx, |workspace, _, cx| {
                workspace
                    .project()
                    .read(cx)
                    .worktree(cx)
                    .map(|worktree| worktree.read(cx).id())
            })
            .expect("first open should create a worktree");

        let second_open = workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_workspace_for_path(second_path.clone(), OpenMode::Activate, window, cx)
        });
        let workspace = second_open
            .await
            .expect("second workspace open should succeed");

        workspace
            .read_with(cx, |workspace, cx| workspace.worktree_scan_complete(cx))
            .await;
        let worktree = workspace.update_in(cx, |workspace, _, cx| {
            workspace.project().read(cx).worktree(cx).unwrap()
        });
        worktree.flush_fs_events(cx).await;

        let (current_worktree_id, current_root) = workspace.update_in(cx, |workspace, _, cx| {
            let project = workspace.project().read(cx);
            (
                project.worktree(cx).map(|worktree| worktree.read(cx).id()),
                project.root(cx),
            )
        });

        assert_ne!(current_worktree_id, Some(first_worktree_id));
        assert_eq!(current_root, Some(second_path));
    }
}
