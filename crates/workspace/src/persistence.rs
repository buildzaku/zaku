pub mod model;

use anyhow::Context;
use chrono::{DateTime, NaiveDateTime, Utc};
use gpui::{App, WindowId};
use indoc::indoc;
use std::path::PathBuf;

use db::{AppDatabase, ThreadSafeConnection};
use fs::Fs;

use self::model::{SerializedWorkspace, SerializedWorkspaceLocation, SessionWorkspace};
use crate::WorkspaceId;

#[derive(Clone)]
pub struct WorkspaceDb(ThreadSafeConnection);

impl WorkspaceDb {
    pub fn from_app_db(db: &AppDatabase) -> Self {
        Self(db.0.clone())
    }

    pub fn global(cx: &App) -> Self {
        Self(AppDatabase::global(cx).clone())
    }

    #[cfg(any(test, feature = "test-support"))]
    pub async fn open_test_db(name: &'static str) -> Self {
        let workspace_db = Self(db::open_test_db(name).await);
        workspace_db
            .initialize_schema()
            .await
            .expect("workspace persistence schema should initialize");
        workspace_db
    }

    pub async fn next_id(&self) -> anyhow::Result<WorkspaceId> {
        self.0
            .write(|connection| {
                let next_id = connection
                    .select_row::<i64>("INSERT INTO workspace DEFAULT VALUES RETURNING id")
                    .context("failed to prepare next workspace id query")
                    .and_then(|mut f| f().context("failed to allocate next workspace id"))?
                    .context("next workspace id query returned no row")?;

                Ok(WorkspaceId::from(next_id))
            })
            .await
    }

    pub async fn save_workspace(&self, workspace: SerializedWorkspace) {
        if let Err(error) = self
            .0
            .write(move |connection| {
                let workspace_location = workspace.location.path();
                let window_id = serialize_window_id(workspace.window_id)?;
                connection.with_savepoint("save_workspace", || {
                    connection
                        .exec_bound(indoc! {"
                            DELETE FROM workspace
                            WHERE id != ?1 AND location = ?2
                        "})
                        .context("failed to prepare old workspace location cleanup query")
                        .and_then(|mut f| f((i64::from(workspace.id), workspace_location)))
                        .context("failed to clear old workspace locations")?;

                    connection
                        .exec_bound(indoc! {"
                            INSERT INTO workspace(id, location, session_id, window_id, activation_order, timestamp)
                            VALUES (?1, ?2, ?3, ?4, (SELECT COALESCE(MAX(activation_order), 0) + 1 FROM workspace), CURRENT_TIMESTAMP)
                            ON CONFLICT(id)
                            DO UPDATE SET
                                location = excluded.location,
                                session_id = excluded.session_id,
                                window_id = excluded.window_id,
                                timestamp = CURRENT_TIMESTAMP
                        "})
                        .context("failed to prepare workspace upsert query")
                        .and_then(|mut f| {
                            f((
                                i64::from(workspace.id),
                                workspace_location,
                                workspace.session_id.as_deref(),
                                window_id,
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
            if Self::all_paths_exist_with_a_directory(
                std::slice::from_ref(&workspace_path),
                fs,
                Some(timestamp),
            )
            .await
            {
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

    pub async fn update_activation_order(&self, workspace_id: WorkspaceId) -> anyhow::Result<()> {
        self.0
            .write(move |connection| {
                connection
                    .exec_bound(indoc! {"
                        UPDATE workspace
                        SET activation_order = (SELECT COALESCE(MAX(activation_order), 0) + 1 FROM workspace)
                        WHERE id = ?1
                    "})
                    .context("failed to prepare workspace activation order update query")
                    .and_then(|mut f| f([i64::from(workspace_id)]))
                    .context("failed to update workspace activation order")?;

                Ok(())
            })
            .await
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
            if Self::all_paths_exist_with_a_directory(
                std::slice::from_ref(&workspace_path),
                fs,
                None,
            )
            .await
            {
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

    #[cfg(test)]
    pub(crate) async fn clear_recent_workspaces(&self) -> anyhow::Result<()> {
        self.0
            .write(|connection| {
                connection
                    .exec("DELETE FROM workspace")
                    .context("failed to set up recent workspace clear query")
                    .and_then(|mut f| f())
                    .context("failed to clear recent workspaces")?;
                Ok(())
            })
            .await
    }

    #[cfg(test)]
    pub(crate) fn recent_workspace_count(&self) -> anyhow::Result<usize> {
        self.0.read(|connection| {
            let count = connection
                .select_row::<i64>("SELECT COUNT(*) FROM workspace")
                .context("failed to prepare recent workspace count query")
                .and_then(|mut f| f().context("failed to count recent workspaces"))?
                .context("recent workspace count query returned no row")?;

            Ok(count as usize)
        })
    }

    #[cfg(test)]
    fn workspace_for_id(&self, workspace_id: WorkspaceId) -> Option<SerializedWorkspace> {
        self.0
            .read(|connection| {
                connection
                    .select_row_bound::<[i64; 1], (PathBuf, Option<String>, Option<i64>)>(indoc! {"
                        SELECT location, session_id, window_id
                        FROM workspace
                        WHERE id = ?1 AND location IS NOT NULL
                    "})
                    .context("failed to prepare workspace by id query")
                    .and_then(|mut f| f([i64::from(workspace_id)]))
                    .context("failed to query workspace by id")
                    .map(|workspace| {
                        workspace.map(|(location, session_id, window_id)| SerializedWorkspace {
                            id: workspace_id,
                            location: SerializedWorkspaceLocation::Local(location),
                            session_id,
                            window_id: window_id
                                .and_then(|window_id| u64::try_from(window_id).ok()),
                        })
                    })
            })
            .ok()
            .flatten()
    }

    pub(crate) async fn initialize_schema(&self) -> anyhow::Result<()> {
        self.0
            .write(|connection| {
                connection.with_savepoint("initialize_workspace_schema", || {
                    connection
                        .exec(indoc! {"
                            CREATE TABLE IF NOT EXISTS workspace(
                                id INTEGER PRIMARY KEY,
                                location BLOB UNIQUE,
                                session_id TEXT,
                                window_id INTEGER,
                                activation_order INTEGER NOT NULL DEFAULT 0,
                                timestamp TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                            ) STRICT
                        "})
                        .context("failed to set up workspace table initialization")
                        .and_then(|mut f| f())
                        .context("failed to initialize workspace persistence table")?;

                    connection
                        .exec(indoc! {"
                            CREATE INDEX IF NOT EXISTS workspace_activation_order_idx
                            ON workspace(activation_order DESC)
                        "})
                        .context("failed to set up workspace activation order index initialization")
                        .and_then(|mut f| f())
                        .context("failed to initialize workspace activation order index")?;

                    connection
                        .exec(indoc! {"
                            CREATE INDEX IF NOT EXISTS workspace_timestamp_idx
                            ON workspace(timestamp DESC)
                        "})
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
        self.0.read(|connection| {
            let rows = connection
                .select::<(i64, PathBuf, String)>(indoc! {"
                    SELECT id, location, timestamp
                    FROM workspace
                    WHERE location IS NOT NULL
                    ORDER BY activation_order DESC
                "})
                .context("failed to prepare recent workspace query")
                .and_then(|mut f| f())
                .context("failed to execute recent workspace query")?;

            Ok(rows
                .into_iter()
                .map(|(id, location, timestamp)| {
                    (
                        WorkspaceId::from(id),
                        SerializedWorkspaceLocation::Local(location),
                        parse_timestamp(&timestamp),
                    )
                })
                .collect())
        })
    }

    fn session_workspaces(
        &self,
        session_id: String,
    ) -> anyhow::Result<Vec<(WorkspaceId, SerializedWorkspaceLocation, Option<u64>)>> {
        self.0.read(|connection| {
            let rows = connection
                .select_bound::<String, (i64, PathBuf, Option<i64>)>(indoc! {"
                        SELECT id, location, window_id
                        FROM workspace
                        WHERE session_id = ?1 AND location IS NOT NULL
                        ORDER BY activation_order DESC
                    "})
                .context("failed to prepare session workspaces query")
                .and_then(|mut f| f(session_id))
                .context("failed to execute session workspaces query")?;

            Ok(rows
                .into_iter()
                .map(|(workspace_id, location, window_id)| {
                    (
                        WorkspaceId::from(workspace_id),
                        SerializedWorkspaceLocation::Local(location),
                        window_id.and_then(|window_id| u64::try_from(window_id).ok()),
                    )
                })
                .collect())
        })
    }

    pub async fn delete_workspace_by_id(&self, workspace_id: WorkspaceId) -> anyhow::Result<()> {
        self.0
            .write(move |connection| {
                connection
                    .exec_bound("DELETE FROM workspace WHERE id = ?1")
                    .context("failed to prepare workspace deletion query")
                    .and_then(|mut f| f([i64::from(workspace_id)]))
                    .context("failed to delete workspace by id")?;

                Ok(())
            })
            .await
    }

    async fn all_paths_exist_with_a_directory(
        paths: &[PathBuf],
        fs: &dyn Fs,
        timestamp: Option<DateTime<Utc>>,
    ) -> bool {
        let mut any_directory = false;

        for path in paths {
            match fs.metadata(path).await.ok().flatten() {
                None => {
                    return timestamp.is_some_and(|timestamp| {
                        Utc::now() - timestamp < chrono::Duration::days(7)
                    });
                }
                Some(metadata) => {
                    if metadata.is_dir {
                        any_directory = true;
                    }
                }
            }
        }

        any_directory
    }
}

fn parse_timestamp(text: &str) -> DateTime<Utc> {
    NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S")
        .map(|naive| naive.and_utc())
        .unwrap_or_else(|_| Utc::now())
}

fn serialize_window_id(window_id: Option<u64>) -> anyhow::Result<Option<i64>> {
    window_id
        .map(|window_id| i64::try_from(window_id).context("window id exceeds SQLite INTEGER range"))
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    use gpui::{TestAppContext, WindowId};
    use serde_json::json;

    #[cfg(unix)]
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    use std::sync::Arc;

    use fs::TempFs;
    use util_macros::path;

    use crate::{CloseWindow, OpenMode, Root, SharedState, Workspace};

    #[gpui::test]
    async fn test_save_workspace_deduplicates_paths(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        temp_fs.insert_tree("project", json!(null));

        let workspace_db =
            WorkspaceDb::open_test_db("test_save_workspace_deduplicates_paths").await;
        workspace_db
            .clear_recent_workspaces()
            .await
            .expect("workspace recent list should clear");

        let project_path = temp_fs.path().join("project");
        workspace_db
            .save_workspace(SerializedWorkspace {
                id: WorkspaceId::from(1),
                location: SerializedWorkspaceLocation::Local(project_path.clone()),
                session_id: Some("session-a".to_string()),
                window_id: Some(10),
            })
            .await;
        workspace_db
            .save_workspace(SerializedWorkspace {
                id: WorkspaceId::from(1),
                location: SerializedWorkspaceLocation::Local(project_path.clone()),
                session_id: Some("session-a".to_string()),
                window_id: Some(10),
            })
            .await;

        let recent_workspaces = workspace_db
            .recent_workspaces_on_disk(&temp_fs)
            .await
            .expect("recent workspace query should succeed");

        let Some((workspace_id, location, _timestamp)) = recent_workspaces.first() else {
            panic!("expected a recent workspace");
        };
        assert_eq!(*workspace_id, WorkspaceId::from(1));
        assert_eq!(location.path(), project_path);
        assert_eq!(
            workspace_db
                .recent_workspace_count()
                .expect("workspace count query should succeed"),
            1
        );
    }

    #[cfg(unix)]
    #[gpui::test]
    async fn test_save_workspace_preserves_non_utf8_paths(_cx: &mut TestAppContext) {
        let workspace_db =
            WorkspaceDb::open_test_db("test_save_workspace_preserves_non_utf8_paths").await;
        let path = PathBuf::from(OsString::from_vec(vec![0x2f, 0x74, 0x6d, 0x70, 0x2f, 0x80]));

        workspace_db
            .save_workspace(SerializedWorkspace {
                id: WorkspaceId::from(1),
                location: SerializedWorkspaceLocation::Local(path.clone()),
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

        let temp_fs = Arc::new(TempFs::new(cx.executor()));
        let shared_state = cx.update(|cx| Arc::new(SharedState::test_new(temp_fs.clone(), cx)));
        crate::tests::init_test(shared_state.clone(), cx);

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
        root.update_in(cx, |root, window, cx| {
            root.replace_workspace(window, cx);
        });
        cx.run_until_parked();

        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());
        let workspace_id = workspace
            .read_with(cx, |workspace, _| workspace.database_id())
            .expect("workspace should have a database_id after initialization");

        let project_path = temp_fs.path().join(path!("project"));
        let open_workspace = workspace.update_in(cx, |workspace, window, cx| {
            workspace.open_workspace_for_path(project_path.clone(), OpenMode::Activate, window, cx)
        });
        open_workspace.await.expect("workspace open should succeed");

        workspace
            .read_with(cx, |workspace, cx| workspace.worktree_scan_complete(cx))
            .await;

        workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.flush_serialization(window, cx)
            })
            .await;

        let serialized = workspace_db.workspace_for_id(workspace_id);
        assert!(
            serialized.is_some(),
            "Workspace should be fully serialized in the DB after database_id assignment"
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

        let workspace_db = WorkspaceDb::open_test_db("test_last_session_workspace_locations").await;

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
                &temp_fs,
            )
            .await
            .expect("last session workspace query should succeed");

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
            WorkspaceDb::open_test_db("test_last_session_workspace_locations_skips_missing_paths")
                .await;

        workspace_db
            .save_workspace(SerializedWorkspace {
                id: WorkspaceId::from(1),
                location: SerializedWorkspaceLocation::Local(temp_fs.path().join(path!("project"))),
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
                session_id: Some("session-uuid".to_string()),
                window_id: Some(2),
            })
            .await;

        let locations = workspace_db
            .last_session_workspace_locations("session-uuid", None, &temp_fs)
            .await
            .expect("last session workspace query should succeed");

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

        let temp_fs = Arc::new(TempFs::new(cx.executor()));
        let shared_state = cx.update(|cx| Arc::new(SharedState::test_new(temp_fs.clone(), cx)));
        crate::tests::init_test(shared_state.clone(), cx);

        temp_fs.insert_tree(path!("project"), json!(null));

        let workspace_db = cx.update(|cx| WorkspaceDb::global(cx));
        let workspace_id = workspace_db.next_id().await.unwrap();
        let (root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(Workspace::create(workspace_id, shared_state, window, cx))
        });
        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());
        let project_path = temp_fs.path().join(path!("project"));

        workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.open_workspace_for_path(
                    project_path.clone(),
                    OpenMode::Activate,
                    window,
                    cx,
                )
            })
            .await
            .expect("workspace open should succeed");
        workspace
            .read_with(cx, |workspace, cx| workspace.worktree_scan_complete(cx))
            .await;
        workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.flush_serialization(window, cx)
            })
            .await;

        let workspace_id = workspace
            .read_with(cx, |workspace, _| workspace.database_id())
            .expect("workspace should have a database_id after opening");
        let serialized_workspace = workspace_db
            .workspace_for_id(workspace_id)
            .expect("workspace should be serialized before replacement");

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

        let temp_fs = Arc::new(TempFs::new(cx.executor()));
        let shared_state = cx.update(|cx| Arc::new(SharedState::test_new(temp_fs.clone(), cx)));
        crate::tests::init_test(shared_state.clone(), cx);

        temp_fs.insert_tree(path!("project"), json!(null));

        let workspace_db = cx.update(|cx| WorkspaceDb::global(cx));
        let workspace_id = workspace_db.next_id().await.unwrap();
        let (root, cx) = cx.add_window_view(move |window, cx| {
            Root::new(Workspace::create(workspace_id, shared_state, window, cx))
        });
        let workspace = root.update_in(cx, |root, _, _| root.workspace().clone());
        let project_path = temp_fs.path().join(path!("project"));

        workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.open_workspace_for_path(
                    project_path.clone(),
                    OpenMode::Activate,
                    window,
                    cx,
                )
            })
            .await
            .expect("workspace open should succeed");
        workspace
            .read_with(cx, |workspace, cx| workspace.worktree_scan_complete(cx))
            .await;
        workspace
            .update_in(cx, |workspace, window, cx| {
                workspace.flush_serialization(window, cx)
            })
            .await;

        let workspace_id = workspace
            .read_with(cx, |workspace, _| workspace.database_id())
            .expect("workspace should have a database_id after opening");
        let serialized_workspace = workspace_db
            .workspace_for_id(workspace_id)
            .expect("workspace should be serialized before close");

        assert!(serialized_workspace.session_id.is_some());
        assert!(serialized_workspace.window_id.is_some());

        root.update_in(cx, |root, window, cx| {
            root.close_window(&CloseWindow, window, cx)
        });
        cx.run_until_parked();

        let serialized_workspace = workspace_db
            .workspace_for_id(workspace_id)
            .expect("workspace row should remain after close");

        assert_eq!(serialized_workspace.session_id, None);
        assert_eq!(serialized_workspace.window_id, None);
    }
}
