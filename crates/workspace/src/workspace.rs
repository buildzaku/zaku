pub mod dock;
pub mod pane;
pub mod panel;
mod persistence;
pub mod status_bar;
pub mod welcome;

pub use persistence::{
    DB as WORKSPACE_DB, WorkspaceDb,
    model::{SerializedWorkspace, SerializedWorkspaceLocation},
};

use futures::channel::oneshot;
use gpui::{
    Action, App, Axis, Bounds, Div, DragMoveEvent, Empty, Entity, FocusHandle, Focusable, Global,
    KeyContext, MouseButton, MouseDownEvent, PathPromptOptions, Pixels, Point, PromptLevel, Size,
    Subscription, Task, Window, WindowBounds, WindowOptions, prelude::*,
};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc, time::Duration};

use actions::{
    menu,
    workspace::{CloseProject, CloseWindow, Open, ToggleBottomDock, ToggleLeftDock},
    zaku::{Minimize, Zoom},
};
use metadata::ZAKU_IDENTIFIER;
use project::Project;
use session::AppSession;
use theme::{ActiveTheme, GlobalTheme, SystemAppearance};
use ui::{StyledTypography, h_flex};
use util::ResultExt;

#[cfg(any(test, feature = "test-support"))]
use session::Session;

use crate::{
    dock::Dock,
    pane::Pane,
    panel::{ProjectPanel, ResponsePanel, buttons::PanelButtons},
    status_bar::StatusBar,
};

const KEY_CONTEXT: &str = "Workspace";
const MIN_DOCK_WIDTH: Pixels = gpui::px(110.0);
const MIN_PANE_WIDTH: Pixels = gpui::px(250.0);
const MIN_CONFIG_PANE_HEIGHT: Pixels = gpui::px(180.0);
const MIN_RESPONSE_PANE_HEIGHT: Pixels = gpui::px(110.0);
const SERIALIZATION_THROTTLE_TIME: Duration = Duration::from_millis(200);
const DEFAULT_WINDOW_SIZE: Size<Pixels> = gpui::size(gpui::px(1180.0), gpui::px(760.0));

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

#[derive(Clone)]
pub struct SharedState {
    pub fs: Arc<dyn fs::Fs>,
    pub session: Entity<AppSession>,
}

struct GlobalSharedState(Arc<SharedState>);

impl Global for GlobalSharedState {}

impl SharedState {
    pub fn new(fs: Arc<dyn fs::Fs>, session: Entity<AppSession>) -> Self {
        Self { fs, session }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test_new(fs: Arc<dyn fs::Fs>, cx: &mut App) -> Self {
        let session = cx.new(|cx| AppSession::new(Session::test_new(), cx));
        Self { fs, session }
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
    SharedState::set_global(shared_state, cx);

    cx.observe_new(|workspace: &mut Workspace, window, cx| {
        let Some(window) = window else {
            return;
        };
        register_actions(workspace, window, cx);
    })
    .detach();
}

fn register_actions(workspace: &mut Workspace, _: &mut Window, _: &mut Context<Workspace>) {
    workspace
        .register_action(|workspace, action: &Open, window, cx| {
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
        .register_action(|_, _: &Minimize, window, _| {
            window.minimize_window();
        })
        .register_action(|_, _: &Zoom, window, _| {
            window.zoom_window();
        });
}

pub fn default_window_options(cx: &mut App) -> WindowOptions {
    let mut bounds = Bounds::centered(None, DEFAULT_WINDOW_SIZE, cx);
    bounds.origin.y -= gpui::px(36.0);

    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        app_id: Some(ZAKU_IDENTIFIER.to_owned()),
        ..Default::default()
    }
}

fn prompt_and_open_workspace(
    workspace: &mut Workspace,
    options: PathPromptOptions,
    create_new_window: bool,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let selected_path = workspace.prompt_open_path(options, window, cx);
    let shared_state = workspace.shared_state().clone();

    cx.spawn_in(window, async move |this, cx| {
        let Some(selected_path) = selected_path.await.log_err().flatten() else {
            return;
        };

        if create_new_window {
            cx.update(|_, cx| {
                let window_options = default_window_options(cx);

                if let Err(error) = cx.open_window(window_options, move |window, cx| {
                    let shared_state = shared_state.clone();
                    let selected_path = selected_path.clone();
                    cx.new(|cx| {
                        let workspace = Workspace::create(shared_state, window, cx);
                        let open_workspace = workspace.update(cx, |workspace, cx| {
                            workspace.open_workspace_for_path(selected_path.clone(), window, cx)
                        });
                        open_workspace.detach_and_log_err(cx);
                        Root::new(workspace)
                    })
                }) {
                    log::error!("Failed to open workspace window: {error}");
                }
            })
            .log_err();
            return;
        }

        if let Some(open_task) = this
            .update_in(cx, |workspace, window, cx| {
                workspace.open_workspace_for_path(selected_path, window, cx)
            })
            .log_err()
        {
            open_task.await.log_err();
        }
    })
    .detach();
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DockPosition {
    Left,
    Bottom,
}

impl DockPosition {
    pub fn label(&self) -> &'static str {
        match self {
            DockPosition::Left => "Left",
            DockPosition::Bottom => "Bottom",
        }
    }

    pub fn axis(&self) -> Axis {
        match self {
            DockPosition::Left => Axis::Horizontal,
            DockPosition::Bottom => Axis::Vertical,
        }
    }
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

        cx.spawn_in(window, async move |this, cx| {
            if let Ok(flush_task) = workspace.update_in(cx, |workspace, window, cx| {
                workspace.flush_serialization(window, cx)
            }) {
                flush_task.await;
            }

            this.update_in(cx, |root, window, cx| {
                root.workspace = Workspace::create(shared_state.clone(), window, cx);
                cx.notify();
            })?;

            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    fn close_project(&mut self, _: &CloseProject, window: &mut Window, cx: &mut Context<Self>) {
        self.replace_workspace(window, cx);
    }

    pub fn close_window(&mut self, _: &CloseWindow, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            let workspace = this.update(cx, |root, _cx| root.workspace().clone())?;
            let flush_task = workspace.update_in(cx, |workspace, window, cx| {
                workspace.flush_serialization(window, cx)
            })?;

            flush_task.await;

            cx.update(|window, _cx| {
                window.remove_window();
            })?;

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
    workspace_actions: Vec<Box<dyn Fn(Div, &Workspace, &mut Window, &mut Context<Self>) -> Div>>,
    database_id: Option<WorkspaceId>,
    project: Entity<Project>,
    left_dock: Entity<Dock>,
    bottom_dock: Entity<Dock>,
    pane: Entity<Pane>,
    response_panel: Entity<ResponsePanel>,
    status_bar: Entity<StatusBar>,
    bounds: Bounds<Pixels>,
    previous_dock_drag_coordinates: Option<Point<Pixels>>,
    _schedule_serialize_workspace: Option<Task<()>>,
    _serialize_workspace_task: Option<Task<()>>,
    _window_appearance_subscription: Subscription,
}

#[derive(Clone)]
pub struct DraggedDock(pub DockPosition);

impl Render for DraggedDock {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

impl Workspace {
    pub fn create<V>(
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
        let workspace = cx.new(|cx| Self::new(shared_state, project, window, cx));
        let weak_workspace = workspace.downgrade();
        let window_id = window.window_handle().window_id().as_u64();

        cx.spawn_in(window, async move |_this, cx| {
            match WORKSPACE_DB.next_id().await {
                Ok(workspace_id) => {
                    match weak_workspace.update_in(cx, |workspace, window, cx| {
                        workspace.set_database_id(workspace_id);
                        workspace._schedule_serialize_workspace.take();
                        workspace._serialize_workspace_task =
                            Some(workspace.serialize_workspace_internal(window, cx));
                        Some(workspace.shared_state().session.read(cx).id().to_string())
                    }) {
                        Ok(session_id) => {
                            cx.background_spawn(async move {
                                if let Err(error) = WORKSPACE_DB
                                    .set_session_binding(workspace_id, session_id, Some(window_id))
                                    .await
                                {
                                    log::error!(
                                        "Failed to bind workspace session metadata: {error}"
                                    );
                                }
                            })
                            .detach();
                        }
                        Err(_) => {
                            cx.background_spawn(async move {
                                if let Err(error) =
                                    WORKSPACE_DB.delete_workspace_by_id(workspace_id).await
                                {
                                    log::error!("Failed to delete unbound workspace id: {error}");
                                }
                            })
                            .detach();
                        }
                    }
                }
                Err(error) => {
                    log::error!("Failed to allocate workspace id: {error}");
                }
            }
        })
        .detach();

        workspace
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
                root.close_window(&CloseWindow, window, cx);
            })
            .log_err();
        });
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

    pub(crate) fn set_database_id(&mut self, id: WorkspaceId) {
        self.database_id = Some(id);
    }

    pub fn prompt_open_path(
        &mut self,
        options: PathPromptOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> oneshot::Receiver<Option<PathBuf>> {
        let (sender, receiver) = oneshot::channel();
        let path_prompt = cx.prompt_for_paths(options);

        cx.spawn_in(window, async move |workspace, cx| {
            let selection = match path_prompt.await {
                Ok(selection) => selection,
                Err(_) => return Ok::<(), anyhow::Error>(()),
            };

            match selection {
                Ok(selected_paths) => {
                    let selected_path = selected_paths.and_then(|paths| paths.into_iter().next());

                    if sender.send(selected_path).is_err() {
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

        receiver
    }

    pub fn open_workspace_for_path(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        let project = self.project();

        cx.spawn_in(window, async move |this, cx| {
            let open_task = project.update(cx, |project, cx| {
                project.find_or_create_worktree(&path, true, cx)
            });
            open_task.await?;

            this.update_in(cx, |workspace, window, cx| {
                workspace.pane.update(cx, |pane, cx| {
                    pane.set_should_display_welcome_page(false, cx);
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
            })?;
            Ok(())
        })
    }

    fn project(&self) -> Entity<Project> {
        self.project.clone()
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
        self._schedule_serialize_workspace.take();
        self._serialize_workspace_task.take();

        let serialize_task = self.serialize_workspace_internal(window, cx);
        cx.spawn(async move |_| {
            serialize_task.await;
        })
    }

    fn serialize_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self._schedule_serialize_workspace.is_none() {
            self._schedule_serialize_workspace =
                Some(cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor()
                        .timer(SERIALIZATION_THROTTLE_TIME)
                        .await;
                    if let Err(error) = this.update_in(cx, |this, window, cx| {
                        this._serialize_workspace_task =
                            Some(this.serialize_workspace_internal(window, cx));
                        this._schedule_serialize_workspace.take();
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
                    session_id: Some(self.shared_state.session.read(cx).id().to_string()),
                    window_id: Some(window.window_handle().window_id().as_u64()),
                };

                window.spawn(cx, async move |_| {
                    WORKSPACE_DB.save_workspace(serialized_workspace).await;
                })
            }
            None => window.spawn(cx, async move |_| {
                if let Err(error) = WORKSPACE_DB.delete_workspace_by_id(database_id).await {
                    log::error!("Failed to delete workspace without root path: {error}");
                }
            }),
        }
    }

    fn toggle_dock(&mut self, position: DockPosition, window: &mut Window, cx: &mut Context<Self>) {
        let dock = self.dock_at_position(position).clone();
        let was_visible = dock.read(cx).is_open();
        if was_visible && !window.bindings_for_action(&menu::Cancel).is_empty() {
            // Move focus back to the center so dismissing a menu does not focus a hidden dock element.
            let focus_handle = self.pane.read(cx).focus_handle(cx);
            window.focus(&focus_handle, cx);
        }
        window.dispatch_action(menu::Cancel.boxed_clone(), cx);

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

            if let Some(active_panel) = dock.active_panel() {
                if !was_visible {
                    let focus_handle = active_panel.panel_focus_handle(cx);
                    window.focus(&focus_handle, cx);
                }
            }
        });

        if focus_center {
            let focus_handle = self.pane.read(cx).focus_handle(cx);
            window.focus(&focus_handle, cx);
        }

        cx.notify();
    }

    pub fn new(
        shared_state: Arc<SharedState>,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let window_appearance_subscription =
            cx.observe_window_appearance(window, |_, window, cx| {
                let window_appearance = window.appearance();
                *SystemAppearance::global_mut(cx) = SystemAppearance(window_appearance.into());
                GlobalTheme::reload_theme(cx);
            });

        let workspace = cx.entity();
        let pane = cx.new(|cx| Pane::new(workspace.downgrade(), window, cx));
        let pane_focus_handle = pane.read(cx).focus_handle(cx);
        window.focus(&pane_focus_handle, cx);

        let left_dock = cx.new(|cx| Dock::new(DockPosition::Left, window, cx));
        let bottom_dock = cx.new(|cx| Dock::new(DockPosition::Bottom, window, cx));

        let pane_handle = pane.downgrade();
        let project_panel =
            cx.new(|cx| ProjectPanel::new(project.clone(), pane_handle.clone(), cx));
        left_dock.update(cx, |left_dock, cx| {
            left_dock.add_panel(project_panel.clone(), window, cx);
        });

        let response_panel = cx.new(|cx| ResponsePanel::new(pane_handle, window, cx));
        bottom_dock.update(cx, |bottom_dock, cx| {
            bottom_dock.add_panel(response_panel.clone(), window, cx);
        });

        let left_dock_buttons = cx.new(|cx| PanelButtons::new(left_dock.clone(), cx));
        let bottom_dock_buttons = cx.new(|cx| PanelButtons::new(bottom_dock.clone(), cx));

        pane.update(cx, |pane, cx| {
            pane.set_should_display_welcome_page(true, cx);
        });

        let status_bar = cx.new(|cx| StatusBar::new(&pane, window, cx));
        status_bar.update(cx, |status_bar, cx| {
            status_bar.add_left_item(left_dock_buttons, window, cx);
            status_bar.add_right_item(bottom_dock_buttons, window, cx);
        });

        let pane_focus_handle = pane.read(cx).focus_handle(cx);
        window.focus(&pane_focus_handle, cx);

        Self {
            shared_state,
            workspace_actions: Default::default(),
            database_id: None,
            project,
            left_dock,
            bottom_dock,
            pane,
            response_panel,
            status_bar,
            bounds: Bounds::default(),
            previous_dock_drag_coordinates: None,
            _schedule_serialize_workspace: None,
            _serialize_workspace_task: None,
            _window_appearance_subscription: window_appearance_subscription,
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test_new(project: Entity<Project>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let shared_state = SharedState::global(cx);
        window.activate_window();
        let workspace = Self::new(shared_state, project, window, cx);
        workspace
            .pane
            .update(cx, |pane, cx| window.focus(&pane.focus_handle(cx), cx));
        workspace
    }

    fn resize_left_dock(&mut self, size: Pixels, window: &mut Window, cx: &mut App) {
        let max_size = (self.bounds.size.width - MIN_PANE_WIDTH).max(dock::RESIZE_HANDLE_SIZE);
        let size = size.min(max_size).max(MIN_DOCK_WIDTH.min(max_size));
        self.left_dock.update(cx, |dock, cx| {
            dock.resize_active_panel(Some(size), window, cx);
        })
    }

    fn resize_bottom_dock(&mut self, size: Pixels, window: &mut Window, cx: &mut App) {
        let max_size =
            (self.bounds.size.height - MIN_CONFIG_PANE_HEIGHT).max(dock::RESIZE_HANDLE_SIZE);
        let size = size
            .min(max_size)
            .max(MIN_RESPONSE_PANE_HEIGHT.min(max_size));
        self.bottom_dock.update(cx, |dock, cx| {
            dock.resize_active_panel(Some(size), window, cx);
        })
    }

    fn toggle_panel_focus<T: panel::Panel>(
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

    fn open_response_panel(&mut self, window: &mut Window, cx: &mut App) -> Entity<ResponsePanel> {
        self.bottom_dock.update(cx, |dock, cx| {
            if let Some(panel_index) = dock.panel_index_for_type::<ResponsePanel>() {
                dock.activate_panel(panel_index, window, cx);
                dock.set_open(true, window, cx);
            }
        });
        self.response_panel.clone()
    }

    pub fn key_context(&self, cx: &App) -> KeyContext {
        let mut context = KeyContext::new_with_defaults();
        context.add(KEY_CONTEXT);
        context.set("keyboard_layout", cx.keyboard_layout().name().to_string());

        if self.left_dock.read(cx).is_open() {
            if let Some(active_panel) = self.left_dock.read(cx).active_panel() {
                context.set("left_dock", active_panel.panel_key());
            }
        }

        if self.bottom_dock.read(cx).is_open() {
            if let Some(active_panel) = self.bottom_dock.read(cx).active_panel() {
                context.set("bottom_dock", active_panel.panel_key());
            }
        }

        context
    }

    pub fn actions(&self, div: Div, window: &mut Window, cx: &mut Context<Self>) -> Div {
        self.add_workspace_actions_listeners(div, window, cx)
            .on_action(cx.listener(
                |_workspace, action_sequence: &settings::ActionSequence, window, cx| {
                    for action in &action_sequence.0 {
                        window.dispatch_action(action.boxed_clone(), cx);
                    }
                },
            ))
            .on_action(cx.listener(|workspace, _: &ToggleLeftDock, window, cx| {
                workspace.toggle_dock(DockPosition::Left, window, cx);
            }))
            .on_action(cx.listener(|workspace, _: &ToggleBottomDock, window, cx| {
                workspace.toggle_dock(DockPosition::Bottom, window, cx);
            }))
    }

    pub fn register_action<A: Action>(
        &mut self,
        callback: impl Fn(&mut Self, &A, &mut Window, &mut Context<Self>) + 'static,
    ) -> &mut Self {
        let callback = Arc::new(callback);

        self.workspace_actions.push(Box::new(move |div, _, _, cx| {
            let callback = callback.clone();
            div.on_action(cx.listener(move |workspace, event, window, cx| {
                (callback)(workspace, event, window, cx)
            }))
        }));
        self
    }

    pub fn register_action_renderer(
        &mut self,
        callback: impl Fn(Div, &Workspace, &mut Window, &mut Context<Self>) -> Div + 'static,
    ) -> &mut Self {
        self.workspace_actions.push(Box::new(callback));
        self
    }

    fn add_workspace_actions_listeners(
        &self,
        mut div: Div,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        for action in self.workspace_actions.iter() {
            div = (action)(div, self, window, cx);
        }
        div
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui_font = theme::setup_ui_font(window, cx);
        let theme_colors = cx.theme().colors();

        gpui::div()
            .flex()
            .flex_col()
            .bg(theme_colors.background)
            .text_color(theme_colors.text)
            .font(ui_font)
            .text_ui(cx)
            .size_full()
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
                                            - MIN_PANE_WIDTH)
                                            .max(dock::RESIZE_HANDLE_SIZE);
                                        this.left_dock.update(cx, |dock, cx| {
                                            dock.clamp_panel_size(max_left_dock_size, window, cx);
                                        });

                                        let max_bottom_dock_size = (bounds.size.height
                                            - MIN_CONFIG_PANE_HEIGHT)
                                            .max(dock::RESIZE_HANDLE_SIZE);
                                        this.bottom_dock.update(cx, |dock, cx| {
                                            dock.clamp_panel_size(max_bottom_dock_size, window, cx);
                                        });
                                    }
                                });
                            },
                            |_, _, _, _| {},
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
                    ),
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

    use gpui::TestAppContext;
    use indoc::indoc;
    use serde_json::json;
    use std::sync::Arc;

    use crate::dock::test::TestPanel;

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    use fs::Fs;

    use fs::TempFs;
    use settings::SettingsStore;
    use theme::LoadThemes;
    use util_macros::path;

    pub fn init_test(shared_state: Arc<SharedState>, cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
            theme::init(LoadThemes::JustBase, cx);
            editor::init(cx);
            crate::init(shared_state, cx);
        });
    }

    #[gpui::test]
    async fn test_concurrent_equivalent_workspace_opens_coalesce_to_canonical_root(
        cx: &mut TestAppContext,
    ) {
        cx.executor().allow_parking();

        let temp_fs = Arc::new(TempFs::new(cx.executor()));
        let shared_state = cx.update(|cx| Arc::new(SharedState::test_new(temp_fs.clone(), cx)));
        init_test(shared_state.clone(), cx);

        let project = cx.new(|cx| Project::new(temp_fs.clone(), cx));
        let (workspace, cx) =
            cx.add_window_view(move |window, cx| Workspace::test_new(project, window, cx));

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

        workspace.update_in(cx, |workspace, _, _| {
            workspace.set_database_id(WorkspaceId::from(1));
        });

        let first_open = workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_workspace_for_path(alternate_project_path.clone(), window, cx)
        });
        let second_open = workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_workspace_for_path(canonical_project_path.clone(), window, cx)
        });

        first_open
            .await
            .expect("equivalent older workspace open should still succeed");
        second_open
            .await
            .expect("equivalent newer workspace open should succeed");

        workspace
            .read_with(cx, |workspace, cx| workspace.worktree_scan_complete(cx))
            .await;

        workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.flush_serialization(window, cx)
            })
            .await;

        let current_root = workspace.update_in(cx, |workspace, _, cx| workspace.root(cx));
        let recent_workspaces = WORKSPACE_DB
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

        let temp_fs = Arc::new(TempFs::new(cx.executor()));
        let shared_state = cx.update(|cx| Arc::new(SharedState::test_new(temp_fs.clone(), cx)));
        init_test(shared_state.clone(), cx);
        let project = cx.new(|cx| Project::new(temp_fs.clone(), cx));
        let (workspace, cx) =
            cx.add_window_view(move |window, cx| Workspace::test_new(project, window, cx));

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
            workspace.open_workspace_for_path(first_path.clone(), window, cx)
        });
        first_open
            .await
            .expect("first workspace open should succeed");

        workspace
            .read_with(cx, |workspace, cx| workspace.worktree_scan_complete(cx))
            .await;

        let second_open = workspace.update_in(cx, |workspace, _, cx| {
            workspace.project().update(cx, |project, cx| {
                project.find_or_create_worktree(&second_path, true, cx)
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
    async fn test_newer_workspace_open_supersedes_previous_workspace_open(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = Arc::new(TempFs::new(cx.executor()));
        let shared_state = cx.update(|cx| Arc::new(SharedState::test_new(temp_fs.clone(), cx)));
        init_test(shared_state.clone(), cx);
        let project = cx.new(|cx| Project::new(temp_fs.clone(), cx));
        let (workspace, cx) =
            cx.add_window_view(move |window, cx| Workspace::test_new(project, window, cx));

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
            workspace.open_workspace_for_path(first_path.clone(), window, cx)
        });
        let second_open = workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_workspace_for_path(second_path.clone(), window, cx)
        });

        second_open
            .await
            .expect("newer workspace open should succeed");
        assert!(
            first_open.await.is_err(),
            "older workspace open should not report success once superseded"
        );

        workspace
            .read_with(cx, |workspace, cx| workspace.worktree_scan_complete(cx))
            .await;

        let current_root = workspace.update_in(cx, |workspace, _, cx| workspace.root(cx));
        assert_eq!(current_root, Some(second_path));
    }

    #[gpui::test]
    async fn test_latest_equivalent_workspace_open_supersedes_intermediate_workspace_open(
        cx: &mut TestAppContext,
    ) {
        cx.executor().allow_parking();

        let temp_fs = Arc::new(TempFs::new(cx.executor()));
        let shared_state = cx.update(|cx| Arc::new(SharedState::test_new(temp_fs.clone(), cx)));
        init_test(shared_state.clone(), cx);
        let project = cx.new(|cx| Project::new(temp_fs.clone(), cx));
        let (workspace, cx) =
            cx.add_window_view(move |window, cx| Workspace::test_new(project, window, cx));

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
            workspace.open_workspace_for_path(first_path.clone(), window, cx)
        });
        let second_open = workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_workspace_for_path(second_path.clone(), window, cx)
        });
        let third_open = workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_workspace_for_path(first_path.clone(), window, cx)
        });

        first_open
            .await
            .expect("matching older workspace open should still succeed");
        assert!(
            second_open.await.is_err(),
            "intermediate workspace open should be superseded by the latest request"
        );
        third_open
            .await
            .expect("latest equivalent workspace open should succeed");

        workspace
            .read_with(cx, |workspace, cx| workspace.worktree_scan_complete(cx))
            .await;

        let current_root = workspace.update_in(cx, |workspace, _, cx| workspace.root(cx));
        assert_eq!(current_root, Some(first_path));
    }

    #[gpui::test]
    async fn test_docks_are_disabled_on_welcome_page(cx: &mut TestAppContext) {
        let temp_fs = Arc::new(TempFs::new(cx.executor()));
        let shared_state = cx.update(|cx| Arc::new(SharedState::test_new(temp_fs.clone(), cx)));
        init_test(shared_state.clone(), cx);
        let project = cx.new(|cx| Project::new(temp_fs.clone(), cx));
        let (workspace, cx) =
            cx.add_window_view(move |window, cx| Workspace::test_new(project, window, cx));

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

        let temp_fs = Arc::new(TempFs::new(cx.executor()));
        let shared_state = cx.update(|cx| Arc::new(SharedState::test_new(temp_fs.clone(), cx)));
        init_test(shared_state.clone(), cx);
        let project = cx.new(|cx| Project::new(temp_fs.clone(), cx));
        let (workspace, cx) =
            cx.add_window_view(move |window, cx| Workspace::test_new(project, window, cx));

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
            workspace.set_database_id(WorkspaceId::from(1));
        });
        let open_workspace = workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_workspace_for_path(project_path.clone(), window, cx)
        });
        open_workspace.await.expect("workspace open should succeed");

        workspace
            .read_with(cx, |workspace, cx| workspace.worktree_scan_complete(cx))
            .await;

        workspace
            .update_in(cx, |workspace, window, cx| {
                assert!(!workspace.pane.read(cx).should_display_welcome_page());
                workspace.flush_serialization(window, cx)
            })
            .await;

        let recent_workspaces = WORKSPACE_DB
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
    async fn test_toggle_docks_and_panels(cx: &mut TestAppContext) {
        let temp_fs = Arc::new(TempFs::new(cx.executor()));
        let shared_state = cx.update(|cx| Arc::new(SharedState::test_new(temp_fs.clone(), cx)));
        init_test(shared_state.clone(), cx);

        let project = cx.new(|cx| Project::new(temp_fs.clone(), cx));
        let (workspace, cx) =
            cx.add_window_view(move |window, cx| Workspace::test_new(project, window, cx));

        let panel = workspace.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| TestPanel::new(100, cx));
            workspace.left_dock.update(cx, |left_dock, cx| {
                left_dock.add_panel(panel.clone(), window, cx);
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
    async fn test_send_request_opens_response_panel(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = Arc::new(TempFs::new(cx.executor()));
        let shared_state = cx.update(|cx| Arc::new(SharedState::test_new(temp_fs.clone(), cx)));
        init_test(shared_state.clone(), cx);

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
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let (workspace, cx) =
            cx.add_window_view(move |window, cx| Workspace::test_new(project, window, cx));

        workspace.update_in(cx, |workspace, _, _| {
            workspace.set_database_id(WorkspaceId::from(1));
        });

        let pane = workspace.update_in(cx, |workspace, _, _| workspace.pane.clone());
        pane.update_in(cx, |pane, window, cx| {
            pane.send_request(window, cx);
        });

        workspace.update_in(cx, |workspace, _, cx| {
            let response_panel_id = Entity::entity_id(&workspace.response_panel);
            let active_panel_id = workspace
                .bottom_dock
                .read(cx)
                .active_panel()
                .map(|panel| panel.panel_id());

            assert!(workspace.bottom_dock.read(cx).is_open());
            assert_eq!(active_panel_id, Some(response_panel_id));
        });
    }

    #[gpui::test]
    async fn test_opening_same_workspace_reuses_current_worktree(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = Arc::new(TempFs::new(cx.executor()));
        let shared_state = cx.update(|cx| Arc::new(SharedState::test_new(temp_fs.clone(), cx)));
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
        let (workspace, cx) =
            cx.add_window_view(move |window, cx| Workspace::test_new(project, window, cx));

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
            workspace.open_workspace_for_path(project_path.clone(), window, cx)
        });
        second_open
            .await
            .expect("second workspace open should succeed");

        workspace
            .read_with(cx, |workspace, cx| workspace.worktree_scan_complete(cx))
            .await;

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
    async fn test_opening_equivalent_workspace_path_reuses_current_worktree(
        cx: &mut TestAppContext,
    ) {
        cx.executor().allow_parking();

        let temp_fs = Arc::new(TempFs::new(cx.executor()));
        let shared_state = cx.update(|cx| Arc::new(SharedState::test_new(temp_fs.clone(), cx)));
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
        let (workspace, cx) =
            cx.add_window_view(move |window, cx| Workspace::test_new(project, window, cx));

        workspace.update_in(cx, |workspace, _, _| {
            workspace.set_database_id(WorkspaceId::from(1));
        });

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
            workspace.open_workspace_for_path(alternate_project_path.clone(), window, cx)
        });
        second_open
            .await
            .expect("second workspace open should succeed");

        workspace
            .read_with(cx, |workspace, cx| workspace.worktree_scan_complete(cx))
            .await;

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

        let recent_workspaces = WORKSPACE_DB
            .recent_workspaces_on_disk(temp_fs.as_ref())
            .await
            .expect("recent workspace query should succeed");

        assert_eq!(Some(first_worktree_id), second_worktree_id);
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

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[gpui::test]
    async fn test_opening_symlinked_workspace_path_reuses_current_worktree(
        cx: &mut TestAppContext,
    ) {
        cx.executor().allow_parking();

        let temp_fs = Arc::new(TempFs::new(cx.executor()));
        let shared_state = cx.update(|cx| Arc::new(SharedState::test_new(temp_fs.clone(), cx)));
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
        let (workspace, cx) =
            cx.add_window_view(move |window, cx| Workspace::test_new(project, window, cx));

        workspace.update_in(cx, |workspace, _, _| {
            workspace.set_database_id(WorkspaceId::from(1));
        });

        let first_open = workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_workspace_for_path(alias_project_path.clone(), window, cx)
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
            workspace.open_workspace_for_path(canonical_project_path.clone(), window, cx)
        });
        second_open
            .await
            .expect("second workspace open should succeed");

        workspace
            .read_with(cx, |workspace, cx| workspace.worktree_scan_complete(cx))
            .await;

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

        let recent_workspaces = WORKSPACE_DB
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

        let temp_fs = Arc::new(TempFs::new(cx.executor()));
        let shared_state = cx.update(|cx| Arc::new(SharedState::test_new(temp_fs.clone(), cx)));
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
        let (workspace, cx) =
            cx.add_window_view(move |window, cx| Workspace::test_new(project, window, cx));

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
            workspace.open_workspace_for_path(second_path.clone(), window, cx)
        });
        second_open
            .await
            .expect("second workspace open should succeed");

        workspace
            .read_with(cx, |workspace, cx| workspace.worktree_scan_complete(cx))
            .await;

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
