use std::path::PathBuf;

use db::{
    Bind, Column, Row, Statement, StaticColumnCount, ThreadSafeConnection, query,
    sql::domain::Domain, sql_macros::sql,
};
use workspace::{ItemId, WorkspaceId};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SerializedEditor {
    pub(crate) absolute_path: PathBuf,
}

impl StaticColumnCount for SerializedEditor {}

impl Bind for SerializedEditor {
    fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
        statement.bind(&self.absolute_path, start_index)
    }
}

impl Column for SerializedEditor {
    fn column(row: &mut Row<'_, '_>, start_index: i32) -> anyhow::Result<(Self, i32)> {
        let (absolute_path, next_index) = Column::column(row, start_index)?;
        Ok((Self { absolute_path }, next_index))
    }
}

pub(crate) struct EditorDb(ThreadSafeConnection);

impl EditorDb {
    query! {
        pub(crate) fn load_serialized_editor(
            item_id: ItemId,
            workspace_id: WorkspaceId,
        ) -> anyhow::Result<Option<SerializedEditor>> {
            SELECT path
            FROM editor
            WHERE id = ? AND workspace_id = ?
        }
    }

    query! {
        pub(crate) async fn save_serialized_editor(
            item_id: ItemId,
            workspace_id: WorkspaceId,
            serialized_editor: SerializedEditor,
        ) -> anyhow::Result<()> {
            INSERT INTO editor(
                id,
                workspace_id,
                path
            )
            VALUES (?1, ?2, ?3)
            ON CONFLICT(id, workspace_id)
            DO UPDATE SET
                path = excluded.path
        }
    }
}

impl Domain for EditorDb {
    const NAME: &str = stringify!(EditorDb);
    const MIGRATIONS: &[&str] = &[sql!(
        CREATE TABLE IF NOT EXISTS editor(
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

db::static_connection!(EditorDb, [workspace::WorkspaceDb]);

#[cfg(test)]
mod tests {
    use super::*;

    use gpui::TestAppContext;

    use workspace::WorkspaceDb;

    #[gpui::test]
    async fn test_save_and_load_serialized_editor(cx: &mut TestAppContext) {
        let workspace_db = cx.update(|cx| WorkspaceDb::global(cx));
        let workspace_id = workspace_db.next_id().await.unwrap();
        let editor_db = cx.update(|cx| EditorDb::global(cx));

        let serialized_editor = SerializedEditor {
            absolute_path: PathBuf::from("settings.jsonc"),
        };
        editor_db
            .save_serialized_editor(1234, workspace_id, serialized_editor.clone())
            .await
            .unwrap();
        assert_eq!(
            editor_db
                .load_serialized_editor(1234, workspace_id)
                .unwrap(),
            Some(serialized_editor)
        );

        let serialized_editor = SerializedEditor {
            absolute_path: PathBuf::from("renamed-settings.jsonc"),
        };
        editor_db
            .save_serialized_editor(1234, workspace_id, serialized_editor.clone())
            .await
            .unwrap();
        assert_eq!(
            editor_db
                .load_serialized_editor(1234, workspace_id)
                .unwrap(),
            Some(serialized_editor)
        );
    }
}
