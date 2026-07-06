use std::path::PathBuf;

use db::{
    Bind, Column, Row, Statement, StaticColumnCount, ThreadSafeConnection, query,
    sql::domain::Domain, sql_macros::sql,
};
use workspace::{ItemId, WorkspaceId};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SerializedRequestEditor {
    pub(crate) absolute_path: PathBuf,
}

impl StaticColumnCount for SerializedRequestEditor {}

impl Bind for SerializedRequestEditor {
    fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
        statement.bind(&self.absolute_path, start_index)
    }
}

impl Column for SerializedRequestEditor {
    fn column(row: &mut Row<'_, '_>, start_index: i32) -> anyhow::Result<(Self, i32)> {
        let (absolute_path, next_index) = Column::column(row, start_index)?;
        Ok((Self { absolute_path }, next_index))
    }
}

pub(crate) struct RequestEditorDb(ThreadSafeConnection);

impl RequestEditorDb {
    query! {
        pub(crate) fn load_serialized_request_editor(
            item_id: ItemId,
            workspace_id: WorkspaceId,
        ) -> anyhow::Result<Option<SerializedRequestEditor>> {
            SELECT path
            FROM request_editor
            WHERE id = ? AND workspace_id = ?
        }
    }

    query! {
        pub(crate) async fn save_serialized_request_editor(
            item_id: ItemId,
            workspace_id: WorkspaceId,
            serialized_request_editor: SerializedRequestEditor,
        ) -> anyhow::Result<()> {
            INSERT INTO request_editor(id, workspace_id, path)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(id, workspace_id)
            DO UPDATE SET
                path = excluded.path
        }
    }
}

impl Domain for RequestEditorDb {
    const NAME: &str = stringify!(RequestEditorDb);
    const MIGRATIONS: &[&str] = &[sql!(
        CREATE TABLE IF NOT EXISTS request_editor(
            id INTEGER NOT NULL,
            workspace_id INTEGER NOT NULL,
            path BLOB NOT NULL,
            PRIMARY KEY(id, workspace_id),
            FOREIGN KEY(workspace_id) REFERENCES workspace(id)
            ON DELETE CASCADE
            ON UPDATE CASCADE
        ) STRICT;
    )];
}

db::static_connection!(RequestEditorDb, [workspace::WorkspaceDb]);
