use anyhow::Context;
use jiff::Timestamp;

use db::{Column, Row, ThreadSafeConnection, query, sql_macros::sql};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SerializedCommandUsage {
    pub(crate) command_name: String,
    pub(crate) invocations: u16,
    pub(crate) last_invoked: Timestamp,
}

impl Column for SerializedCommandUsage {
    fn column(row: &mut Row<'_, '_>, start_index: i32) -> anyhow::Result<(Self, i32)> {
        let (command_name, next_index) = String::column(row, start_index)?;
        let (invocations, next_index) = u16::column(row, next_index)?;
        let (last_invoked_raw, next_index) = i64::column(row, next_index)?;

        Ok((
            Self {
                command_name,
                invocations,
                last_invoked: Timestamp::from_second(last_invoked_raw)?,
            },
            next_index,
        ))
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SerializedCommandInvocation {
    pub(crate) command_name: String,
    pub(crate) user_query: String,
    pub(crate) last_invoked: Timestamp,
}

#[cfg(test)]
impl Column for SerializedCommandInvocation {
    fn column(row: &mut Row<'_, '_>, start_index: i32) -> anyhow::Result<(Self, i32)> {
        let (command_name, next_index) = String::column(row, start_index)?;
        let (user_query, next_index) = String::column(row, next_index)?;
        let (last_invoked_raw, next_index) = i64::column(row, next_index)?;

        Ok((
            Self {
                command_name,
                user_query,
                last_invoked: Timestamp::from_second(last_invoked_raw)?,
            },
            next_index,
        ))
    }
}

pub(crate) struct CommandPaletteDB(ThreadSafeConnection);

impl CommandPaletteDB {
    pub(crate) async fn initialize_schema(&self) -> anyhow::Result<()> {
        self.0
            .write(|connection| {
                connection.with_savepoint("initialize_command_palette_schema", || {
                    connection
                        .exec(sql!(
                            CREATE TABLE IF NOT EXISTS command_invocation(
                                id INTEGER PRIMARY KEY,
                                command_name TEXT NOT NULL,
                                user_query TEXT NOT NULL,
                                last_invoked INTEGER DEFAULT (unixepoch()) NOT NULL
                            ) STRICT
                        ))
                        .context("failed to set up command invocation table initialization")
                        .and_then(|mut f| f())
                        .context("failed to initialize command invocation table")?;

                    Ok(())
                })
            })
            .await
    }

    pub(crate) async fn write_command_invocation(
        &self,
        command_name: impl Into<String>,
        user_query: impl Into<String>,
    ) -> anyhow::Result<()> {
        let command_name = command_name.into();
        let user_query = user_query.into();
        log::debug!(
            "Writing command invocation: command_name={command_name}, user_query={user_query}"
        );
        self.write_command_invocation_internal(command_name, user_query)
            .await
    }

    query! {
        pub fn get_command_usage(
            command_name: &str,
        ) -> anyhow::Result<Option<SerializedCommandUsage>> {
            SELECT command_name, COUNT(1), MAX(last_invoked)
            FROM command_invocation
            WHERE command_name = ?
            GROUP BY command_name
        }
    }

    query! {
        async fn write_command_invocation_internal(
            command_name: String,
            user_query: String,
        ) -> anyhow::Result<()> {
            INSERT INTO command_invocation(command_name, user_query) VALUES (?1, ?2);
            DELETE FROM command_invocation
            WHERE id IN (
                SELECT MIN(id)
                FROM command_invocation
                HAVING COUNT(1) > 1000
            )
        }
    }

    query! {
        pub(crate) fn list_commands_used() -> anyhow::Result<Vec<SerializedCommandUsage>> {
            SELECT command_name, COUNT(1), MAX(last_invoked)
            FROM command_invocation
            GROUP BY command_name
            ORDER BY COUNT(1) DESC
        }
    }

    query! {
        pub(crate) fn list_recent_queries() -> anyhow::Result<Vec<String>> {
            SELECT user_query
            FROM command_invocation
            WHERE LENGTH(user_query) > 0
            GROUP BY user_query
            ORDER BY MAX(last_invoked) ASC
        }
    }

    #[cfg(test)]
    query! {
        pub(crate) fn get_last_invoked(
            command_name: &str,
        ) -> anyhow::Result<Option<SerializedCommandInvocation>> {
            SELECT command_name, user_query, last_invoked
            FROM command_invocation
            WHERE command_name = ?
            ORDER BY last_invoked DESC
            LIMIT 1
        }
    }
}

db::static_connection!(CommandPaletteDB, []);

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    async fn test_command_invocation_is_recorded() {
        let db = CommandPaletteDB::test_open("test_command_invocation_is_recorded").await;
        let retrieved_command = db.get_last_invoked("zaku: open settings file").unwrap();

        assert!(retrieved_command.is_none());

        db.write_command_invocation("zaku: open settings file", "")
            .await
            .unwrap();

        let retrieved_command = db
            .get_last_invoked("zaku: open settings file")
            .unwrap()
            .unwrap();

        assert_eq!(retrieved_command.command_name, "zaku: open settings file");
        assert_eq!(retrieved_command.user_query, "");
    }

    #[gpui::test]
    async fn test_gets_usage_history() {
        let db = CommandPaletteDB::test_open("test_gets_usage_history").await;

        db.write_command_invocation("zaku: open settings file", "settings")
            .await
            .unwrap();
        db.write_command_invocation("zaku: open settings file", "open settings")
            .await
            .unwrap();

        let retrieved_command = db
            .get_last_invoked("zaku: open settings file")
            .unwrap()
            .unwrap();

        let command_usage = db
            .get_command_usage("zaku: open settings file")
            .unwrap()
            .unwrap();

        assert_eq!(command_usage.command_name, "zaku: open settings file");
        assert_eq!(command_usage.invocations, 2);
        assert_eq!(command_usage.last_invoked, retrieved_command.last_invoked);
    }

    #[gpui::test]
    async fn test_commands_ordered_by_invocation_count() {
        let db = CommandPaletteDB::test_open("test_commands_ordered_by_invocation_count").await;

        let commands = db.list_commands_used().unwrap();
        assert!(commands.is_empty());

        db.write_command_invocation("zaku: about", "about")
            .await
            .unwrap();
        db.write_command_invocation("workspace: send request", "send request")
            .await
            .unwrap();
        db.write_command_invocation("workspace: send request", "send request")
            .await
            .unwrap();

        let commands = db.list_commands_used().unwrap();

        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].command_name, "workspace: send request");
        assert_eq!(commands[0].invocations, 2);
        assert_eq!(commands[1].command_name, "zaku: about");
        assert_eq!(commands[1].invocations, 1);
    }

    #[gpui::test]
    async fn test_command_invocations_are_capped() {
        let db = CommandPaletteDB::test_open("test_command_invocations_are_capped").await;

        for _ in 1..=1001 {
            db.write_command_invocation("zaku: open settings file", "settings")
                .await
                .unwrap();
        }

        let command_usage = db
            .get_command_usage("zaku: open settings file")
            .unwrap()
            .unwrap();

        assert_eq!(command_usage.invocations, 1000);
    }
}
