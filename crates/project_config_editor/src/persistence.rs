use std::path::PathBuf;

use db::{
    Bind, Column, Row, Statement, StaticColumnCount, ThreadSafeConnection, query,
    sql::domain::Domain, sql_macros::sql,
};
use workspace::{ItemId, WorkspaceId};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SerializedProjectConfigEditor {
    pub(crate) absolute_path: PathBuf,
}

impl StaticColumnCount for SerializedProjectConfigEditor {}

impl Bind for SerializedProjectConfigEditor {
    fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
        statement.bind(&self.absolute_path, start_index)
    }
}

impl Column for SerializedProjectConfigEditor {
    fn column(row: &mut Row<'_, '_>, start_index: i32) -> anyhow::Result<(Self, i32)> {
        let (absolute_path, next_index) = Column::column(row, start_index)?;
        Ok((Self { absolute_path }, next_index))
    }
}

pub(crate) struct ProjectConfigEditorDb(ThreadSafeConnection);

impl ProjectConfigEditorDb {
    query! {
        pub(crate) fn load_serialized_project_config_editor(
            item_id: ItemId,
            workspace_id: WorkspaceId,
        ) -> anyhow::Result<Option<SerializedProjectConfigEditor>> {
            SELECT path
            FROM project_config_editor
            WHERE id = ? AND workspace_id = ?
        }
    }

    query! {
        pub(crate) async fn save_serialized_project_config_editor(
            item_id: ItemId,
            workspace_id: WorkspaceId,
            serialized_project_config_editor: SerializedProjectConfigEditor,
        ) -> anyhow::Result<()> {
            INSERT INTO project_config_editor(id, workspace_id, path)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(id, workspace_id)
            DO UPDATE SET
                path = excluded.path
        }
    }
}

impl Domain for ProjectConfigEditorDb {
    const NAME: &str = stringify!(ProjectConfigEditorDb);
    const MIGRATIONS: &[&str] = &[sql!(
        CREATE TABLE IF NOT EXISTS project_config_editor(
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

db::static_connection!(ProjectConfigEditorDb, [workspace::WorkspaceDb]);

#[cfg(test)]
mod tests {
    use super::*;

    use gpui::TestAppContext;

    use workspace::WorkspaceDb;

    #[gpui::test]
    async fn test_save_and_load_serialized_project_config_editor(cx: &mut TestAppContext) {
        let workspace_db = cx.update(|cx| WorkspaceDb::global(cx));
        let workspace_id = workspace_db.next_id().await.unwrap();
        let project_config_editor_db = cx.update(|cx| ProjectConfigEditorDb::global(cx));

        let serialized_project_config_editor = SerializedProjectConfigEditor {
            absolute_path: PathBuf::from(".zaku/environments/Dev.toml"),
        };
        project_config_editor_db
            .save_serialized_project_config_editor(
                1234,
                workspace_id,
                serialized_project_config_editor.clone(),
            )
            .await
            .unwrap();
        assert_eq!(
            project_config_editor_db
                .load_serialized_project_config_editor(1234, workspace_id)
                .unwrap(),
            Some(serialized_project_config_editor)
        );

        let serialized_project_config_editor = SerializedProjectConfigEditor {
            absolute_path: PathBuf::from(".zaku/environments/Staging.toml"),
        };
        project_config_editor_db
            .save_serialized_project_config_editor(
                1234,
                workspace_id,
                serialized_project_config_editor.clone(),
            )
            .await
            .unwrap();
        assert_eq!(
            project_config_editor_db
                .load_serialized_project_config_editor(1234, workspace_id)
                .unwrap(),
            Some(serialized_project_config_editor)
        );
    }
}
