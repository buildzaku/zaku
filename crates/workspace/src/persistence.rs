pub(crate) mod model;

use anyhow::Context;
use gpui::{App, Bounds, Task, WindowBounds, WindowId};
use jiff::{SignedDuration, Timestamp, civil::DateTime, tz::TimeZone};
use std::path::{Path, PathBuf};

use db::{
    Bind, Column, Connection, Row, Statement, StaticColumnCount, ThreadSafeConnection,
    kv::KeyValueStore, query, sql_macros::sql,
};
use fs::Fs;
use serde::{Deserialize, Serialize};
use util::ResultExt;
use uuid::Uuid;

use self::model::{
    DockStructure, PaneId, SerializedItem, SerializedPane, SerializedWorkspace, SessionWorkspace,
};
use crate::{ItemId, WorkspaceId};

#[derive(Copy, Clone, Debug, PartialEq, Default)]
pub struct SerializedWindowBounds(pub WindowBounds);

impl StaticColumnCount for SerializedWindowBounds {
    fn column_count() -> usize {
        5
    }
}

impl Bind for SerializedWindowBounds {
    fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
        match self.0 {
            WindowBounds::Windowed(bounds) => {
                let next_index = statement.bind(&"Windowed", start_index)?;
                statement.bind(
                    &(
                        f32::from(bounds.origin.x),
                        f32::from(bounds.origin.y),
                        f32::from(bounds.size.width),
                        f32::from(bounds.size.height),
                    ),
                    next_index,
                )
            }
            WindowBounds::Maximized(bounds) => {
                let next_index = statement.bind(&"Maximized", start_index)?;
                statement.bind(
                    &(
                        f32::from(bounds.origin.x),
                        f32::from(bounds.origin.y),
                        f32::from(bounds.size.width),
                        f32::from(bounds.size.height),
                    ),
                    next_index,
                )
            }
            WindowBounds::Fullscreen(bounds) => {
                let next_index = statement.bind(&"FullScreen", start_index)?;
                statement.bind(
                    &(
                        f32::from(bounds.origin.x),
                        f32::from(bounds.origin.y),
                        f32::from(bounds.size.width),
                        f32::from(bounds.size.height),
                    ),
                    next_index,
                )
            }
        }
    }
}

impl Column for SerializedWindowBounds {
    fn column(row: &mut Row<'_, '_>, start_index: i32) -> anyhow::Result<(Self, i32)> {
        let (window_state, next_index) = String::column(row, start_index)?;
        let ((origin_x, origin_y, width, height), _): ((f32, f32, f32, f32), _) =
            Column::column(row, next_index)?;
        let bounds = Bounds {
            origin: gpui::point(gpui::px(origin_x), gpui::px(origin_y)),
            size: gpui::size(gpui::px(width), gpui::px(height)),
        };

        let status = match window_state.as_str() {
            "Windowed" | "Fixed" => SerializedWindowBounds(WindowBounds::Windowed(bounds)),
            "Maximized" => SerializedWindowBounds(WindowBounds::Maximized(bounds)),
            "FullScreen" => SerializedWindowBounds(WindowBounds::Fullscreen(bounds)),
            _ => anyhow::bail!("Window State did not have a valid string"),
        };

        Ok((status, next_index + 4))
    }
}

const DEFAULT_WINDOW_BOUNDS_KEY: &str = "default_window_bounds";

pub(crate) fn read_default_window_bounds(kv_store: &KeyValueStore) -> Option<(Uuid, WindowBounds)> {
    let json_str = kv_store
        .read_kv(DEFAULT_WINDOW_BOUNDS_KEY)
        .log_err()
        .flatten()?;

    let (display_uuid, persisted) =
        serde_json::from_str::<(Uuid, WindowBoundsJson)>(&json_str).ok()?;
    Some((display_uuid, persisted.into()))
}

pub(crate) async fn write_default_window_bounds(
    kv_store: &KeyValueStore,
    bounds: WindowBounds,
    display_uuid: Uuid,
) -> anyhow::Result<()> {
    let persisted = WindowBoundsJson::from(bounds);
    let json_str = serde_json::to_string(&(display_uuid, persisted))?;
    kv_store
        .write_kv(DEFAULT_WINDOW_BOUNDS_KEY.to_string(), json_str)
        .await?;
    Ok(())
}

const DEFAULT_DOCK_STATE_KEY: &str = "default_dock_state";

pub(crate) fn read_default_dock_state(kv_store: &KeyValueStore) -> Option<DockStructure> {
    let json_str = kv_store
        .read_kv(DEFAULT_DOCK_STATE_KEY)
        .log_err()
        .flatten()?;

    serde_json::from_str::<DockStructure>(&json_str).ok()
}

pub(crate) async fn write_default_dock_state(
    kv_store: &KeyValueStore,
    docks: DockStructure,
) -> anyhow::Result<()> {
    let json_str = serde_json::to_string(&docks)?;
    kv_store
        .write_kv(DEFAULT_DOCK_STATE_KEY.to_string(), json_str)
        .await?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
pub enum WindowBoundsJson {
    Windowed {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    Maximized {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    Fullscreen {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
}

impl From<WindowBounds> for WindowBoundsJson {
    fn from(bounds: WindowBounds) -> Self {
        match bounds {
            WindowBounds::Windowed(bounds) => {
                let origin = bounds.origin;
                let size = bounds.size;
                WindowBoundsJson::Windowed {
                    x: f32::from(origin.x),
                    y: f32::from(origin.y),
                    width: f32::from(size.width),
                    height: f32::from(size.height),
                }
            }
            WindowBounds::Maximized(bounds) => {
                let origin = bounds.origin;
                let size = bounds.size;
                WindowBoundsJson::Maximized {
                    x: f32::from(origin.x),
                    y: f32::from(origin.y),
                    width: f32::from(size.width),
                    height: f32::from(size.height),
                }
            }
            WindowBounds::Fullscreen(bounds) => {
                let origin = bounds.origin;
                let size = bounds.size;
                WindowBoundsJson::Fullscreen {
                    x: f32::from(origin.x),
                    y: f32::from(origin.y),
                    width: f32::from(size.width),
                    height: f32::from(size.height),
                }
            }
        }
    }
}

impl From<WindowBoundsJson> for WindowBounds {
    fn from(bounds: WindowBoundsJson) -> Self {
        match bounds {
            WindowBoundsJson::Windowed {
                x,
                y,
                width,
                height,
            } => WindowBounds::Windowed(Bounds {
                origin: gpui::point(gpui::px(x), gpui::px(y)),
                size: gpui::size(gpui::px(width), gpui::px(height)),
            }),
            WindowBoundsJson::Maximized {
                x,
                y,
                width,
                height,
            } => WindowBounds::Maximized(Bounds {
                origin: gpui::point(gpui::px(x), gpui::px(y)),
                size: gpui::size(gpui::px(width), gpui::px(height)),
            }),
            WindowBoundsJson::Fullscreen {
                x,
                y,
                width,
                height,
            } => WindowBounds::Fullscreen(Bounds {
                origin: gpui::point(gpui::px(x), gpui::px(y)),
                size: gpui::size(gpui::px(width), gpui::px(height)),
            }),
        }
    }
}

pub struct WorkspaceDb(ThreadSafeConnection);

impl WorkspaceDb {
    query! {
        pub async fn next_id() -> anyhow::Result<WorkspaceId> {
            INSERT INTO workspace DEFAULT VALUES RETURNING id
        }
    }

    pub async fn save_workspace(&self, workspace: SerializedWorkspace) {
        if let Err(error) = self
            .0
            .write(move |connection| {
                let workspace_location = workspace.location.as_path();
                connection.with_savepoint("save_workspace", || {
                    connection
                        .exec_bound(sql!(
                            DELETE FROM pane
                            WHERE workspace_id = ?1
                        ))
                        .context("failed to prepare old pane cleanup query")
                        .and_then(|mut f| f(workspace.id))
                        .context("failed to clear old pane")?;

                    connection
                        .exec_bound(sql!(
                            DELETE FROM workspace
                            WHERE id != ?1 AND location = ?2
                        ))
                        .context("failed to prepare old workspace location cleanup query")
                        .and_then(|mut f| f((workspace.id, workspace_location)))
                        .context("failed to clear old workspace locations")?;

                    connection
                        .exec_bound(sql!(
                            INSERT INTO workspace(
                                id,
                                location,
                                left_dock_open,
                                left_dock_active_panel,
                                bottom_dock_open,
                                bottom_dock_active_panel,
                                session_id,
                                window_id,
                                activation_order,
                                timestamp
                            )
                            VALUES (
                                ?1,
                                ?2,
                                ?3,
                                ?4,
                                ?5,
                                ?6,
                                ?7,
                                ?8,
                                (SELECT COALESCE(MAX(activation_order), 0) + 1 FROM workspace),
                                CURRENT_TIMESTAMP
                            )
                            ON CONFLICT(id)
                            DO UPDATE SET
                                location = excluded.location,
                                left_dock_open = excluded.left_dock_open,
                                left_dock_active_panel = excluded.left_dock_active_panel,
                                bottom_dock_open = excluded.bottom_dock_open,
                                bottom_dock_active_panel = excluded.bottom_dock_active_panel,
                                session_id = excluded.session_id,
                                window_id = excluded.window_id,
                                timestamp = CURRENT_TIMESTAMP
                        ))
                        .context("failed to prepare workspace upsert query")
                        .and_then(|mut f| {
                            f((
                                workspace.id,
                                workspace_location,
                                workspace.docks.clone(),
                                workspace.session_id.as_deref(),
                                workspace.window_id,
                            ))
                        })
                        .context("failed to upsert workspace")?;

                    Self::save_pane(connection, workspace.id, &workspace.center_pane)
                        .context("failed to save center pane")?;

                    Ok(())
                })
            })
            .await
        {
            log::error!("Failed to save workspace: {error}");
        }
    }

    pub async fn recent_workspaces_on_disk(
        &self,
        fs: &dyn Fs,
    ) -> anyhow::Result<Vec<(WorkspaceId, PathBuf, Timestamp)>> {
        let mut existing_workspaces = Vec::new();
        let mut delete_tasks = Vec::new();

        for (workspace_id, location, timestamp) in self.recent_workspaces()? {
            if Self::workspace_path_is_restorable(&location, fs, Some(timestamp)).await {
                existing_workspaces.push((workspace_id, location, timestamp));
            } else {
                delete_tasks.push(self.delete_workspace_by_id(workspace_id));
            }
        }

        let _ = futures::future::join_all(delete_tasks).await;

        Ok(existing_workspaces)
    }

    pub async fn last_workspace(
        &self,
        fs: &dyn Fs,
    ) -> anyhow::Result<Option<(WorkspaceId, PathBuf, Timestamp)>> {
        Ok(self.recent_workspaces_on_disk(fs).await?.into_iter().next())
    }

    query! {
        pub async fn update_activation_order(workspace_id: WorkspaceId) -> anyhow::Result<()> {
            UPDATE workspace
            SET activation_order = (SELECT COALESCE(MAX(activation_order), 0) + 1 FROM workspace)
            WHERE id = ?
        }
    }

    query! {
        pub(crate) async fn set_window_open_status(
            workspace_id: WorkspaceId,
            bounds: SerializedWindowBounds,
            display: Uuid,
        ) -> anyhow::Result<()> {
            UPDATE workspace
            SET window_state = ?2,
                window_x = ?3,
                window_y = ?4,
                window_width = ?5,
                window_height = ?6,
                display = ?7
            WHERE id = ?1
        }
    }

    pub async fn last_session_workspace_locations(
        &self,
        last_session_id: &str,
        last_session_window_stack: Option<Vec<WindowId>>,
        fs: &dyn Fs,
    ) -> anyhow::Result<Vec<SessionWorkspace>> {
        let mut workspaces = Vec::new();

        for (location, window_id) in self.session_workspaces(last_session_id.to_owned())? {
            if Self::workspace_path_is_restorable(&location, fs, None).await {
                workspaces.push(SessionWorkspace {
                    location,
                    window_id: window_id.map(WindowId::from),
                });
            }
        }

        if let Some(stack) = last_session_window_stack {
            workspaces.sort_by_key(|workspace| {
                workspace
                    .window_id
                    .and_then(|window_id| stack.iter().position(|&id| id == window_id))
            });
        }

        Ok(workspaces)
    }

    pub(crate) async fn initialize_schema(&self) -> anyhow::Result<()> {
        self.0
            .write(|connection| {
                connection.with_savepoint("initialize_workspace_schema", || {
                    connection
                        .exec(sql!(
                            CREATE TABLE IF NOT EXISTS workspace(
                                id INTEGER PRIMARY KEY,
                                location BLOB UNIQUE,
                                window_state TEXT,
                                window_x REAL,
                                window_y REAL,
                                window_width REAL,
                                window_height REAL,
                                display BLOB,
                                left_dock_open INTEGER,
                                left_dock_active_panel TEXT,
                                bottom_dock_open INTEGER,
                                bottom_dock_active_panel TEXT,
                                session_id TEXT,
                                window_id INTEGER,
                                activation_order INTEGER NOT NULL DEFAULT 0,
                                timestamp TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                            ) STRICT
                        ))
                        .context("failed to set up workspace table initialization")
                        .and_then(|mut f| f())
                        .context("failed to initialize workspace persistence table")?;

                    connection
                        .exec(sql!(
                            CREATE TABLE IF NOT EXISTS pane(
                                id INTEGER PRIMARY KEY,
                                workspace_id INTEGER NOT NULL UNIQUE,
                                active INTEGER NOT NULL,
                                FOREIGN KEY(workspace_id) REFERENCES workspace(id)
                                ON DELETE CASCADE
                                ON UPDATE CASCADE
                            ) STRICT
                        ))
                        .context("failed to set up pane table initialization")
                        .and_then(|mut f| f())
                        .context("failed to initialize pane table")?;

                    connection
                        .exec(sql!(
                            CREATE TABLE IF NOT EXISTS item(
                                id INTEGER NOT NULL,
                                workspace_id INTEGER NOT NULL,
                                pane_id INTEGER NOT NULL,
                                kind TEXT NOT NULL,
                                position INTEGER NOT NULL,
                                active INTEGER NOT NULL,
                                preview INTEGER NOT NULL,
                                FOREIGN KEY(workspace_id) REFERENCES workspace(id)
                                ON DELETE CASCADE
                                ON UPDATE CASCADE,
                                FOREIGN KEY(pane_id) REFERENCES pane(id)
                                ON DELETE CASCADE,
                                PRIMARY KEY(id, workspace_id)
                            ) STRICT
                        ))
                        .context("failed to set up item table initialization")
                        .and_then(|mut f| f())
                        .context("failed to initialize item table")?;

                    connection
                        .exec(sql!(
                            CREATE INDEX IF NOT EXISTS workspace_activation_order_idx
                            ON workspace(activation_order DESC)
                        ))
                        .context("failed to set up workspace activation order index initialization")
                        .and_then(|mut f| f())
                        .context("failed to initialize workspace activation order index")?;

                    connection
                        .exec(sql!(
                            CREATE INDEX IF NOT EXISTS workspace_timestamp_idx
                            ON workspace(timestamp DESC)
                        ))
                        .context("failed to set up workspace timestamp index initialization")
                        .and_then(|mut f| f())
                        .context("failed to initialize workspace persistence index")?;

                    Ok(())
                })
            })
            .await
    }

    fn recent_workspaces(&self) -> anyhow::Result<Vec<(WorkspaceId, PathBuf, Timestamp)>> {
        Ok(self
            .recent_workspaces_query()?
            .into_iter()
            .map(|(workspace_id, location, timestamp)| {
                (workspace_id, location, parse_timestamp(&timestamp))
            })
            .collect())
    }

    query! {
        fn recent_workspaces_query() -> anyhow::Result<Vec<(WorkspaceId, PathBuf, String)>> {
            SELECT id, location, timestamp
            FROM workspace
            WHERE location IS NOT NULL
            ORDER BY activation_order DESC
        }
    }

    fn session_workspaces(
        &self,
        session_id: String,
    ) -> anyhow::Result<Vec<(PathBuf, Option<u64>)>> {
        Ok(self
            .session_workspaces_query(session_id)?
            .into_iter()
            .map(|(location, window_id)| {
                (
                    location,
                    window_id.and_then(|window_id| u64::try_from(window_id).ok()),
                )
            })
            .collect())
    }

    query! {
        fn session_workspaces_query(
            session_id: String,
        ) -> anyhow::Result<Vec<(PathBuf, Option<i64>)>> {
            SELECT location, window_id
            FROM workspace
            WHERE session_id = ? AND location IS NOT NULL
            ORDER BY activation_order DESC
        }
    }

    query! {
        pub async fn delete_workspace_by_id(workspace_id: WorkspaceId) -> anyhow::Result<()> {
            DELETE FROM workspace WHERE id = ?
        }
    }

    query! {
        pub(crate) async fn clear_recent_workspaces() -> anyhow::Result<()> {
            DELETE FROM workspace
        }
    }

    async fn workspace_path_is_restorable(
        path: &Path,
        fs: &dyn Fs,
        timestamp: Option<Timestamp>,
    ) -> bool {
        match fs.metadata(path).await.ok().flatten() {
            None => timestamp.is_some_and(|timestamp| {
                Timestamp::now().duration_since(timestamp) < SignedDuration::from_hours(24 * 7)
            }),
            Some(metadata) => metadata.is_dir,
        }
    }

    pub(crate) fn workspace_for_path<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Option<SerializedWorkspace> {
        self.read(|connection| {
            connection
                .select_row_bound::<&Path, (
                    WorkspaceId,
                    PathBuf,
                    Option<SerializedWindowBounds>,
                    Option<Uuid>,
                    DockStructure,
                    Option<String>,
                    Option<u64>,
                )>(sql!(
                    SELECT
                        id,
                        location,
                        window_state,
                        window_x,
                        window_y,
                        window_width,
                        window_height,
                        display,
                        left_dock_open,
                        left_dock_active_panel,
                        bottom_dock_open,
                        bottom_dock_active_panel,
                        session_id,
                        window_id
                    FROM workspace
                    WHERE location = ? AND location IS NOT NULL
                    LIMIT 1
                ))
                .context("failed to prepare workspace by path query")
                .and_then(|mut f| f(path.as_ref()))
                .context("failed to query workspace by path")
                .map(|workspace| {
                    workspace.and_then(
                        |(
                            workspace_id,
                            location,
                            window_bounds,
                            display,
                            docks,
                            session_id,
                            window_id,
                        )| {
                            Some(SerializedWorkspace {
                                id: workspace_id,
                                location,
                                center_pane: Self::get_center_pane(connection, workspace_id)
                                    .context("failed to get center pane")
                                    .log_err()?,
                                docks,
                                window_bounds,
                                display,
                                session_id,
                                window_id,
                            })
                        },
                    )
                })
        })
        .context("No workspace found for path")
        .log_err()
        .flatten()
    }

    fn get_center_pane(
        connection: &Connection,
        workspace_id: WorkspaceId,
    ) -> anyhow::Result<SerializedPane> {
        let pane = connection
            .select_row_bound::<WorkspaceId, (PaneId, bool)>(sql!(
                SELECT id, active
                FROM pane
                WHERE workspace_id = ?
            ))
            .context("failed to prepare center pane query")
            .and_then(|mut f| f(workspace_id))
            .context("failed to query center pane")?;

        if let Some((pane_id, active)) = pane {
            let items = Self::get_items(connection, pane_id)?;
            if items.is_empty() {
                Ok(SerializedPane::new(Vec::new(), true))
            } else {
                Ok(SerializedPane::new(items, active))
            }
        } else {
            Ok(SerializedPane::new(Vec::new(), true))
        }
    }

    fn get_items(connection: &Connection, pane_id: PaneId) -> anyhow::Result<Vec<SerializedItem>> {
        connection
            .select_bound::<PaneId, SerializedItem>(sql!(
                SELECT kind, id, active, preview
                FROM item
                WHERE pane_id = ?
                ORDER BY position
            ))
            .context("failed to prepare items query")
            .and_then(|mut f| f(pane_id))
            .context("failed to query items")
    }

    fn save_pane(
        connection: &Connection,
        workspace_id: WorkspaceId,
        pane: &SerializedPane,
    ) -> anyhow::Result<PaneId> {
        let pane_id = connection
            .select_row_bound::<(WorkspaceId, bool), PaneId>(sql!(
                INSERT INTO pane(workspace_id, active)
                VALUES (?, ?)
                RETURNING id
            ))
            .context("failed to prepare pane insertion")
            .and_then(|mut f| f((workspace_id, pane.active)))
            .context("failed to insert pane")?
            .context("failed to retrieve id from inserted pane")?;

        Self::save_items(connection, workspace_id, pane_id, &pane.children)
            .context("failed to save pane items")?;

        Ok(pane_id)
    }

    fn save_items(
        connection: &Connection,
        workspace_id: WorkspaceId,
        pane_id: PaneId,
        items: &[SerializedItem],
    ) -> anyhow::Result<()> {
        let mut insert = connection
            .exec_bound(sql!(
                INSERT INTO item(workspace_id, pane_id, position, kind, id, active, preview)
                VALUES (?, ?, ?, ?, ?, ?, ?)
            ))
            .context("failed to prepare item insertion")?;

        for (position, item) in items.iter().enumerate() {
            insert((workspace_id, pane_id, position, item)).context("failed to insert item")?;
        }

        Ok(())
    }

    #[cfg(test)]
    query! {
        pub(crate) fn recent_workspace_count() -> anyhow::Result<usize> {
            SELECT COUNT(*) FROM workspace
        }
    }
}

db::static_connection!(WorkspaceDb, []);

pub fn delete_unloaded_items(
    alive_items: Vec<ItemId>,
    workspace_id: WorkspaceId,
    table: &'static str,
    db: &ThreadSafeConnection,
    cx: &mut App,
) -> Task<anyhow::Result<()>> {
    let db = db.clone();
    cx.spawn(async move |_| {
        db.write(move |connection| {
            let placeholders = alive_items
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(", ");
            let query = format!(
                "DELETE FROM {table} WHERE workspace_id = ? AND id NOT IN ({placeholders})"
            );
            let mut statement = Statement::prepare(connection, query)
                .context("failed to prepare unloaded item deletion")?;
            let mut next_index = statement.bind(&workspace_id, 1)?;
            for item_id in alive_items {
                next_index = statement.bind(&item_id, next_index)?;
            }
            statement.exec().context("failed to delete unloaded items")
        })
        .await
    })
}

fn parse_timestamp(text: &str) -> Timestamp {
    DateTime::strptime("%Y-%m-%d %H:%M:%S", text)
        .and_then(|datetime| datetime.to_zoned(TimeZone::UTC))
        .map_or_else(|_| Timestamp::now(), |datetime| datetime.timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    use gpui::{TestAppContext, WindowId};
    use indoc::indoc;
    use serde_json::json;

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    use fs::TempFs;
    use util_macros::path;
    use worktree::WorktreeModelHandle;

    use crate::{OpenMode, Root, SharedState, Workspace, tests::init_test};

    #[gpui::test]
    async fn test_save_workspace_deduplicates_paths(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        temp_fs.insert_tree("project", json!(null));

        let workspace_db = WorkspaceDb::test_open("test_save_workspace_deduplicates_paths").await;
        workspace_db.clear_recent_workspaces().await.unwrap();

        let project_path = temp_fs.path().join("project");
        workspace_db
            .save_workspace(SerializedWorkspace {
                id: WorkspaceId::from(1),
                location: project_path.clone(),
                center_pane: SerializedPane::default(),
                docks: DockStructure::default(),
                window_bounds: None,
                display: None,
                session_id: Some("session-uuid".to_string()),
                window_id: Some(10),
            })
            .await;
        workspace_db
            .save_workspace(SerializedWorkspace {
                id: WorkspaceId::from(1),
                location: project_path.clone(),
                center_pane: SerializedPane::default(),
                docks: DockStructure::default(),
                window_bounds: None,
                display: None,
                session_id: Some("session-uuid".to_string()),
                window_id: Some(10),
            })
            .await;

        let recent_workspaces = workspace_db
            .recent_workspaces_on_disk(temp_fs.as_ref())
            .await
            .unwrap();

        let Some((workspace_id, location, _timestamp)) = recent_workspaces.first() else {
            panic!("expected a recent workspace");
        };
        assert_eq!(*workspace_id, WorkspaceId::from(1));
        assert_eq!(location, &project_path);
        assert_eq!(workspace_db.recent_workspace_count().unwrap(), 1);
    }

    #[gpui::test]
    async fn test_clear_recent_workspaces(_cx: &mut TestAppContext) {
        let workspace_db = WorkspaceDb::test_open("test_clear_recent_workspaces").await;
        workspace_db.clear_recent_workspaces().await.unwrap();

        workspace_db
            .save_workspace(SerializedWorkspace {
                id: WorkspaceId::from(1),
                location: PathBuf::from("first"),
                center_pane: SerializedPane::default(),
                docks: DockStructure::default(),
                window_bounds: None,
                display: None,
                session_id: Some("session-uuid".to_string()),
                window_id: Some(10),
            })
            .await;
        workspace_db
            .save_workspace(SerializedWorkspace {
                id: WorkspaceId::from(2),
                location: PathBuf::from("second"),
                center_pane: SerializedPane::default(),
                docks: DockStructure::default(),
                window_bounds: None,
                display: None,
                session_id: Some("session-uuid".to_string()),
                window_id: Some(11),
            })
            .await;

        assert_eq!(workspace_db.recent_workspaces().unwrap().len(), 2);

        workspace_db.clear_recent_workspaces().await.unwrap();

        assert!(workspace_db.recent_workspaces().unwrap().is_empty());
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[gpui::test]
    async fn test_save_workspace_preserves_non_utf8_paths(_cx: &mut TestAppContext) {
        let workspace_db =
            WorkspaceDb::test_open("test_save_workspace_preserves_non_utf8_paths").await;
        let path = PathBuf::from(OsString::from_vec(vec![0x2f, 0x74, 0x6d, 0x70, 0x2f, 0x80]));

        workspace_db
            .save_workspace(SerializedWorkspace {
                id: WorkspaceId::from(1),
                location: path.clone(),
                center_pane: SerializedPane::default(),
                docks: DockStructure::default(),
                window_bounds: None,
                display: None,
                session_id: Some("session-uuid".to_string()),
                window_id: Some(10),
            })
            .await;

        let rows = workspace_db.recent_workspaces().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, path);
    }

    #[gpui::test]
    async fn test_create_workspace_serialization(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let shared_state = cx.update(|cx| SharedState::test_new(temp_fs.clone(), None, cx));
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

        let workspace_db = cx.update(|cx| WorkspaceDb::global(cx));
        let workspace_id = workspace_db.next_id().await.unwrap();
        let (root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(Workspace::create(workspace_id, shared_state, window, cx))
        });
        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());

        let project_path = temp_fs.path().join(path!("project"));
        let open_workspace = workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_workspace_for_path(project_path.clone(), OpenMode::Activate, window, cx)
        });
        let workspace = open_workspace.await.unwrap();
        let workspace_id = workspace
            .read_with(cx, |workspace, _| workspace.database_id())
            .unwrap();

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

        let serialized = workspace_db
            .workspace_for_path(&project_path)
            .expect("workspace should be fully serialized in the DB after database_id assignment");
        assert_eq!(serialized.id, workspace_id);
    }

    #[gpui::test]
    async fn test_last_session_workspace_locations(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        temp_fs.insert_tree(path!("first"), json!(null));
        temp_fs.insert_tree(path!("second"), json!(null));
        temp_fs.insert_tree(path!("third"), json!(null));
        temp_fs.insert_tree(path!("fourth"), json!(null));

        let workspace_db = WorkspaceDb::test_open("test_last_session_workspace_locations").await;

        let first_path = temp_fs.path().join(path!("first"));
        let second_path = temp_fs.path().join(path!("second"));
        let third_path = temp_fs.path().join(path!("third"));
        let fourth_path = temp_fs.path().join(path!("fourth"));

        for (workspace_id, location, window_id) in [
            (1, first_path.clone(), 9_u64),
            (2, second_path.clone(), 5_u64),
            (3, third_path.clone(), 8_u64),
            (4, fourth_path.clone(), 2_u64),
        ] {
            workspace_db
                .save_workspace(SerializedWorkspace {
                    id: WorkspaceId::from(workspace_id),
                    location,
                    center_pane: SerializedPane::default(),
                    docks: DockStructure::default(),
                    window_bounds: None,
                    display: None,
                    session_id: Some("session-uuid".to_string()),
                    window_id: Some(window_id),
                })
                .await;
        }

        let locations = workspace_db
            .last_session_workspace_locations(
                "session-uuid",
                Some(Vec::from([
                    WindowId::from(2_u64),
                    WindowId::from(8_u64),
                    WindowId::from(5_u64),
                    WindowId::from(9_u64),
                ])),
                temp_fs.as_ref(),
            )
            .await
            .unwrap();

        assert_eq!(
            locations,
            vec![
                SessionWorkspace {
                    location: fourth_path,
                    window_id: Some(WindowId::from(2_u64)),
                },
                SessionWorkspace {
                    location: third_path,
                    window_id: Some(WindowId::from(8_u64)),
                },
                SessionWorkspace {
                    location: second_path,
                    window_id: Some(WindowId::from(5_u64)),
                },
                SessionWorkspace {
                    location: first_path,
                    window_id: Some(WindowId::from(9_u64)),
                },
            ]
        );
    }

    #[gpui::test]
    async fn test_last_session_workspace_locations_skips_missing_paths(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        temp_fs.insert_tree(path!("project"), json!(null));

        let workspace_db =
            WorkspaceDb::test_open("test_last_session_workspace_locations_skips_missing_paths")
                .await;

        workspace_db
            .save_workspace(SerializedWorkspace {
                id: WorkspaceId::from(1),
                location: temp_fs.path().join(path!("project")),
                center_pane: SerializedPane::default(),
                docks: DockStructure::default(),
                window_bounds: None,
                display: None,
                session_id: Some("session-uuid".to_string()),
                window_id: Some(1),
            })
            .await;
        workspace_db
            .save_workspace(SerializedWorkspace {
                id: WorkspaceId::from(2),
                location: temp_fs.path().join(path!("missing_project")),
                center_pane: SerializedPane::default(),
                docks: DockStructure::default(),
                window_bounds: None,
                display: None,
                session_id: Some("session-uuid".to_string()),
                window_id: Some(2),
            })
            .await;

        let locations = workspace_db
            .last_session_workspace_locations("session-uuid", None, temp_fs.as_ref())
            .await
            .unwrap();

        assert_eq!(
            locations,
            vec![SessionWorkspace {
                location: temp_fs.path().join(path!("project")),
                window_id: Some(WindowId::from(1_u64)),
            }]
        );
    }

    #[gpui::test]
    async fn test_replace_workspace_removes_workspace_from_current_session(
        cx: &mut TestAppContext,
    ) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let shared_state = cx.update(|cx| SharedState::test_new(temp_fs.clone(), None, cx));
        init_test(shared_state.clone(), cx);

        temp_fs.insert_tree(path!("project"), json!(null));

        let workspace_db = cx.update(|cx| WorkspaceDb::global(cx));
        let workspace_id = workspace_db.next_id().await.unwrap();
        let (root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(Workspace::create(workspace_id, shared_state, window, cx))
        });
        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());
        let project_path = temp_fs.path().join(path!("project"));

        let workspace = workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.open_workspace_for_path(
                    project_path.clone(),
                    OpenMode::Activate,
                    window,
                    cx,
                )
            })
            .await
            .unwrap();
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

        let serialized_workspace = workspace_db
            .workspace_for_path(&project_path)
            .expect("workspace row should exist after serialization");

        assert!(serialized_workspace.session_id.is_some());
        assert!(serialized_workspace.window_id.is_some());

        root.update_in(cx, |root, window, cx| root.replace_workspace(window, cx));
        cx.run_until_parked();

        let serialized_workspace = workspace_db
            .workspace_for_path(&project_path)
            .expect("workspace row should remain after replacement");

        assert_eq!(serialized_workspace.session_id, None);
        assert_eq!(serialized_workspace.window_id, None);
    }

    #[gpui::test]
    async fn test_close_window_removes_workspace_from_current_session(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let shared_state = cx.update(|cx| SharedState::test_new(temp_fs.clone(), None, cx));
        init_test(shared_state.clone(), cx);

        temp_fs.insert_tree(path!("project"), json!(null));

        let workspace_db = cx.update(|cx| WorkspaceDb::global(cx));
        let workspace_id = workspace_db.next_id().await.unwrap();
        let (root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(Workspace::create(workspace_id, shared_state, window, cx))
        });
        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());
        let project_path = temp_fs.path().join(path!("project"));

        let workspace = workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.open_workspace_for_path(
                    project_path.clone(),
                    OpenMode::Activate,
                    window,
                    cx,
                )
            })
            .await
            .unwrap();
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

        let serialized_workspace = workspace_db
            .workspace_for_path(&project_path)
            .expect("workspace row should exist after serialization");

        assert!(serialized_workspace.session_id.is_some());
        assert!(serialized_workspace.window_id.is_some());

        root.update_in(cx, |root, window, cx| {
            root.close_window(&actions::workspace::CloseWindow, window, cx);
        });
        cx.run_until_parked();

        let serialized_workspace = workspace_db
            .workspace_for_path(&project_path)
            .expect("workspace row should remain after close");

        assert_eq!(serialized_workspace.session_id, None);
        assert_eq!(serialized_workspace.window_id, None);
    }

    #[gpui::test]
    async fn test_center_pane_serialization(_cx: &mut TestAppContext) {
        let workspace_db = WorkspaceDb::test_open("test_center_pane_serialization").await;
        let workspace_id = workspace_db.next_id().await.unwrap();
        let location = PathBuf::from("project");
        let center_pane = SerializedPane::new(
            vec![
                SerializedItem::new("Editor", 1, false, false),
                SerializedItem::new("RequestEditor", 2, true, false),
                SerializedItem::new("RequestEditor", 3, false, true),
            ],
            true,
        );

        workspace_db
            .save_workspace(SerializedWorkspace {
                id: workspace_id,
                location: location.clone(),
                center_pane: center_pane.clone(),
                docks: DockStructure::default(),
                window_bounds: None,
                display: None,
                session_id: Some("session-uuid".to_string()),
                window_id: Some(10),
            })
            .await;

        let serialized_workspace = workspace_db.workspace_for_path(&location).unwrap();
        assert_eq!(serialized_workspace.center_pane, center_pane);
    }

    #[gpui::test]
    async fn test_empty_center_pane_serialization(_cx: &mut TestAppContext) {
        let workspace_db = WorkspaceDb::test_open("test_empty_center_pane_serialization").await;
        let workspace_id = workspace_db.next_id().await.unwrap();
        let location = PathBuf::from("project");

        workspace_db
            .save_workspace(SerializedWorkspace {
                id: workspace_id,
                location: location.clone(),
                center_pane: SerializedPane::default(),
                docks: DockStructure::default(),
                window_bounds: None,
                display: None,
                session_id: Some("session-uuid".to_string()),
                window_id: Some(10),
            })
            .await;

        let serialized_workspace = workspace_db.workspace_for_path(&location).unwrap();
        assert_eq!(
            serialized_workspace.center_pane,
            SerializedPane::new(Vec::new(), true)
        );
    }

    #[gpui::test]
    async fn test_cleanup_pane_items(_cx: &mut TestAppContext) {
        let workspace_db = WorkspaceDb::test_open("test_cleanup_pane_items").await;
        let workspace_id = workspace_db.next_id().await.unwrap();
        let location = PathBuf::from("project");
        let old_center_pane = SerializedPane::new(
            vec![
                SerializedItem::new("Editor", 1, true, false),
                SerializedItem::new("RequestEditor", 2, false, true),
            ],
            true,
        );
        let new_center_pane = SerializedPane::new(
            vec![SerializedItem::new("RequestEditor", 3, true, true)],
            true,
        );

        workspace_db
            .save_workspace(SerializedWorkspace {
                id: workspace_id,
                location: location.clone(),
                center_pane: old_center_pane,
                docks: DockStructure::default(),
                window_bounds: None,
                display: None,
                session_id: Some("session-uuid".to_string()),
                window_id: Some(10),
            })
            .await;
        workspace_db
            .save_workspace(SerializedWorkspace {
                id: workspace_id,
                location: location.clone(),
                center_pane: new_center_pane.clone(),
                docks: DockStructure::default(),
                window_bounds: None,
                display: None,
                session_id: Some("session-uuid".to_string()),
                window_id: Some(10),
            })
            .await;

        let serialized_workspace = workspace_db.workspace_for_path(&location).unwrap();
        assert_eq!(serialized_workspace.center_pane, new_center_pane);
    }

    #[gpui::test]
    async fn test_dock_serialization(_cx: &mut TestAppContext) {
        let workspace_db = WorkspaceDb::test_open("test_dock_serialization").await;
        let workspace_id = workspace_db.next_id().await.unwrap();
        let location = PathBuf::from("project");
        let docks = DockStructure {
            left: model::DockData {
                visible: true,
                active_panel: Some("ProjectPanel".to_string()),
            },
            bottom: model::DockData {
                visible: false,
                active_panel: Some("ResponsePanel".to_string()),
            },
        };

        workspace_db
            .save_workspace(SerializedWorkspace {
                id: workspace_id,
                location: location.clone(),
                center_pane: SerializedPane::default(),
                docks: docks.clone(),
                window_bounds: None,
                display: None,
                session_id: Some("session-uuid".to_string()),
                window_id: Some(10),
            })
            .await;

        let serialized_workspace = workspace_db.workspace_for_path(&location).unwrap();
        assert_eq!(serialized_workspace.docks, docks);
    }
}
