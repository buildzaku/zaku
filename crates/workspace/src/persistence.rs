pub mod model;

use anyhow::Context;
use chrono::{DateTime, NaiveDateTime, Utc};
use gpui::{Bounds, WindowBounds, WindowId};
use std::path::{Path, PathBuf};

use db::{
    Bind, Column, Row, Statement, StaticColumnCount, ThreadSafeConnection, kv::KeyValueStore,
    query, sql_macros::sql,
};
use fs::Fs;
use serde::{Deserialize, Serialize};
use util::ResultExt;
use uuid::Uuid;

use self::model::{SerializedWorkspace, SerializedWorkspaceLocation, SessionWorkspace};
use crate::WorkspaceId;

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
        let ((x, y, width, height), _): ((f32, f32, f32, f32), _) =
            Column::column(row, next_index)?;
        let bounds = Bounds {
            origin: gpui::point(gpui::px(x), gpui::px(y)),
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

pub fn read_default_window_bounds(kv_store: &KeyValueStore) -> Option<(Uuid, WindowBounds)> {
    let json_str = kv_store
        .read_kv(DEFAULT_WINDOW_BOUNDS_KEY)
        .log_err()
        .flatten()?;

    let (display_uuid, persisted) =
        serde_json::from_str::<(Uuid, WindowBoundsJson)>(&json_str).ok()?;
    Some((display_uuid, persisted.into()))
}

pub async fn write_default_window_bounds(
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

db::static_connection!(WorkspaceDb, []);

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
                let workspace_location = workspace.location.path();
                connection.with_savepoint("save_workspace", || {
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
                                (SELECT COALESCE(MAX(activation_order), 0) + 1 FROM workspace),
                                CURRENT_TIMESTAMP
                            )
                            ON CONFLICT(id)
                            DO UPDATE SET
                                location = excluded.location,
                                session_id = excluded.session_id,
                                window_id = excluded.window_id,
                                timestamp = CURRENT_TIMESTAMP
                        ))
                        .context("failed to prepare workspace upsert query")
                        .and_then(|mut f| {
                            f((
                                workspace.id,
                                workspace_location,
                                workspace.session_id.as_deref(),
                                workspace.window_id,
                            ))
                        })
                        .context("failed to upsert workspace")?;

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
    ) -> anyhow::Result<Vec<(WorkspaceId, SerializedWorkspaceLocation, DateTime<Utc>)>> {
        let mut existing_workspaces = Vec::new();
        let mut delete_tasks = Vec::new();

        for (workspace_id, location, timestamp) in self.recent_workspaces()? {
            let workspace_path = location.path().to_path_buf();
            if Self::workspace_path_is_restorable(&workspace_path, fs, Some(timestamp)).await {
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
    ) -> anyhow::Result<Option<(WorkspaceId, SerializedWorkspaceLocation, DateTime<Utc>)>> {
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

        for (workspace_id, location, window_id) in
            self.session_workspaces(last_session_id.to_owned())?
        {
            let workspace_path = location.path().to_path_buf();
            if Self::workspace_path_is_restorable(&workspace_path, fs, None).await {
                workspaces.push(SessionWorkspace {
                    workspace_id,
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

    fn recent_workspaces(
        &self,
    ) -> anyhow::Result<Vec<(WorkspaceId, SerializedWorkspaceLocation, DateTime<Utc>)>> {
        Ok(self
            .recent_workspaces_query()?
            .into_iter()
            .map(|(workspace_id, location, timestamp)| {
                (
                    workspace_id,
                    SerializedWorkspaceLocation::Local(location),
                    parse_timestamp(&timestamp),
                )
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
    ) -> anyhow::Result<Vec<(WorkspaceId, SerializedWorkspaceLocation, Option<u64>)>> {
        Ok(self
            .session_workspaces_query(session_id)?
            .into_iter()
            .map(|(workspace_id, location, window_id)| {
                (
                    workspace_id,
                    SerializedWorkspaceLocation::Local(location),
                    window_id.and_then(|window_id| u64::try_from(window_id).ok()),
                )
            })
            .collect())
    }

    query! {
        fn session_workspaces_query(
            session_id: String,
        ) -> anyhow::Result<Vec<(WorkspaceId, PathBuf, Option<i64>)>> {
            SELECT id, location, window_id
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

    async fn workspace_path_is_restorable(
        path: &Path,
        fs: &dyn Fs,
        timestamp: Option<DateTime<Utc>>,
    ) -> bool {
        match fs.metadata(path).await.ok().flatten() {
            None => timestamp
                .is_some_and(|timestamp| Utc::now() - timestamp < chrono::Duration::days(7)),
            Some(metadata) => metadata.is_dir,
        }
    }

    pub(crate) fn workspace_for_root<P: AsRef<Path>>(
        &self,
        worktree_root: P,
    ) -> Option<SerializedWorkspace> {
        self.read(|connection| {
            connection
                .select_row_bound::<&Path, (
                    WorkspaceId,
                    PathBuf,
                    Option<SerializedWindowBounds>,
                    Option<Uuid>,
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
                        session_id,
                        window_id
                    FROM workspace
                    WHERE location = ? AND location IS NOT NULL
                    LIMIT 1
                ))
                .context("failed to prepare workspace by root query")
                .and_then(|mut f| f(worktree_root.as_ref()))
                .context("failed to query workspace by root")
                .map(|workspace| {
                    workspace.map(
                        |(
                            workspace_id,
                            location,
                            window_bounds,
                            display,
                            session_id,
                            window_id,
                        )| {
                            SerializedWorkspace {
                                id: workspace_id,
                                location: SerializedWorkspaceLocation::Local(location),
                                window_bounds,
                                display,
                                session_id,
                                window_id,
                            }
                        },
                    )
                })
        })
        .context("No workspace found for root")
        .log_err()
        .flatten()
    }

    pub(crate) fn workspace_for_id(
        &self,
        workspace_id: WorkspaceId,
    ) -> Option<SerializedWorkspace> {
        self.read(|connection| {
            connection
                .select_row_bound::<WorkspaceId, (
                    PathBuf,
                    Option<SerializedWindowBounds>,
                    Option<Uuid>,
                    Option<String>,
                    Option<u64>,
                )>(sql!(
                    SELECT
                        location,
                        window_state,
                        window_x,
                        window_y,
                        window_width,
                        window_height,
                        display,
                        session_id,
                        window_id
                    FROM workspace
                    WHERE id = ? AND location IS NOT NULL
                ))
                .context("failed to prepare workspace by id query")
                .and_then(|mut f| f(workspace_id))
                .context("failed to query workspace by id")
                .map(|workspace| {
                    workspace.map(
                        |(location, window_bounds, display, session_id, window_id)| {
                            SerializedWorkspace {
                                id: workspace_id,
                                location: SerializedWorkspaceLocation::Local(location),
                                window_bounds,
                                display,
                                session_id,
                                window_id,
                            }
                        },
                    )
                })
        })
        .context("No workspace found for id")
        .log_err()
        .flatten()
    }

    #[cfg(test)]
    query! {
        pub(crate) async fn clear_recent_workspaces() -> anyhow::Result<()> {
            DELETE FROM workspace
        }
    }

    #[cfg(test)]
    query! {
        pub(crate) fn recent_workspace_count() -> anyhow::Result<usize> {
            SELECT COUNT(*) FROM workspace
        }
    }
}

fn parse_timestamp(text: &str) -> DateTime<Utc> {
    NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S")
        .map_or_else(|_| Utc::now(), |naive| naive.and_utc())
}

#[cfg(test)]
mod tests {
    use super::*;

    use gpui::{TestAppContext, WindowId};
    use indoc::indoc;
    use serde_json::json;

    #[cfg(any(target_os = "macos", target_os = "linux"))]
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
                location: SerializedWorkspaceLocation::Local(project_path.clone()),
                window_bounds: None,
                display: None,
                session_id: Some("session-a".to_string()),
                window_id: Some(10),
            })
            .await;
        workspace_db
            .save_workspace(SerializedWorkspace {
                id: WorkspaceId::from(1),
                location: SerializedWorkspaceLocation::Local(project_path.clone()),
                window_bounds: None,
                display: None,
                session_id: Some("session-a".to_string()),
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
        assert_eq!(location.path(), project_path);
        assert_eq!(workspace_db.recent_workspace_count().unwrap(), 1);
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[gpui::test]
    async fn test_save_workspace_preserves_non_utf8_paths(_cx: &mut TestAppContext) {
        let workspace_db =
            WorkspaceDb::test_open("test_save_workspace_preserves_non_utf8_paths").await;
        let path = PathBuf::from(OsString::from_vec(vec![0x2f, 0x74, 0x6d, 0x70, 0x2f, 0x80]));

        workspace_db
            .save_workspace(SerializedWorkspace {
                id: WorkspaceId::from(1),
                location: SerializedWorkspaceLocation::Local(path.clone()),
                window_bounds: None,
                display: None,
                session_id: Some("session-a".to_string()),
                window_id: Some(10),
            })
            .await;

        let rows = workspace_db.recent_workspaces().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, SerializedWorkspaceLocation::Local(path));
    }

    #[gpui::test]
    async fn test_create_workspace_serialization(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
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

        let serialized = workspace_db.workspace_for_id(workspace_id);
        assert!(
            serialized.is_some(),
            "workspace should be fully serialized in the DB after database_id assignment"
        );
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
                    location: SerializedWorkspaceLocation::Local(location),
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
                    workspace_id: WorkspaceId::from(4),
                    location: SerializedWorkspaceLocation::Local(fourth_path),
                    window_id: Some(WindowId::from(2_u64)),
                },
                SessionWorkspace {
                    workspace_id: WorkspaceId::from(3),
                    location: SerializedWorkspaceLocation::Local(third_path),
                    window_id: Some(WindowId::from(8_u64)),
                },
                SessionWorkspace {
                    workspace_id: WorkspaceId::from(2),
                    location: SerializedWorkspaceLocation::Local(second_path),
                    window_id: Some(WindowId::from(5_u64)),
                },
                SessionWorkspace {
                    workspace_id: WorkspaceId::from(1),
                    location: SerializedWorkspaceLocation::Local(first_path),
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
                location: SerializedWorkspaceLocation::Local(temp_fs.path().join(path!("project"))),
                window_bounds: None,
                display: None,
                session_id: Some("session-uuid".to_string()),
                window_id: Some(1),
            })
            .await;
        workspace_db
            .save_workspace(SerializedWorkspace {
                id: WorkspaceId::from(2),
                location: SerializedWorkspaceLocation::Local(
                    temp_fs.path().join(path!("missing_project")),
                ),
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
                workspace_id: WorkspaceId::from(1),
                location: SerializedWorkspaceLocation::Local(temp_fs.path().join(path!("project"))),
                window_id: Some(WindowId::from(1_u64)),
            }]
        );
    }

    #[gpui::test]
    async fn test_replace_workspace_removes_workspace_from_current_session(
        cx: &mut TestAppContext,
    ) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
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

        let workspace_id = workspace
            .read_with(cx, |workspace, _| workspace.database_id())
            .unwrap();
        let serialized_workspace = workspace_db
            .workspace_for_id(workspace_id)
            .expect("workspace row should exist after serialization");

        assert!(serialized_workspace.session_id.is_some());
        assert!(serialized_workspace.window_id.is_some());

        root.update_in(cx, |root, window, cx| root.replace_workspace(window, cx));
        cx.run_until_parked();

        let serialized_workspace = workspace_db
            .workspace_for_id(workspace_id)
            .expect("workspace row should remain after replacement");

        assert_eq!(serialized_workspace.session_id, None);
        assert_eq!(serialized_workspace.window_id, None);
    }

    #[gpui::test]
    async fn test_close_window_removes_workspace_from_current_session(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
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

        let workspace_id = workspace
            .read_with(cx, |workspace, _| workspace.database_id())
            .unwrap();
        let serialized_workspace = workspace_db
            .workspace_for_id(workspace_id)
            .expect("workspace row should exist after serialization");

        assert!(serialized_workspace.session_id.is_some());
        assert!(serialized_workspace.window_id.is_some());

        root.update_in(cx, |root, window, cx| {
            root.close_window(&actions::workspace::CloseWindow, window, cx);
        });
        cx.run_until_parked();

        let serialized_workspace = workspace_db
            .workspace_for_id(workspace_id)
            .expect("workspace row should remain after close");

        assert_eq!(serialized_workspace.session_id, None);
        assert_eq!(serialized_workspace.window_id, None);
    }
}
