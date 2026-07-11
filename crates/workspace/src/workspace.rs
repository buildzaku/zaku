mod breadcrumbs;
mod create_project;
pub mod dock;
pub mod item;
mod modal_layer;
pub mod notifications;
pub mod pane;
mod persistence;
pub mod status_bar;
pub mod toolbar;
pub mod welcome;

pub use breadcrumbs::Breadcrumbs;
pub use dock::{DockPosition, DraggedDock, Panel, PanelHandle};
pub use item::{
    Item, ItemBufferKind, ItemEvent, ItemHandle, ProjectItem, SerializableItem,
    SerializableItemHandle, TabContentParams, TabTooltipContent, WeakItemHandle,
};
pub use modal_layer::*;
pub use persistence::{
    SerializedWindowBounds, WorkspaceDb, delete_unloaded_items,
    model::{
        DockData, DockStructure, ItemId, SerializedItem, SerializedPane, SerializedWorkspace,
        SessionWorkspace,
    },
};
pub use toolbar::{Toolbar, ToolbarItemEvent, ToolbarItemLocation, ToolbarItemView};

use anyhow::anyhow;
use futures::{
    StreamExt,
    channel::{
        mpsc::{self, UnboundedReceiver, UnboundedSender},
        oneshot,
    },
};
#[cfg(target_os = "linux")]
use gpui::WindowDecorations;
use gpui::{
    Action, AnyView, App, AsyncWindowContext, Bounds, BoxShadow, Context, CursorStyle, Decorations,
    Div, DragMoveEvent, Entity, EntityId, EventEmitter, FocusHandle, Focusable, Global,
    HitboxBehavior, Hsla, KeyContext, ManagedView, MouseButton, MouseDownEvent, PathPromptOptions,
    Pixels, Point, PromptLevel, ResizeEdge, Size, Stateful, Subscription, Task, Tiling,
    TitlebarOptions, WeakEntity, Window, WindowBackgroundAppearance, WindowBounds, WindowHandle,
    WindowId, WindowOptions, prelude::*,
};
#[cfg(any(test, feature = "test"))]
use gpui::{TestAppContext, VisualTestContext};
use serde::{Deserialize, Serialize};
use std::{
    any::TypeId,
    borrow::Cow,
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use uuid::Uuid;

use db::{Bind, Column, Row, Statement, StaticColumnCount, kv::KeyValueStore};
use http_client::HttpClient;
#[cfg(any(test, feature = "test"))]
use http_client::{FakeHttpClient, StatusCode};
use language::LanguageRegistry;
use metadata::ZAKU_IDENTIFIER;
use project::{Project, ProjectEntryId, ProjectEvent, ProjectPath};
use session::AppSession;
use settings::{SettingsStore, ThemeAppearanceMode};
use theme::{ActiveTheme, Appearance, SystemAppearance};
use ui::StyledTypography;
#[cfg(target_os = "macos")]
use ui::utils;
use util::ResultExt;

#[cfg(any(test, feature = "test"))]
use session::Session;

use crate::{
    create_project::CreateProjectModal,
    dock::{Dock, PanelButtons},
    notifications::{DetachAndPromptErr, NotificationId, Notifications},
    pane::{Pane, PaneEvent},
    status_bar::StatusBar,
};

const KEY_CONTEXT: &str = "Workspace";
const MIN_CENTER_PANE_HEIGHT: Pixels = gpui::px(180.0);
const MIN_RESPONSE_PANE_HEIGHT: Pixels = gpui::px(110.0);
const DEFAULT_WINDOW_SIZE: Size<Pixels> = gpui::size(gpui::px(1180.0), gpui::px(760.0));
pub const SERIALIZATION_THROTTLE_TIME: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

impl StaticColumnCount for WorkspaceId {}

impl Bind for WorkspaceId {
    fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
        i64::from(*self).bind(statement, start_index)
    }
}

impl Column for WorkspaceId {
    fn column(row: &mut Row<'_, '_>, start_index: i32) -> anyhow::Result<(Self, i32)> {
        anyhow::Context::with_context(
            i64::column(row, start_index)
                .map(|(workspace_id, next_index)| (Self(workspace_id), next_index)),
            || format!("Failed to read WorkspaceId at index {start_index}"),
        )
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OpenMode {
    NewWindow,
    #[default]
    Activate,
}

#[derive(Debug, PartialEq, Eq)]
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
pub struct AppState {
    pub fs: Arc<dyn fs::Fs>,
    pub http_client: Arc<dyn HttpClient>,
    pub languages: Arc<LanguageRegistry>,
    pub session: Entity<AppSession>,
}

impl AppState {
    pub fn new(
        fs: Arc<dyn fs::Fs>,
        http_client: Arc<dyn HttpClient>,
        session: Entity<AppSession>,
        languages: Arc<LanguageRegistry>,
    ) -> Self {
        Self {
            fs,
            http_client,
            languages,
            session,
        }
    }

    #[cfg(any(test, feature = "test"))]
    pub fn test_new(
        fs: Arc<dyn fs::Fs>,
        http_client: Option<Arc<dyn HttpClient>>,
        cx: &mut App,
    ) -> Arc<Self> {
        let http_client =
            http_client.unwrap_or_else(|| FakeHttpClient::with_response(StatusCode::NOT_FOUND));
        let languages = Arc::new(LanguageRegistry::test_new(cx.background_executor().clone()));
        let session = cx.new(|cx| AppSession::new(Session::test_new(), cx));

        Arc::new(Self {
            fs,
            http_client,
            languages,
            session,
        })
    }

    #[track_caller]
    pub fn global(cx: &App) -> Arc<Self> {
        cx.global::<GlobalAppState>().0.clone()
    }

    pub fn try_global(cx: &App) -> Option<Arc<Self>> {
        cx.try_global::<GlobalAppState>()
            .map(|app_state| app_state.0.clone())
    }

    pub fn set_global(app_state: Arc<AppState>, cx: &mut App) {
        cx.set_global(GlobalAppState(app_state));
    }
}

struct GlobalAppState(Arc<AppState>);

impl Global for GlobalAppState {}

pub fn init(app_state: Arc<AppState>, cx: &mut App) {
    AppState::set_global(app_state.clone(), cx);

    cx.observe_new({
        move |workspace: &mut Workspace, window, cx| {
            let Some(window) = window else {
                return;
            };
            register_actions(app_state.clone(), workspace, window, cx);
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
            return Task::ready(Err(anyhow!("cannot open file {:?}", path.path)));
        };

        open_project_item
    }
}

impl Global for ProjectItemRegistry {}

pub fn register_project_item<I: ProjectItem>(cx: &mut App) {
    cx.default_global::<ProjectItemRegistry>().register::<I>();
}

#[derive(Clone, Copy)]
struct SerializableItemDescriptor {
    deserialize: fn(
        Entity<Project>,
        WeakEntity<Workspace>,
        WorkspaceId,
        ItemId,
        &mut Window,
        &mut Context<Pane>,
    ) -> Task<anyhow::Result<Box<dyn ItemHandle>>>,
    cleanup: fn(WorkspaceId, Vec<ItemId>, &mut Window, &mut App) -> Task<anyhow::Result<()>>,
    view_to_serializable_item: fn(AnyView) -> Box<dyn SerializableItemHandle>,
}

#[derive(Default)]
pub(crate) struct SerializableItemRegistry {
    descriptors_by_kind: HashMap<Arc<str>, SerializableItemDescriptor>,
    descriptors_by_type: HashMap<TypeId, SerializableItemDescriptor>,
}

impl SerializableItemRegistry {
    pub(crate) fn deserialize(
        item_kind: &str,
        project: Entity<Project>,
        workspace: WeakEntity<Workspace>,
        workspace_id: WorkspaceId,
        item_id: ItemId,
        window: &mut Window,
        cx: &mut Context<Pane>,
    ) -> Task<anyhow::Result<Box<dyn ItemHandle>>> {
        let Some(descriptor) = Self::descriptor(item_kind, cx) else {
            return Task::ready(Err(anyhow!(
                "cannot deserialize {item_kind}, descriptor not found"
            )));
        };

        (descriptor.deserialize)(project, workspace, workspace_id, item_id, window, cx)
    }

    fn cleanup(
        item_kind: &str,
        workspace_id: WorkspaceId,
        loaded_items: Vec<ItemId>,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<()>> {
        let Some(descriptor) = Self::descriptor(item_kind, cx) else {
            return Task::ready(Err(anyhow!(
                "cannot cleanup {item_kind}, descriptor not found"
            )));
        };

        (descriptor.cleanup)(workspace_id, loaded_items, window, cx)
    }

    pub(crate) fn view_to_serializable_item_handle(
        view: AnyView,
        cx: &App,
    ) -> Option<Box<dyn SerializableItemHandle>> {
        let this = cx.try_global::<Self>()?;
        let descriptor = this.descriptors_by_type.get(&view.entity_type())?;
        Some((descriptor.view_to_serializable_item)(view))
    }

    fn descriptor(item_kind: &str, cx: &App) -> Option<SerializableItemDescriptor> {
        let this = cx.try_global::<Self>()?;
        this.descriptors_by_kind.get(item_kind).copied()
    }
}

impl Global for SerializableItemRegistry {}

pub fn register_serializable_item<I: SerializableItem>(cx: &mut App) {
    let serialized_item_kind = I::serialized_item_kind();

    let registry = cx.default_global::<SerializableItemRegistry>();
    let descriptor = SerializableItemDescriptor {
        deserialize: |project, workspace, workspace_id, item_id, window, cx| {
            let task = I::deserialize(project, workspace, workspace_id, item_id, window, cx);
            cx.foreground_executor()
                .spawn(async { Ok(Box::new(task.await?) as Box<_>) })
        },
        cleanup: |workspace_id, loaded_items, window, cx| {
            I::cleanup(workspace_id, loaded_items, window, cx)
        },
        view_to_serializable_item: |view| {
            Box::new(
                view.downcast::<I>()
                    .expect("serializable item descriptor should match view type"),
            )
        },
    };
    registry
        .descriptors_by_kind
        .insert(Arc::from(serialized_item_kind), descriptor);
    registry
        .descriptors_by_type
        .insert(TypeId::of::<I>(), descriptor);
}

pub fn create_and_open_file(
    path: &'static Path,
    window: &mut Window,
    cx: &mut Context<Workspace>,
    default_content: impl FnOnce() -> Cow<'static, str> + Send + 'static,
) -> Task<anyhow::Result<Box<dyn ItemHandle>>> {
    cx.spawn_in(window, async move |workspace, cx| {
        let fs = workspace.read_with(cx, |workspace, _| workspace.app_state.fs.clone())?;

        match fs.metadata(path).await? {
            Some(metadata) if metadata.is_dir => {
                anyhow::bail!("{} is a directory", path.display());
            }
            Some(_) => {}
            None => {
                let default_content = default_content();
                fs.write(path, default_content.as_bytes()).await?;
            }
        }

        let path = fs
            .canonicalize(path)
            .await
            .unwrap_or_else(|_| path.to_path_buf());
        let project = workspace.read_with(cx, |workspace, _| workspace.project().clone())?;
        let (worktree, path) = project
            .update(cx, |project, cx| {
                project.find_or_create_worktree(&path, false, cx)
            })
            .await?;
        let worktree_id = worktree.read_with(cx, |worktree, _| worktree.id());
        let project_path = ProjectPath { worktree_id, path };

        workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.open_path(project_path, None, true, window, cx)
            })?
            .await
    })
}

fn register_actions(
    app_state: Arc<AppState>,
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
        .register_action(
            |workspace, _: &actions::workspace::NewProject, window, cx| {
                let workspace_handle = workspace.weak_handle();
                workspace.toggle_modal(window, cx, move |window, cx| {
                    CreateProjectModal::new(workspace_handle, window, cx)
                });
            },
        )
        .register_action({
            move |_, _: &actions::workspace::NewWindow, _, cx| {
                open_new(app_state.clone(), cx).detach_and_log_err(cx);
            }
        })
        .register_action(|_, _: &actions::zaku::Minimize, window, _| {
            window.minimize_window();
        })
        .register_action(|_, _: &actions::zaku::Zoom, window, _| {
            window.zoom_window();
        })
        .register_action(
            |workspace, action: &actions::projects::ClearRecent, window, cx| {
                workspace.clear_recent_projects(action, window, cx);
            },
        )
        .register_action(
            |workspace, action: &actions::theme::ToggleMode, window, cx| {
                workspace.toggle_theme_mode(action, window, cx);
            },
        );
}

pub fn default_window_options(cx: &mut App) -> WindowOptions {
    let (window_bounds, display) = if cx.active_window().is_some() {
        (None, None)
    } else if let Some((display_uuid, bounds)) =
        persistence::read_default_window_bounds(&KeyValueStore::global(cx))
    {
        (Some(bounds), Some(display_uuid))
    } else if cx.windows().is_empty() {
        let mut bounds = Bounds::centered(None, DEFAULT_WINDOW_SIZE, cx);
        bounds.origin.y -= gpui::px(36.0);
        (Some(WindowBounds::Windowed(bounds)), None)
    } else {
        (None, None)
    };

    let mut options = build_window_options(display, cx);
    options.window_bounds = window_bounds;
    options
}

fn build_window_options(display_uuid: Option<Uuid>, cx: &mut App) -> WindowOptions {
    let display = display_uuid.and_then(|uuid| {
        cx.displays()
            .into_iter()
            .find(|display| display.uuid().ok() == Some(uuid))
    });
    let window_decorations = {
        #[cfg(target_os = "linux")]
        {
            Some(WindowDecorations::Client)
        }
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            None
        }
    };
    let traffic_light_position = {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            None
        }
        #[cfg(target_os = "macos")]
        {
            let (x_inset, y_inset) = utils::traffic_light_inset(gpui::px(32.0), cx);
            Some(gpui::point(x_inset, y_inset))
        }
    };

    WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: None,
            appears_transparent: true,
            traffic_light_position,
        }),
        display_id: display.map(|display| display.id()),
        window_background: WindowBackgroundAppearance::Opaque,
        window_decorations,
        app_id: Some(ZAKU_IDENTIFIER.to_owned()),
        ..WindowOptions::default()
    }
}

pub fn open_new(app_state: Arc<AppState>, cx: &mut App) -> Task<anyhow::Result<()>> {
    let workspace_db = WorkspaceDb::global(cx);

    cx.spawn(async move |cx| {
        let workspace_id = workspace_db.next_id().await?;
        let window_options = cx.update(|cx| {
            cx.activate(true);
            default_window_options(cx)
        });

        cx.open_window(window_options, move |window, cx| {
            window.activate_window();

            cx.new(|cx| Root::new(Workspace::create(workspace_id, app_state, window, cx)))
        })?;

        anyhow::Ok(())
    })
}

pub fn with_active_or_new_workspace(
    cx: &mut App,
    updater: impl FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) + Send + 'static,
) {
    if let Some(root) = cx
        .active_window()
        .and_then(|window| window.downcast::<Root>())
    {
        cx.defer(move |cx| {
            root.update(cx, |root, window, cx| {
                let workspace = root.workspace().clone();
                workspace.update(cx, |workspace, cx| {
                    updater(workspace, window, cx);
                });
            })
            .log_err();
        });
    } else {
        let app_state = AppState::global(cx);
        let workspace_db = WorkspaceDb::global(cx);
        cx.spawn(async move |cx| {
            let workspace_id = workspace_db.next_id().await?;
            let window_options = cx.update(|cx| {
                cx.activate(true);
                default_window_options(cx)
            });

            cx.open_window(window_options, move |window, cx| {
                window.activate_window();
                cx.new(|cx| {
                    let workspace = Workspace::create(workspace_id, app_state, window, cx);
                    workspace.update(cx, |workspace, cx| {
                        updater(workspace, window, cx);
                    });
                    Root::new(workspace)
                })
            })?;

            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
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
        let app_state = workspace.read(cx).app_state().clone();
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
                    root.workspace = Workspace::create(workspace_id, app_state, window, cx);
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
        let root = workspace.update(cx, |workspace, cx| {
            workspace.actions(gpui::div().flex().items_center(), window, cx)
        });
        let modal_layer = workspace.read(cx).modal_layer.clone();

        client_side_decorations(
            root.key_context(workspace_key_context)
                .relative()
                .size_full()
                .on_action(cx.listener(Self::close_project))
                .on_action(cx.listener(Self::close_window))
                .child(
                    gpui::div()
                        .flex()
                        .flex_1()
                        .size_full()
                        .overflow_hidden()
                        .child(self.workspace().clone()),
                )
                .child(modal_layer),
            window,
            cx,
            Tiling::default(),
        )
    }
}

#[cfg(any(test, feature = "test"))]
pub fn build_workspace<'a>(
    project: &Entity<Project>,
    cx: &'a mut TestAppContext,
) -> (Entity<Workspace>, &'a mut VisualTestContext) {
    let project = project.clone();
    let (root, cx) = cx.add_window_view(move |window, cx| {
        window.activate_window();
        Root::new(cx.new(|cx| Workspace::test_new(project, window, cx)))
    });
    cx.run_until_parked();
    let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());
    (workspace, cx)
}

struct GlobalResizeEdge(ResizeEdge);

impl Global for GlobalResizeEdge {}

fn resize_edge(
    position: Point<Pixels>,
    shadow_size: Pixels,
    window_size: Size<Pixels>,
    tiling: Tiling,
) -> Option<ResizeEdge> {
    let bounds = Bounds::new(Point::default(), window_size).inset(shadow_size * 1.5);
    if bounds.contains(&position) {
        return None;
    }

    let corner_size = gpui::size(shadow_size * 1.5, shadow_size * 1.5);
    let top_left_bounds = Bounds::new(Point::new(gpui::px(0.0), gpui::px(0.0)), corner_size);
    if !tiling.top && top_left_bounds.contains(&position) {
        return Some(ResizeEdge::TopLeft);
    }

    let top_right_bounds = Bounds::new(
        Point::new(window_size.width - corner_size.width, gpui::px(0.0)),
        corner_size,
    );
    if !tiling.top && top_right_bounds.contains(&position) {
        return Some(ResizeEdge::TopRight);
    }

    let bottom_left_bounds = Bounds::new(
        Point::new(gpui::px(0.0), window_size.height - corner_size.height),
        corner_size,
    );
    if !tiling.bottom && bottom_left_bounds.contains(&position) {
        return Some(ResizeEdge::BottomLeft);
    }

    let bottom_right_bounds = Bounds::new(
        Point::new(
            window_size.width - corner_size.width,
            window_size.height - corner_size.height,
        ),
        corner_size,
    );
    if !tiling.bottom && bottom_right_bounds.contains(&position) {
        return Some(ResizeEdge::BottomRight);
    }

    if !tiling.top && position.y < shadow_size {
        Some(ResizeEdge::Top)
    } else if !tiling.bottom && position.y > window_size.height - shadow_size {
        Some(ResizeEdge::Bottom)
    } else if !tiling.left && position.x < shadow_size {
        Some(ResizeEdge::Left)
    } else if !tiling.right && position.x > window_size.width - shadow_size {
        Some(ResizeEdge::Right)
    } else {
        None
    }
}

pub fn client_side_decorations(
    element: impl IntoElement,
    window: &mut Window,
    cx: &mut App,
    border_radius_tiling: Tiling,
) -> Stateful<Div> {
    const BORDER_SIZE: Pixels = gpui::px(1.0);
    let decorations = window.window_decorations();
    let tiling = match decorations {
        Decorations::Server => Tiling::default(),
        Decorations::Client { tiling } => tiling,
    };

    match decorations {
        Decorations::Client { .. } => window.set_client_inset(theme::CLIENT_SIDE_DECORATION_SHADOW),
        Decorations::Server => window.set_client_inset(gpui::px(0.0)),
    }

    gpui::div()
        .id("window-backdrop")
        .bg(gpui::transparent_black())
        .map(|this| match decorations {
            Decorations::Server => this,
            Decorations::Client { .. } => this
                .when(
                    !(tiling.top
                        || tiling.right
                        || border_radius_tiling.top
                        || border_radius_tiling.right),
                    |this| this.rounded_tr(theme::CLIENT_SIDE_DECORATION_ROUNDING),
                )
                .when(
                    !(tiling.top
                        || tiling.left
                        || border_radius_tiling.top
                        || border_radius_tiling.left),
                    |this| this.rounded_tl(theme::CLIENT_SIDE_DECORATION_ROUNDING),
                )
                .when(
                    !(tiling.bottom
                        || tiling.right
                        || border_radius_tiling.bottom
                        || border_radius_tiling.right),
                    |this| this.rounded_br(theme::CLIENT_SIDE_DECORATION_ROUNDING),
                )
                .when(
                    !(tiling.bottom
                        || tiling.left
                        || border_radius_tiling.bottom
                        || border_radius_tiling.left),
                    |this| this.rounded_bl(theme::CLIENT_SIDE_DECORATION_ROUNDING),
                )
                .when(!tiling.top, |this| {
                    this.pt(theme::CLIENT_SIDE_DECORATION_SHADOW)
                })
                .when(!tiling.bottom, |this| {
                    this.pb(theme::CLIENT_SIDE_DECORATION_SHADOW)
                })
                .when(!tiling.left, |this| {
                    this.pl(theme::CLIENT_SIDE_DECORATION_SHADOW)
                })
                .when(!tiling.right, |this| {
                    this.pr(theme::CLIENT_SIDE_DECORATION_SHADOW)
                })
                .on_mouse_move(move |event, window, cx| {
                    let size = window.window_bounds().get_bounds().size;
                    let new_edge = resize_edge(
                        event.position,
                        theme::CLIENT_SIDE_DECORATION_SHADOW,
                        size,
                        tiling,
                    );

                    let edge = cx.try_global::<GlobalResizeEdge>();
                    if new_edge != edge.map(|edge| edge.0)
                        && let Err(error) = window.window_handle().update(cx, |root, _, cx| {
                            cx.notify(root.entity_id());
                        })
                    {
                        log::error!("Failed to notify resize edge change: {error:?}");
                    }
                })
                .on_mouse_down(MouseButton::Left, move |event, window, _cx| {
                    let size = window.window_bounds().get_bounds().size;
                    let Some(edge) = resize_edge(
                        event.position,
                        theme::CLIENT_SIDE_DECORATION_SHADOW,
                        size,
                        tiling,
                    ) else {
                        return;
                    };

                    window.start_window_resize(edge);
                }),
        })
        .size_full()
        .child(
            gpui::div()
                .cursor(CursorStyle::Arrow)
                .map(|this| match decorations {
                    Decorations::Server => this,
                    Decorations::Client { .. } => this
                        .border_color(cx.theme().colors().border)
                        .when(
                            !(tiling.top
                                || tiling.right
                                || border_radius_tiling.top
                                || border_radius_tiling.right),
                            |this| this.rounded_tr(theme::CLIENT_SIDE_DECORATION_ROUNDING),
                        )
                        .when(
                            !(tiling.top
                                || tiling.left
                                || border_radius_tiling.top
                                || border_radius_tiling.left),
                            |this| this.rounded_tl(theme::CLIENT_SIDE_DECORATION_ROUNDING),
                        )
                        .when(
                            !(tiling.bottom
                                || tiling.right
                                || border_radius_tiling.bottom
                                || border_radius_tiling.right),
                            |this| this.rounded_br(theme::CLIENT_SIDE_DECORATION_ROUNDING),
                        )
                        .when(
                            !(tiling.bottom
                                || tiling.left
                                || border_radius_tiling.bottom
                                || border_radius_tiling.left),
                            |this| this.rounded_bl(theme::CLIENT_SIDE_DECORATION_ROUNDING),
                        )
                        .when(!tiling.top, |this| this.border_t(BORDER_SIZE))
                        .when(!tiling.bottom, |this| this.border_b(BORDER_SIZE))
                        .when(!tiling.left, |this| this.border_l(BORDER_SIZE))
                        .when(!tiling.right, |this| this.border_r(BORDER_SIZE))
                        .when(!tiling.is_tiled(), |this| {
                            this.shadow(vec![BoxShadow {
                                color: Hsla {
                                    h: 0.0,
                                    s: 0.0,
                                    l: 0.0,
                                    a: 0.4,
                                },
                                blur_radius: theme::CLIENT_SIDE_DECORATION_SHADOW / 2.0,
                                spread_radius: gpui::px(0.0),
                                inset: false,
                                offset: gpui::point(gpui::px(0.0), gpui::px(0.0)),
                            }])
                        }),
                })
                .on_mouse_move(|_event, _window, cx| {
                    cx.stop_propagation();
                })
                .size_full()
                .child(element),
        )
        .map(|this| match decorations {
            Decorations::Server => this,
            Decorations::Client { tiling, .. } => this.child(
                gpui::canvas(
                    |_bounds, window, _cx| {
                        window.insert_hitbox(
                            Bounds::new(
                                gpui::point(gpui::px(0.0), gpui::px(0.0)),
                                window.window_bounds().get_bounds().size,
                            ),
                            HitboxBehavior::Normal,
                        )
                    },
                    move |_bounds, hitbox, window, cx| {
                        let Some(edge) = resize_edge(
                            window.mouse_position(),
                            theme::CLIENT_SIDE_DECORATION_SHADOW,
                            window.window_bounds().get_bounds().size,
                            tiling,
                        ) else {
                            return;
                        };
                        cx.set_global(GlobalResizeEdge(edge));
                        window.set_cursor_style(
                            match edge {
                                ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
                                ResizeEdge::Left | ResizeEdge::Right => {
                                    CursorStyle::ResizeLeftRight
                                }
                                ResizeEdge::TopLeft | ResizeEdge::BottomRight => {
                                    CursorStyle::ResizeUpLeftDownRight
                                }
                                ResizeEdge::TopRight | ResizeEdge::BottomLeft => {
                                    CursorStyle::ResizeUpRightDownLeft
                                }
                            },
                            &hitbox,
                        );
                    },
                )
                .size_full()
                .absolute(),
            ),
        })
}

pub enum WorkspaceEvent {
    PaneAdded(Entity<Pane>),
    ActiveItemChanged,
    PaneRestored(Entity<Pane>),
}

pub struct Workspace {
    app_state: Arc<AppState>,
    weak_self: WeakEntity<Self>,
    registered_actions: Vec<Box<dyn Fn(Div, &Workspace, &mut Window, &mut Context<Self>) -> Div>>,
    database_id: Option<WorkspaceId>,
    session_id: Option<String>,
    project: Entity<Project>,
    left_dock: Entity<Dock>,
    bottom_dock: Entity<Dock>,
    pane: Entity<Pane>,
    panes_by_item: HashMap<EntityId, WeakEntity<Pane>>,
    status_bar: Entity<StatusBar>,
    pub(crate) modal_layer: Entity<ModalLayer>,
    titlebar_item: Option<AnyView>,
    notifications: Notifications,
    suppressed_notifications: HashSet<NotificationId>,
    bounds: Bounds<Pixels>,
    previous_dock_drag_coordinates: Option<Point<Pixels>>,
    bounds_save_task_queued: Option<Task<()>>,
    scheduled_serialization_task: Option<Task<()>>,
    serialization_task: Option<Task<()>>,
    serializable_items_tx: UnboundedSender<Box<dyn SerializableItemHandle>>,
    _items_serializer: Task<anyhow::Result<()>>,
    _project_subscription: Subscription,
    _window_activation_subscription: Subscription,
    _window_bounds_subscription: Subscription,
    _window_appearance_subscription: Subscription,
}

impl Workspace {
    pub fn create<V>(
        workspace_id: WorkspaceId,
        app_state: Arc<AppState>,
        window: &mut Window,
        cx: &mut Context<V>,
    ) -> Entity<Self>
    where
        V: 'static,
    {
        let project = cx.new({
            let fs = app_state.fs.clone();
            let languages = app_state.languages.clone();
            move |cx| Project::new(fs.clone(), languages.clone(), cx)
        });

        cx.new(|cx| {
            Self::new(
                Some(workspace_id),
                Some(app_state.session.read(cx).id().to_string()),
                app_state,
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

    pub fn app_state(&self) -> &Arc<AppState> {
        &self.app_state
    }

    pub fn database_id(&self) -> Option<WorkspaceId> {
        self.database_id
    }

    pub fn session_id(&self) -> Option<String> {
        self.session_id.clone()
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

    pub fn open(
        path: PathBuf,
        app_state: Arc<AppState>,
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
                .update(|cx| {
                    Project::open(
                        app_state.fs.clone(),
                        app_state.languages.clone(),
                        path.clone(),
                        cx,
                    )
                })
                .await?;
            let serialized_workspace = workspace_db.workspace_for_path(path.as_path());
            let workspace_id = if let Some(serialized_workspace) = serialized_workspace.as_ref() {
                serialized_workspace.id
            } else {
                workspace_db.next_id().await?
            };

            let (window, workspace) = if let Some(window) = window_to_replace {
                let workspace = window.update(cx, |root: &mut Root, window, cx| {
                    let session_id = app_state.session.read(cx).id().to_string();
                    let project = project.clone();
                    let app_state = app_state.clone();
                    let workspace = cx.new(|cx| {
                        Workspace::new(
                            Some(workspace_id),
                            Some(session_id),
                            app_state,
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
                let window_options = cx.update(|cx| {
                    let (window_bounds, display) = if let Some(workspace) =
                        serialized_workspace.as_ref()
                        && let Some(display) = workspace.display
                        && let Some(bounds) = workspace.window_bounds.as_ref()
                    {
                        (Some(bounds.0), Some(display))
                    } else if cx.active_window().is_some() {
                        (None, None)
                    } else if let Some((display, bounds)) =
                        persistence::read_default_window_bounds(&KeyValueStore::global(cx))
                    {
                        (Some(bounds), Some(display))
                    } else if cx.windows().is_empty() {
                        let mut bounds = Bounds::centered(None, DEFAULT_WINDOW_SIZE, cx);
                        bounds.origin.y -= gpui::px(36.0);
                        (Some(WindowBounds::Windowed(bounds)), None)
                    } else {
                        (None, None)
                    };

                    let mut options = build_window_options(display, cx);
                    options.window_bounds = window_bounds;
                    options
                });
                let window = cx.open_window(window_options, move |window, cx| {
                    let session_id = app_state.session.read(cx).id().to_string();
                    let workspace = cx.new(|cx| {
                        Workspace::new(
                            Some(workspace_id),
                            Some(session_id),
                            app_state,
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

            if let Some(serialized_workspace) = serialized_workspace {
                let restore_task = window.update(cx, |root: &mut Root, window, cx| {
                    root.workspace().update(cx, |workspace, cx| {
                        workspace.restore_workspace(serialized_workspace, window, cx)
                    })
                })?;
                restore_task.await.log_err();
            }

            if let Some(database_id) =
                workspace.read_with(cx, |workspace, _| workspace.database_id())
                && let Err(error) = workspace_db.update_activation_order(database_id).await
            {
                log::error!("Failed to update workspace activation order: {error}");
            }

            Ok(OpenResult { window, workspace })
        })
    }

    fn restore_workspace(
        &mut self,
        serialized_workspace: SerializedWorkspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        let project = self.project.clone();
        let workspace_id = serialized_workspace.id;
        let center_pane = serialized_workspace.center_pane;
        let docks = serialized_workspace.docks;

        cx.spawn_in(window, async move |workspace, cx| {
            let scan_complete =
                workspace.read_with(cx, |workspace, cx| workspace.worktree_scan_complete(cx))?;
            scan_complete.await;

            let pane =
                workspace.update_in(cx, |workspace, window, cx| workspace.add_pane(window, cx))?;
            let weak_pane = pane.downgrade();
            let deserialized_items = match center_pane
                .deserialize_to(&project, &weak_pane, workspace_id, workspace.clone(), cx)
                .await
            {
                Ok(items) => items,
                Err(error) => return Err(error),
            };
            let pane = if weak_pane.read_with(cx, |pane, _| pane.items_len() != 0)? {
                Some(pane)
            } else {
                None
            };

            let item_ids_by_kind = cx.update(|_, cx| {
                let mut item_ids_by_kind = HashMap::<&'static str, Vec<ItemId>>::new();
                for item in deserialized_items.into_iter().flatten() {
                    if let Some(serializable_item_handle) = item.to_serializable_item_handle(cx) {
                        item_ids_by_kind
                            .entry(serializable_item_handle.serialized_item_kind())
                            .or_default()
                            .push(item.item_id().as_u64());
                    }
                }
                item_ids_by_kind
            })?;

            let cleanup_tasks = workspace.update_in(cx, |workspace, window, cx| {
                if let Some(pane) = pane {
                    workspace.set_active_pane(&pane, window, cx);
                    cx.emit(WorkspaceEvent::PaneRestored(pane.clone()));
                    cx.focus_self(window);
                }
                workspace.set_dock_structure(docks, window, cx);
                cx.notify();

                item_ids_by_kind
                    .into_iter()
                    .map(|(item_kind, loaded_items)| {
                        SerializableItemRegistry::cleanup(
                            item_kind,
                            workspace_id,
                            loaded_items,
                            window,
                            cx,
                        )
                    })
                    .collect::<Vec<_>>()
            })?;

            for task in cleanup_tasks {
                task.await.log_err();
            }

            workspace.update_in(cx, |workspace, window, cx| {
                workspace.serialize_workspace_internal(window, cx).detach();
            })?;

            Ok(())
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

        let has_worktree = self.project.read(cx).root_worktree(cx).is_some();
        let has_dirty_items = self.pane.read(cx).items().any(|item| item.is_dirty(cx));
        let is_empty_workspace = !has_worktree && !has_dirty_items;
        if is_empty_workspace {
            open_mode = OpenMode::Activate;
        }

        let app_state = self.app_state().clone();

        cx.spawn_in(window, async move |workspace, cx| {
            let path = app_state.fs.canonicalize(&path).await.unwrap_or(path);
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
                .update(|_, cx| Workspace::open(path, app_state, requesting_window, open_mode, cx))?
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

        window.spawn(cx, async move |cx| {
            let (project_entry_id, build_item) = load_path.await?;
            pane.update_in(cx, |pane, window, cx| {
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
    }

    pub fn add_item_to_active_pane(
        &mut self,
        item: Box<dyn ItemHandle>,
        destination_index: Option<usize>,
        focus_item: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.pane.update(cx, |pane, cx| {
            pane.add_item(item, false, focus_item, true, destination_index, window, cx);
        });
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

    pub fn weak_handle(&self) -> WeakEntity<Self> {
        self.weak_self.clone()
    }

    pub fn set_titlebar_item(&mut self, item: AnyView, _: &mut Window, cx: &mut Context<Self>) {
        self.titlebar_item = Some(item);
        cx.notify();
    }

    pub fn titlebar_item(&self) -> Option<AnyView> {
        self.titlebar_item.clone()
    }

    pub fn pane(&self) -> &Entity<Pane> {
        &self.pane
    }

    fn add_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<Pane> {
        let workspace = self.weak_self.clone();
        let project = self.project.clone();
        let pane = cx.new(|cx| Pane::new(workspace, &project, window, cx));
        cx.subscribe_in(&pane, window, |workspace, pane, event, window, cx| {
            workspace.handle_pane_event(pane, event, window, cx);
        })
        .detach();

        let focus_handle = pane.read(cx).focus_handle(cx);
        window.focus(&focus_handle, cx);

        cx.emit(WorkspaceEvent::PaneAdded(pane.clone()));

        pane
    }

    fn set_active_pane(
        &mut self,
        pane: &Entity<Pane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pane.entity_id() != pane.entity_id() {
            let previous_pane = self.pane.clone();
            let previous_item_ids = previous_pane
                .read(cx)
                .items()
                .map(|item| item.item_id())
                .collect::<Vec<_>>();
            for item_id in previous_item_ids {
                let item_pane_is_previous = self
                    .panes_by_item
                    .get(&item_id)
                    .and_then(WeakEntity::upgrade)
                    .is_some_and(|pane| pane.entity_id() == previous_pane.entity_id());
                if item_pane_is_previous {
                    self.panes_by_item.remove(&item_id);
                }
            }
            self.pane = pane.clone();
        }

        self.status_bar.update(cx, |status_bar, cx| {
            status_bar.set_active_pane(pane, window, cx);
        });
        self.active_item_path_changed(cx);
    }

    pub fn active_item(&self, cx: &App) -> Option<Box<dyn ItemHandle>> {
        self.pane.read(cx).active_item()
    }

    pub fn active_item_as<I: 'static>(&self, cx: &App) -> Option<Entity<I>> {
        self.active_item(cx)?.to_any_view().downcast::<I>().ok()
    }

    fn active_project_path(&self, cx: &App) -> Option<ProjectPath> {
        self.active_item(cx).and_then(|item| item.project_path(cx))
    }

    fn active_item_path_changed(&mut self, cx: &mut Context<Self>) {
        cx.emit(WorkspaceEvent::ActiveItemChanged);
        let active_entry = self.active_project_path(cx);
        self.project.update(cx, |project, cx| {
            project.set_active_path(active_entry.clone(), cx);
        });
        if let Some(project_path) = &active_entry {
            let git_store = self.project.read(cx).git_store().clone();
            git_store.update(cx, |git_store, cx| {
                git_store.set_active_repo_for_path(project_path, cx);
            });
        }
    }

    fn handle_pane_event(
        &mut self,
        pane: &Entity<Pane>,
        event: &PaneEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let PaneEvent::AddItem { item } = event {
            item.added_to_pane(self, pane.clone(), window, cx);
        }

        if matches!(
            event,
            PaneEvent::ActivateItem { .. } | PaneEvent::ChangeItemTitle
        ) {
            self.active_item_path_changed(cx);
        }

        if matches!(event, PaneEvent::RemovedItem { .. }) {
            cx.emit(WorkspaceEvent::ActiveItemChanged);
        }

        self.serialize_workspace(window, cx);
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

    pub fn panel_size_state<T: Panel>(&self, cx: &App) -> Option<dock::PanelSizeState> {
        [self.left_dock.clone(), self.bottom_dock.clone()]
            .into_iter()
            .find_map(|dock| {
                let dock = dock.read(cx);
                let panel = dock.panel::<T>()?;
                dock.stored_panel_size_state(&panel)
            })
    }

    pub fn load_panel_size_state(
        &self,
        panel_key: &'static str,
        cx: &App,
    ) -> Option<dock::PanelSizeState> {
        let workspace_id = self
            .database_id()
            .map(|id| i64::from(id).to_string())
            .or_else(|| self.session_id())?;
        let kv_store = KeyValueStore::global(cx);
        let scope = kv_store.scoped(dock::PANEL_SIZE_STATE_KEY);
        scope
            .read(&format!("{workspace_id}:{panel_key}"))
            .log_err()
            .flatten()
            .and_then(|json| serde_json::from_str::<dock::PanelSizeState>(&json).log_err())
    }

    pub fn save_panel_size_state(
        &self,
        panel_key: &str,
        size_state: dock::PanelSizeState,
        cx: &mut App,
    ) {
        let Some(workspace_id) = self
            .database_id()
            .map(|id| i64::from(id).to_string())
            .or_else(|| self.session_id())
        else {
            return;
        };

        let kv_store = KeyValueStore::global(cx);
        let panel_key = panel_key.to_string();
        cx.background_spawn(async move {
            let scope = kv_store.scoped(dock::PANEL_SIZE_STATE_KEY);
            scope
                .write(
                    format!("{workspace_id}:{panel_key}"),
                    serde_json::to_string(&size_state)?,
                )
                .await
        })
        .detach_and_log_err(cx);
    }

    pub fn capture_dock_state(&self, _window: &Window, cx: &App) -> DockStructure {
        let left_dock = self.left_dock.read(cx);
        let left_visible = left_dock.is_open();
        let left_active_panel = left_dock.active_panel();
        let left_auto_hidden =
            !left_visible && left_active_panel.is_some_and(|panel| panel.auto_hidden(cx));
        let left_active_panel = left_active_panel.map(|panel| panel.persistent_name().to_string());

        let bottom_dock = self.bottom_dock.read(cx);
        let bottom_visible = bottom_dock.is_open();
        let bottom_active_panel = bottom_dock.active_panel();
        let bottom_auto_hidden =
            !bottom_visible && bottom_active_panel.is_some_and(|panel| panel.auto_hidden(cx));
        let bottom_active_panel =
            bottom_active_panel.map(|panel| panel.persistent_name().to_string());

        DockStructure {
            left: DockData {
                visible: left_visible,
                active_panel: left_active_panel,
                auto_hidden: left_auto_hidden,
            },
            bottom: DockData {
                visible: bottom_visible,
                active_panel: bottom_active_panel,
                auto_hidden: bottom_auto_hidden,
            },
        }
    }

    pub fn set_dock_structure(
        &self,
        docks: DockStructure,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let DockStructure { left, bottom } = docks;
        for (dock, data) in [(&self.left_dock, left), (&self.bottom_dock, bottom)] {
            dock.update(cx, |dock, cx| {
                dock.serialized_dock = Some(data);
                dock.restore_state(window, cx);
            });
        }
    }

    pub fn add_panel<T: Panel>(
        &mut self,
        panel: Entity<T>,
        position: DockPosition,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let persisted_size_state = self.load_panel_size_state(T::panel_key(), cx);
        let dock = self.dock_at_position(position).clone();
        dock.update(cx, move |dock, cx| {
            dock.add_panel(&panel, window, cx);
            if let Some(size_state) = persisted_size_state {
                dock.set_panel_size_state(&panel, size_state, cx);
            }
        });
    }

    fn root(&self, cx: &App) -> Option<PathBuf> {
        self.project().read(cx).root(cx)
    }

    pub fn worktree_scan_complete(&self, cx: &App) -> impl Future<Output = ()> + 'static + use<> {
        let scan_complete = self
            .project()
            .read(cx)
            .root_worktree(cx)
            .map(|worktree| worktree.read(cx).scan_complete());

        async move {
            if let Some(scan_complete) = scan_complete {
                scan_complete.await;
            }
        }
    }

    fn save_window_bounds(&self, window: &mut Window, cx: &mut App) -> Task<()> {
        let Some(display) = window.display(cx) else {
            return Task::ready(());
        };
        let Ok(display_uuid) = display.uuid() else {
            return Task::ready(());
        };

        let window_bounds = window.inner_window_bounds();
        let database_id = self.database_id;
        let has_root = self.root(cx).is_some();
        let workspace_db = WorkspaceDb::global(cx);
        let kv_store = KeyValueStore::global(cx);

        cx.background_spawn(async move {
            if !has_root {
                persistence::write_default_window_bounds(&kv_store, window_bounds, display_uuid)
                    .await
                    .log_err();
            }

            if let Some(database_id) = database_id {
                workspace_db
                    .set_window_open_status(
                        database_id,
                        SerializedWindowBounds(window_bounds),
                        display_uuid,
                    )
                    .await
                    .log_err();
            } else {
                persistence::write_default_window_bounds(&kv_store, window_bounds, display_uuid)
                    .await
                    .log_err();
            }
        })
    }

    pub fn flush_serialization(&mut self, window: &mut Window, cx: &mut App) -> Task<()> {
        self.scheduled_serialization_task.take();
        self.serialization_task.take();
        self.bounds_save_task_queued.take();

        let bounds_task = self.save_window_bounds(window, cx);
        let serialize_task = self.serialize_workspace_internal(window, cx);
        cx.spawn(async move |_| {
            bounds_task.await;
            serialize_task.await;
        })
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
        fn serialize_pane_handle(
            pane_handle: &Entity<Pane>,
            window: &mut Window,
            cx: &mut App,
        ) -> SerializedPane {
            let (items, active) = {
                let pane = pane_handle.read(cx);
                let active_item_id = pane.active_item().map(|item| item.item_id());
                let items = pane
                    .items()
                    .filter_map(|handle| {
                        let handle = handle.to_serializable_item_handle(cx)?;
                        let item_id = handle.item_id();
                        Some(SerializedItem {
                            kind: Arc::from(handle.serialized_item_kind()),
                            item_id: item_id.as_u64(),
                            active: Some(item_id) == active_item_id,
                            preview: pane.is_active_preview_item(item_id),
                        })
                    })
                    .collect::<Vec<_>>();

                (items, pane.has_focus(window, cx))
            };

            SerializedPane::new(items, active)
        }

        let docks = self.capture_dock_state(window, cx);

        let Some(database_id) = self.database_id() else {
            let kv_store = KeyValueStore::global(cx);
            return cx.background_spawn(async move {
                persistence::write_default_dock_state(&kv_store, docks)
                    .await
                    .log_err();
            });
        };

        if let Some(root_path) = self.root(cx) {
            let pane = self.pane.clone();
            let center_pane = serialize_pane_handle(&pane, window, cx);
            let serialized_workspace = SerializedWorkspace {
                id: database_id,
                location: root_path,
                center_pane,
                docks,
                window_bounds: Some(SerializedWindowBounds(window.window_bounds())),
                display: None,
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
        } else {
            let kv_store = KeyValueStore::global(cx);
            cx.background_spawn(async move {
                persistence::write_default_dock_state(&kv_store, docks)
                    .await
                    .log_err();
            })
        }
    }

    async fn serialize_items(
        workspace: &WeakEntity<Self>,
        items_rx: UnboundedReceiver<Box<dyn SerializableItemHandle>>,
        cx: &mut AsyncWindowContext,
    ) -> anyhow::Result<()> {
        const CHUNK_SIZE: usize = 200;

        let mut serializable_items = items_rx.ready_chunks(CHUNK_SIZE);

        while let Some(items_received) = serializable_items.next().await {
            let unique_items = items_received.into_iter().fold(
                HashMap::<EntityId, _>::default(),
                |mut items, item| {
                    items.entry(item.item_id()).or_insert(item);
                    items
                },
            );

            for (_, item) in unique_items {
                if let Ok(Some(task)) = workspace.update_in(cx, |workspace, window, cx| {
                    item.serialize(workspace, false, window, cx)
                }) {
                    cx.background_spawn(async move { task.await.log_err() })
                        .detach();
                }
            }

            cx.background_executor()
                .timer(SERIALIZATION_THROTTLE_TIME)
                .await;
        }

        Ok(())
    }

    pub(crate) fn enqueue_item_serialization(
        &mut self,
        item: Box<dyn SerializableItemHandle>,
    ) -> anyhow::Result<()> {
        self.serializable_items_tx
            .unbounded_send(item)
            .map_err(|err| anyhow!("failed to send serializable item over channel: {err}"))
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
        self.serialize_workspace(window, cx);
    }

    pub fn new(
        database_id: Option<WorkspaceId>,
        session_id: Option<String>,
        app_state: Arc<AppState>,
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

        let window_bounds_subscription =
            cx.observe_window_bounds(window, |workspace, window, cx| {
                if workspace.bounds_save_task_queued.is_some() {
                    return;
                }

                workspace.bounds_save_task_queued =
                    Some(cx.spawn_in(window, async move |this, cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(100))
                            .await;
                        if let Err(error) = this.update_in(cx, |this, window, cx| {
                            this.save_window_bounds(window, cx).detach();
                            this.bounds_save_task_queued.take();
                        }) {
                            log::debug!("Failed to save window bounds: {error}");
                        }
                    }));
                cx.notify();
            });

        let window_appearance_subscription =
            cx.observe_window_appearance(window, |_, window, cx| {
                let window_appearance = window.appearance();
                *SystemAppearance::global_mut(cx) = SystemAppearance(window_appearance.into());
                theme_settings::reload_theme(cx);
            });

        let workspace = cx.entity();
        let weak_handle = workspace.downgrade();
        let pane = cx.new(|cx| Pane::new(weak_handle.clone(), &project, window, cx));
        cx.subscribe_in(&pane, window, |workspace, pane, event, window, cx| {
            workspace.handle_pane_event(pane, event, window, cx);
        })
        .detach();

        let project_subscription = cx.subscribe_in(
            &project,
            window,
            |workspace: &mut Workspace, project, event, window, cx| match event {
                ProjectEvent::WorktreeAdded(worktree_id)
                | ProjectEvent::WorktreeUpdatedEntries(worktree_id, _) => {
                    if project
                        .read(cx)
                        .worktree_for_id(*worktree_id, cx)
                        .is_some_and(|worktree| worktree.read(cx).is_visible())
                    {
                        workspace.serialize_workspace(window, cx);
                    }
                }
                ProjectEvent::WorktreeRemoved(_) => {
                    workspace.serialize_workspace(window, cx);
                }
                ProjectEvent::DeletedEntry(_, entry_id) => {
                    workspace.pane.update(cx, |pane, cx| {
                        pane.handle_deleted_project_item(*entry_id, window, cx);
                    });
                }
                ProjectEvent::ActiveEntryChanged(_) | ProjectEvent::EntryMetadataUpdated(_) => {}
            },
        );

        let left_dock = cx.new(|cx| Dock::new(DockPosition::Left, weak_handle.clone(), window, cx));
        let bottom_dock =
            cx.new(|cx| Dock::new(DockPosition::Bottom, weak_handle.clone(), window, cx));

        if project.read(cx).root_worktree(cx).is_none()
            && let Some(default_docks) =
                persistence::read_default_dock_state(&KeyValueStore::global(cx))
        {
            for (dock, serialized_dock) in [
                (&left_dock, &default_docks.left),
                (&bottom_dock, &default_docks.bottom),
            ] {
                dock.update(cx, |dock, cx| {
                    dock.serialized_dock = Some(serialized_dock.clone());
                    dock.restore_state(window, cx);
                });
            }
        }

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
        let modal_layer = cx.new(|_| ModalLayer::new());

        let (serializable_items_tx, serializable_items_rx) =
            mpsc::unbounded::<Box<dyn SerializableItemHandle>>();
        let items_serializer = cx.spawn_in(window, async move |workspace, cx| {
            Self::serialize_items(&workspace, serializable_items_rx, cx).await
        });

        let this = Self {
            app_state,
            weak_self: weak_handle,
            registered_actions: Vec::default(),
            database_id,
            session_id,
            project,
            left_dock,
            bottom_dock,
            pane,
            panes_by_item: HashMap::default(),
            status_bar,
            modal_layer,
            titlebar_item: None,
            notifications: Notifications::default(),
            suppressed_notifications: HashSet::default(),
            bounds: Bounds::default(),
            previous_dock_drag_coordinates: None,
            bounds_save_task_queued: None,
            scheduled_serialization_task: None,
            serialization_task: None,
            serializable_items_tx,
            _items_serializer: items_serializer,
            _project_subscription: project_subscription,
            _window_activation_subscription: window_activation_subscription,
            _window_bounds_subscription: window_bounds_subscription,
            _window_appearance_subscription: window_appearance_subscription,
        };

        cx.defer_in(window, move |this, _, cx| {
            this.show_initial_notifications(cx);
        });

        let pane_focus_handle = this.pane.read(cx).focus_handle(cx);
        window.focus(&pane_focus_handle, cx);

        this
    }

    #[cfg(any(test, feature = "test"))]
    pub fn test_new(project: Entity<Project>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let app_state = AppState::global(cx);
        window.activate_window();
        let workspace = Self::new(
            None,
            Some(app_state.session.read(cx).id().to_string()),
            app_state,
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

    fn clear_recent_projects(
        &mut self,
        _: &actions::projects::ClearRecent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pane = self.pane.clone();
        let workspace_db = WorkspaceDb::global(cx);

        cx.spawn_in(window, async move |_, cx| {
            workspace_db.clear_recent_workspaces().await?;
            pane.update_in(cx, |pane, window, cx| {
                pane.reload_recent_workspaces(window, cx);
            })?;

            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    fn toggle_theme_mode(
        &mut self,
        _: &actions::theme::ToggleMode,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current_mode = cx
            .global::<SettingsStore>()
            .content()
            .theme
            .as_ref()
            .and_then(|theme| theme.mode);
        let new_mode = match current_mode {
            Some(ThemeAppearanceMode::Light) => ThemeAppearanceMode::Dark,
            Some(ThemeAppearanceMode::Dark) => ThemeAppearanceMode::Light,
            Some(ThemeAppearanceMode::System) | None => match cx.theme().appearance() {
                Appearance::Light => ThemeAppearanceMode::Dark,
                Appearance::Dark => ThemeAppearanceMode::Light,
            },
        };

        let fs = self.app_state.fs.clone();
        settings::update_settings_file(fs, cx, move |settings, _| {
            theme_settings::set_mode(settings, new_mode);
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
            self.serialize_workspace(window, cx);
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

    pub fn is_panel_open<T: Panel>(&self, cx: &App) -> bool {
        for dock in [self.left_dock.clone(), self.bottom_dock.clone()] {
            let dock = dock.read(cx);
            if dock.panel_index_for_type::<T>().is_some_and(|panel_index| {
                dock.is_open() && dock.active_panel_index() == Some(panel_index)
            }) {
                return true;
            }
        }

        false
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

    pub fn has_active_modal(&self, _: &mut Window, cx: &mut App) -> bool {
        self.modal_layer.read(cx).has_active_modal()
    }

    pub fn active_modal<V: ManagedView + 'static>(&self, cx: &App) -> Option<Entity<V>> {
        self.modal_layer.read(cx).active_modal()
    }

    pub fn toggle_modal<V: ModalView, B>(&mut self, window: &mut Window, cx: &mut App, build: B)
    where
        B: FnOnce(&mut Window, &mut Context<V>) -> V,
    {
        self.modal_layer.update(cx, |modal_layer, cx| {
            modal_layer.toggle_modal(window, cx, build);
        });
    }

    pub fn hide_modal(&mut self, window: &mut Window, cx: &mut App) -> bool {
        self.modal_layer
            .update(cx, |modal_layer, cx| modal_layer.hide_modal(window, cx))
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

impl EventEmitter<WorkspaceEvent> for Workspace {}

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
            .relative()
            .flex()
            .flex_col()
            .text_color(theme_colors.text)
            .font(ui_font)
            .text_ui(cx)
            .size_full()
            .overflow_hidden()
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
            .children(self.titlebar_item.clone())
            .child(
                gpui::div()
                    .id("workspace")
                    .bg(theme_colors.background)
                    .relative()
                    .flex()
                    .flex_1()
                    .overflow_hidden()
                    .border_y_1()
                    .border_color(theme_colors.border)
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

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    use fs::Fs;
    use fs::TempFs;
    use path::rel_path;
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

    pub(crate) fn init_test(app_state: Arc<AppState>, cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test_new(cx);
            cx.set_global(settings_store);
            theme::init(LoadThemes::JustBase, cx);
            crate::init(app_state, cx);
        });
    }

    async fn init_default_window_bounds(
        app_state: Arc<AppState>,
        cx: &mut TestAppContext,
    ) -> Bounds<Pixels> {
        let bounds = Bounds::new(
            gpui::point(gpui::px(100.0), gpui::px(100.0)),
            gpui::size(gpui::px(680.0), gpui::px(440.0)),
        );
        let workspace_db = cx.update(|cx| WorkspaceDb::global(cx));
        let workspace_id = workspace_db.next_id().await.unwrap();
        let root = cx
            .update(|cx| {
                cx.open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        ..WindowOptions::default()
                    },
                    move |window, cx| {
                        cx.new(|cx| {
                            Root::new(Workspace::create(workspace_id, app_state, window, cx))
                        })
                    },
                )
            })
            .unwrap();
        root.update(cx, |root, window, cx| {
            root.workspace().update(cx, |workspace, cx| {
                workspace.flush_serialization(window, cx)
            })
        })
        .unwrap()
        .await;
        root.update(cx, |_, window, _| window.remove_window())
            .unwrap();

        assert!(cx.windows().is_empty());

        bounds
    }

    #[gpui::test]
    async fn test_tracking_active_path(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state.clone(), cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                "first.toml": "",
                "second.toml": "",
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs, &project_path, cx).await;
        let (workspace, cx) = build_workspace(&project, cx);
        let pane = workspace.update_in(cx, |workspace, _, _| workspace.pane().clone());

        let (worktree_id, first_entry_id, first_path, second_entry_id, second_path) = project
            .update(cx, |project, cx| {
                let worktree = project.root_worktree(cx).unwrap();
                let worktree = worktree.read(cx);
                let first_entry = worktree.entry_for_path(rel_path("first.toml")).unwrap();
                let second_entry = worktree.entry_for_path(rel_path("second.toml")).unwrap();

                (
                    worktree.id(),
                    first_entry.id,
                    first_entry.path.clone(),
                    second_entry.id,
                    second_entry.path.clone(),
                )
            });

        let first_project_item = cx.new(move |_| TestProjectItem {
            entry_id: Some(first_entry_id),
            project_path: Some(ProjectPath {
                worktree_id,
                path: first_path,
            }),
            is_dirty: false,
        });
        let first_item = cx.new(|cx| {
            let mut item = TestItem::new(cx).with_label("First");
            item.project_items.push(first_project_item);
            item
        });

        workspace.update_in(cx, |workspace, window, cx| {
            workspace.add_item_to_active_pane(Box::new(first_item), None, true, window, cx);
        });
        project.update(cx, |project, _| {
            assert_eq!(project.active_entry(), Some(first_entry_id));
        });

        let second_project_item = cx.new(move |_| TestProjectItem {
            entry_id: Some(second_entry_id),
            project_path: Some(ProjectPath {
                worktree_id,
                path: second_path,
            }),
            is_dirty: false,
        });
        let second_item = cx.new(|cx| {
            let mut item = TestItem::new(cx).with_label("Second");
            item.project_items.push(second_project_item);
            item
        });

        workspace.update_in(cx, |workspace, window, cx| {
            workspace.add_item_to_active_pane(Box::new(second_item), None, true, window, cx);
        });
        project.update(cx, |project, _| {
            assert_eq!(project.active_entry(), Some(second_entry_id));
        });

        pane.update_in(cx, |pane, window, cx| {
            pane.activate_item(0, true, true, window, cx);
        });
        project.update(cx, |project, _| {
            assert_eq!(project.active_entry(), Some(first_entry_id));
        });

        pane.update_in(cx, |pane, window, cx| {
            pane.close_active_item(&actions::pane::CloseActiveItem::default(), window, cx)
        })
        .await
        .unwrap();
        project.update(cx, |project, _| {
            assert_eq!(project.active_entry(), Some(second_entry_id));
        });
    }

    #[gpui::test]
    async fn test_concurrent_equivalent_workspace_opens_coalesce_to_canonical_root(
        cx: &mut TestAppContext,
    ) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state.clone(), cx);

        let project = cx.new(|cx| Project::new(temp_fs.clone(), app_state.languages.clone(), cx));
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
            workspace.project().read(cx).root_worktree(cx).unwrap()
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
            .unwrap();

        assert_eq!(current_root, Some(canonical_project_path.clone()));
        assert!(
            recent_workspaces
                .iter()
                .any(|(_, location, _)| location == &canonical_project_path)
        );
        assert!(
            recent_workspaces
                .iter()
                .all(|(_, location, _)| location != &alternate_project_path)
        );
    }

    #[gpui::test]
    async fn test_remove_worktree_invalidates_pending_direct_project_open(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state.clone(), cx);
        let project = cx.new(|cx| Project::new(temp_fs.clone(), app_state.languages.clone(), cx));
        let (workspace, cx) = build_workspace(&project, cx);

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
        let workspace = first_open.await.unwrap();

        workspace
            .read_with(cx, |workspace, cx| workspace.worktree_scan_complete(cx))
            .await;
        let worktree = workspace.update_in(cx, |workspace, _, cx| {
            workspace.project().read(cx).root_worktree(cx).unwrap()
        });
        worktree.flush_fs_events(cx).await;

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
            workspace.project().read(cx).root_worktree(cx)
        });
        assert!(current_worktree.is_none());
    }

    #[gpui::test]
    fn test_docks_are_disabled_on_welcome_page(cx: &mut TestAppContext) {
        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state.clone(), cx);
        let project = cx.new(|cx| Project::new(temp_fs.clone(), app_state.languages.clone(), cx));
        let (workspace, cx) = build_workspace(&project, cx);

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

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state.clone(), cx);
        let project = cx.new(|cx| Project::new(temp_fs.clone(), app_state.languages.clone(), cx));
        let (workspace, cx) = build_workspace(&project, cx);

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
            workspace.project().read(cx).root_worktree(cx).unwrap()
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
            .unwrap();
        assert!(
            recent_workspaces
                .iter()
                .any(|(_, location, _)| location == &project_path)
        );
    }

    #[gpui::test]
    fn test_toggle_docks_and_panels(cx: &mut TestAppContext) {
        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state.clone(), cx);

        let project = cx.new(|cx| Project::new(temp_fs.clone(), app_state.languages.clone(), cx));
        let (workspace, cx) = build_workspace(&project, cx);

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
    fn test_panel_size_state_persistence(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state.clone(), cx);

        let project = cx.new(|cx| Project::new(temp_fs.clone(), app_state.languages.clone(), cx));
        let (workspace, cx) = build_workspace(&project, cx);

        let workspace_id = workspace.update(cx, |workspace, _| {
            workspace.set_random_database_id();
            workspace.bounds.size.width = gpui::px(640.0);
            workspace.database_id().unwrap()
        });

        workspace.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| TestPanel::new(100, cx));
            workspace.add_panel(panel, DockPosition::Left, window, cx);
            workspace.toggle_dock(DockPosition::Left, window, cx);
        });

        workspace.update_in(cx, |workspace, window, cx| {
            workspace.resize_left_dock(gpui::px(350.0), window, cx);
        });

        cx.run_until_parked();

        let persisted = workspace.read_with(cx, |workspace, cx| {
            workspace.load_panel_size_state(TestPanel::panel_key(), cx)
        });
        assert_eq!(
            persisted.and_then(|state| state.size),
            Some(gpui::px(350.0))
        );

        let (root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(Workspace::create(
                workspace_id,
                app_state.clone(),
                window,
                cx,
            ))
        });
        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());

        workspace.update_in(cx, |workspace, window, cx| {
            let panel = cx.new(|cx| TestPanel::new(100, cx));
            workspace.add_panel(panel, DockPosition::Left, window, cx);

            let left_dock = workspace.left_dock().read(cx);
            let size_state = left_dock
                .panel::<TestPanel>()
                .and_then(|panel| left_dock.stored_panel_size_state(&panel));
            assert_eq!(
                size_state.and_then(|state| state.size),
                Some(gpui::px(350.0))
            );
        });
    }

    #[gpui::test]
    fn test_remove_last_item_refocuses_pane(cx: &mut TestAppContext) {
        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state.clone(), cx);

        let project = cx.new(|cx| Project::new(temp_fs.clone(), app_state.languages.clone(), cx));
        let (workspace, cx) = build_workspace(&project, cx);
        let pane = workspace.update_in(cx, |workspace, _, _| workspace.pane().clone());
        let item = cx.new(TestItem::new);
        let item_id = Entity::entity_id(&item);

        workspace.update_in(cx, |workspace, window, cx| {
            workspace.add_item_to_active_pane(Box::new(item), None, true, window, cx);
        });

        pane.update_in(cx, |pane, window, cx| {
            assert!(pane.has_focus(window, cx));
            pane.remove_item(item_id, true, true, window, cx);
            assert!(pane.focus_handle(cx).contains_focused(window, cx));
        });

        workspace.update_in(cx, |_, window, cx| {
            assert!(window.is_action_available(&actions::workspace::NewWindow, cx));
            assert!(window.is_action_available(&actions::workspace::Open::default(), cx));
            assert!(window.is_action_available(&actions::workspace::CloseProject, cx));
        });
    }

    #[gpui::test]
    async fn test_close_all_items(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state, cx);

        temp_fs.insert_tree(path!("project"), Value::default());

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let (workspace, cx) = build_workspace(&project, cx);
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

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state, cx);

        temp_fs.insert_tree(path!("project"), Value::default());

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let (workspace, cx) = build_workspace(&project, cx);
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

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state, cx);

        temp_fs.insert_tree(path!("project"), Value::default());

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let (workspace, cx) = build_workspace(&project, cx);
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

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state, cx);

        temp_fs.insert_tree(path!("project"), Value::default());

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let (workspace, cx) = build_workspace(&project, cx);
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

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state, cx);

        temp_fs.insert_tree(path!("project"), Value::default());

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let (workspace, cx) = build_workspace(&project, cx);
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

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state, cx);

        temp_fs.insert_tree(path!("project"), Value::default());

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let (workspace, cx) = build_workspace(&project, cx);
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

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state, cx);

        temp_fs.insert_tree(path!("project"), Value::default());

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let (workspace, cx) = build_workspace(&project, cx);
        let pane = workspace.update_in(cx, |workspace, _, _| workspace.pane().clone());

        add_labeled_item(&pane, "First", false, cx);
        add_labeled_item(&pane, "Second", false, cx);
        add_labeled_item(&pane, "Third", false, cx);
        add_labeled_item(&pane, "Fourth", false, cx);
        assert_item_labels(&pane, ["First", "Second", "Third", "Fourth*"], cx);
        cx.run_until_parked();

        let first_tab_bounds = cx.debug_bounds("TAB-0").unwrap();
        let third_tab_bounds = cx.debug_bounds("TAB-2").unwrap();

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

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state.clone(), cx);

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
        let (workspace, cx) = build_workspace(&project, cx);

        let first_worktree_id = workspace
            .update_in(cx, |workspace, _, cx| {
                workspace
                    .project()
                    .read(cx)
                    .root_worktree(cx)
                    .map(|worktree| worktree.read(cx).id())
            })
            .unwrap();

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
            workspace.project().read(cx).root_worktree(cx).unwrap()
        });
        worktree.flush_fs_events(cx).await;

        let (second_worktree_id, current_root) = workspace.update_in(cx, |workspace, _, cx| {
            let project = workspace.project().read(cx);
            (
                project
                    .root_worktree(cx)
                    .map(|worktree| worktree.read(cx).id()),
                project.root(cx),
            )
        });

        assert_eq!(Some(first_worktree_id), second_worktree_id);
        assert_eq!(current_root, Some(project_path));
    }

    #[gpui::test]
    async fn test_opening_same_workspace_in_new_window_with_activate(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state.clone(), cx);

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
            Root::new(Workspace::create(workspace_id, app_state, window, cx))
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
            workspace.project().read(cx).root_worktree(cx).unwrap()
        });
        worktree.flush_fs_events(cx).await;

        assert_eq!(cx.windows().len(), 1);
        let root_window_id = cx.update(|window, _| window.window_handle().window_id());

        let workspace_db = cx.update(|_, cx| WorkspaceDb::global(cx));
        let workspace_id = workspace_db.next_id().await.unwrap();
        let app_state = cx.update(|_, cx| AppState::global(cx));
        let (empty_root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(Workspace::create(workspace_id, app_state, window, cx))
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

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state.clone(), cx);

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
            Root::new(Workspace::create(workspace_id, app_state, window, cx))
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
            workspace.project().read(cx).root_worktree(cx).unwrap()
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
            workspace.project().read(cx).root_worktree(cx).unwrap()
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

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state.clone(), cx);

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
        let (workspace, cx) = build_workspace(&project, cx);

        let first_worktree_id = workspace
            .update_in(cx, |workspace, _, cx| {
                workspace
                    .project()
                    .read(cx)
                    .root_worktree(cx)
                    .map(|worktree| worktree.read(cx).id())
            })
            .unwrap();

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
            workspace.project().read(cx).root_worktree(cx).unwrap()
        });
        worktree.flush_fs_events(cx).await;

        let (second_worktree_id, current_root) = workspace.update_in(cx, |workspace, _, cx| {
            let project = workspace.project().read(cx);
            (
                project
                    .root_worktree(cx)
                    .map(|worktree| worktree.read(cx).id()),
                project.root(cx),
            )
        });

        assert_eq!(Some(first_worktree_id), second_worktree_id);
        assert_eq!(current_root, Some(canonical_project_path));
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[gpui::test]
    async fn test_opening_symlinked_workspace_path_reuses_current_worktree(
        cx: &mut TestAppContext,
    ) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state.clone(), cx);

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
        let (workspace, cx) = build_workspace(&project, cx);

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
                project
                    .root_worktree(cx)
                    .map(|worktree| worktree.read(cx).id()),
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
            workspace.project().read(cx).root_worktree(cx).unwrap()
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
                project
                    .root_worktree(cx)
                    .map(|worktree| worktree.read(cx).id()),
                project.root(cx),
            )
        });

        let workspace_db = cx.update(|_, cx| WorkspaceDb::global(cx));
        let recent_workspaces = workspace_db
            .recent_workspaces_on_disk(temp_fs.as_ref())
            .await
            .unwrap();

        assert_eq!(first_worktree_id, second_worktree_id);
        assert_eq!(current_root, Some(canonical_project_path.clone()));
        assert!(
            recent_workspaces
                .iter()
                .any(|(_, location, _)| location == &canonical_project_path)
        );
        assert!(
            recent_workspaces
                .iter()
                .all(|(_, location, _)| location != &alias_project_path)
        );
    }

    #[gpui::test]
    async fn test_opening_different_workspace_replaces_current_worktree(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state.clone(), cx);

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
        let (workspace, cx) = build_workspace(&project, cx);

        let first_worktree_id = workspace
            .update_in(cx, |workspace, _, cx| {
                workspace
                    .project()
                    .read(cx)
                    .root_worktree(cx)
                    .map(|worktree| worktree.read(cx).id())
            })
            .unwrap();

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
            workspace.project().read(cx).root_worktree(cx).unwrap()
        });
        worktree.flush_fs_events(cx).await;

        let (current_worktree_id, current_root) = workspace.update_in(cx, |workspace, _, cx| {
            let project = workspace.project().read(cx);
            (
                project
                    .root_worktree(cx)
                    .map(|worktree| worktree.read(cx).id()),
                project.root(cx),
            )
        });

        assert_ne!(current_worktree_id, Some(first_worktree_id));
        assert_eq!(current_root, Some(second_path));
    }

    #[gpui::test]
    async fn test_window_bounds_on_initial_launch(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state.clone(), cx);

        temp_fs.insert_tree(path!("project"), json!(null));
        let project_path = temp_fs.path().join(path!("project"));
        let result = cx
            .update(|cx| Workspace::open(project_path, app_state, None, OpenMode::NewWindow, cx))
            .await
            .unwrap();

        assert_eq!(
            result
                .window
                .update(cx, |_, window, _| window.window_bounds())
                .unwrap()
                .get_bounds()
                .size,
            DEFAULT_WINDOW_SIZE
        );
    }

    #[gpui::test]
    async fn test_window_bounds_restore_saved_default(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state.clone(), cx);
        temp_fs.insert_tree(path!("project"), json!(null));

        let default_bounds = init_default_window_bounds(app_state.clone(), cx).await;
        let project_path = temp_fs.path().join(path!("project"));
        let result = cx
            .update(|cx| Workspace::open(project_path, app_state, None, OpenMode::NewWindow, cx))
            .await
            .unwrap();

        assert_eq!(
            result
                .window
                .update(cx, |_, window, _| window.window_bounds())
                .unwrap(),
            WindowBounds::Windowed(default_bounds)
        );
    }

    #[gpui::test]
    async fn test_window_bounds_cascade_on_new_window(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state.clone(), cx);

        let workspace_db = cx.update(|cx| WorkspaceDb::global(cx));
        init_default_window_bounds(app_state.clone(), cx).await;

        let workspace_id = workspace_db.next_id().await.unwrap();
        let active_bounds = Bounds::new(
            gpui::point(gpui::px(100.0), gpui::px(100.0)),
            gpui::size(gpui::px(860.0), gpui::px(540.0)),
        );
        cx.update(|cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(active_bounds)),
                    ..WindowOptions::default()
                },
                {
                    let app_state = app_state.clone();
                    move |window, cx| {
                        window.activate_window();
                        cx.new(|cx| {
                            Root::new(Workspace::create(workspace_id, app_state, window, cx))
                        })
                    }
                },
            )
        })
        .unwrap();

        cx.update(|cx| open_new(app_state, cx)).await.unwrap();

        let new_window = cx.update(|cx| cx.active_window().unwrap().downcast::<Root>().unwrap());
        let cascade_offset = gpui::point(gpui::px(25.0), gpui::px(25.0));
        let cascaded_bounds =
            Bounds::new(active_bounds.origin + cascade_offset, active_bounds.size);
        assert_eq!(cx.windows().len(), 2);
        assert_eq!(
            new_window
                .update(cx, |_, window, _| window.window_bounds())
                .unwrap(),
            WindowBounds::Windowed(cascaded_bounds)
        );
    }

    #[gpui::test]
    async fn test_window_bounds_cascade_on_new_project_window(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state.clone(), cx);

        temp_fs.insert_tree(path!("project"), json!(null));
        let project_path = temp_fs.path().join(path!("project"));
        let workspace_db = cx.update(|cx| WorkspaceDb::global(cx));
        init_default_window_bounds(app_state.clone(), cx).await;

        let active_bounds = Bounds::new(
            gpui::point(gpui::px(100.0), gpui::px(100.0)),
            gpui::size(gpui::px(860.0), gpui::px(540.0)),
        );
        let active_workspace_id = workspace_db.next_id().await.unwrap();
        cx.update(|cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(active_bounds)),
                    ..WindowOptions::default()
                },
                {
                    let app_state = app_state.clone();
                    move |window, cx| {
                        window.activate_window();
                        cx.new(|cx| {
                            Root::new(Workspace::create(
                                active_workspace_id,
                                app_state,
                                window,
                                cx,
                            ))
                        })
                    }
                },
            )
        })
        .unwrap();

        let result = cx
            .update(|cx| Workspace::open(project_path, app_state, None, OpenMode::NewWindow, cx))
            .await
            .unwrap();

        let cascade_offset = gpui::point(gpui::px(25.0), gpui::px(25.0));
        let cascaded_bounds =
            Bounds::new(active_bounds.origin + cascade_offset, active_bounds.size);
        assert_eq!(
            result
                .window
                .update(cx, |_, window, _| window.window_bounds())
                .unwrap(),
            WindowBounds::Windowed(cascaded_bounds)
        );
    }

    #[gpui::test]
    async fn test_window_bounds_restore_saved_workspace(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state.clone(), cx);

        temp_fs.insert_tree(path!("project"), json!(null));
        let project_path = temp_fs.path().join(path!("project"));
        init_default_window_bounds(app_state.clone(), cx).await;
        let workspace_db = cx.update(|cx| WorkspaceDb::global(cx));

        let active_bounds = Bounds::new(
            gpui::point(gpui::px(100.0), gpui::px(100.0)),
            gpui::size(gpui::px(860.0), gpui::px(540.0)),
        );
        let active_workspace_id = workspace_db.next_id().await.unwrap();
        let active_window = cx
            .update(|cx| {
                cx.open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(active_bounds)),
                        ..WindowOptions::default()
                    },
                    {
                        let app_state = app_state.clone();
                        move |window, cx| {
                            window.activate_window();
                            cx.new(|cx| {
                                Root::new(Workspace::create(
                                    active_workspace_id,
                                    app_state,
                                    window,
                                    cx,
                                ))
                            })
                        }
                    },
                )
            })
            .unwrap();
        let display_uuid = active_window
            .update(cx, |_, window, cx| {
                window.display(cx).unwrap().uuid().unwrap()
            })
            .unwrap();

        let workspace_id = workspace_db.next_id().await.unwrap();
        workspace_db
            .save_workspace(SerializedWorkspace {
                id: workspace_id,
                location: project_path.clone(),
                center_pane: SerializedPane::default(),
                docks: DockStructure::default(),
                window_bounds: None,
                display: None,
                session_id: None,
                window_id: None,
            })
            .await;

        let saved_workspace_bounds = Bounds::new(
            gpui::point(gpui::px(200.0), gpui::px(200.0)),
            gpui::size(gpui::px(500.0), gpui::px(500.0)),
        );
        workspace_db
            .set_window_open_status(
                workspace_id,
                SerializedWindowBounds(WindowBounds::Windowed(saved_workspace_bounds)),
                display_uuid,
            )
            .await
            .unwrap();

        let result = cx
            .update(|cx| Workspace::open(project_path, app_state, None, OpenMode::NewWindow, cx))
            .await
            .unwrap();

        assert_eq!(
            result
                .window
                .update(cx, |_, window, _| window.window_bounds())
                .unwrap(),
            WindowBounds::Windowed(saved_workspace_bounds)
        );
    }

    #[gpui::test]
    async fn test_center_pane_deserialization_preserves_item_order(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state, cx);
        cx.update(register_serializable_item::<TestItem>);

        temp_fs.insert_tree(path!("project"), Value::default());

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let (workspace, cx) = build_workspace(&project, cx);
        let pane = workspace.update_in(cx, |workspace, _, _| workspace.pane().clone());
        let workspace_db = cx.update(|_, cx| WorkspaceDb::global(cx));
        let workspace_id = workspace_db.next_id().await.unwrap();
        let serialized_pane = SerializedPane::new(
            vec![
                SerializedItem::new("TestItem", 1, false, false),
                SerializedItem::new("TestItem", 2, false, false),
                SerializedItem::new("TestItem", 3, true, false),
                SerializedItem::new("TestItem", 4, false, false),
                SerializedItem::new("TestItem", 5, false, true),
            ],
            true,
        );
        let weak_pane = pane.downgrade();
        let weak_workspace = workspace.downgrade();

        let deserialized_items = workspace
            .update_in(cx, |_, window, cx| {
                cx.spawn_in(window, async move |_, cx| {
                    serialized_pane
                        .deserialize_to(&project, &weak_pane, workspace_id, weak_workspace, cx)
                        .await
                })
            })
            .await
            .unwrap();
        let expected_item_ids = deserialized_items
            .iter()
            .flatten()
            .map(|item| item.item_id())
            .collect::<Vec<_>>();
        let pane_item_ids = pane.read_with(cx, |pane, _| {
            pane.items().map(|item| item.item_id()).collect::<Vec<_>>()
        });

        assert_eq!(pane_item_ids, expected_item_ids);
        let active_item_id =
            pane.read_with(cx, |pane, _| pane.active_item().map(|item| item.item_id()));
        let preview_item_id =
            pane.read_with(cx, |pane, _| pane.preview_item().map(|item| item.item_id()));
        assert_eq!(active_item_id.as_ref(), expected_item_ids.get(2));
        assert_eq!(preview_item_id.as_ref(), expected_item_ids.get(4));
    }
}
