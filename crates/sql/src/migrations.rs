use anyhow::Context;
use indoc::{formatdoc, indoc};
use libsqlite3_sys as sqlite3;
use sha2::{Digest, Sha256};
use std::{borrow::Cow, ffi::CString, fmt};

use crate::connection::Connection;

fn normalize_migration(migration: &str) -> Cow<'_, str> {
    let migration = migration.trim();
    if !migration.as_bytes().contains(&b'\r') {
        return Cow::Borrowed(migration);
    }

    let mut normalized = String::with_capacity(migration.len());
    let mut characters = migration.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\r' {
            if characters.peek() == Some(&'\n') {
                characters.next();
            }
            normalized.push('\n');
        } else {
            normalized.push(character);
        }
    }
    Cow::Owned(normalized)
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct MigrationChecksum([u8; 32]);

impl MigrationChecksum {
    fn new(migration: &str) -> Self {
        Self(Sha256::digest(migration.as_bytes()).into())
    }

    fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl From<&[u8; 32]> for MigrationChecksum {
    fn from(checksum: &[u8; 32]) -> Self {
        Self(*checksum)
    }
}

impl fmt::Display for MigrationChecksum {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&hex::encode(self.0))
    }
}

fn prepare_migration(migration: &str) -> (Cow<'_, str>, MigrationChecksum) {
    let migration = normalize_migration(migration);
    let checksum = MigrationChecksum::new(&migration);
    (migration, checksum)
}

impl Connection {
    fn eager_exec(&self, sql: &str) -> anyhow::Result<()> {
        let sql_cstring = CString::new(sql).context("failed to create sqlite cstr")?;
        // SAFETY: self.sqlite3 is a valid SQLite handle and sql_cstring is a
        // NUL-terminated string that remains valid for the duration of this call.
        let result_code = unsafe {
            sqlite3::sqlite3_exec(
                self.sqlite3,
                sql_cstring.as_ptr(),
                None,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        self.ensure_ok(result_code)
            .with_context(|| format!("failed to execute migration query:\n{sql}"))?;

        Ok(())
    }

    pub fn migrate(
        &self,
        domain: &'static str,
        migrations: &[&'static str],
        should_allow_migration_change: &mut dyn FnMut(usize, &str, &str) -> bool,
    ) -> anyhow::Result<()> {
        self.with_savepoint("migrating", || {
            self.exec(indoc! {"
                    CREATE TABLE IF NOT EXISTS migration (
                        id INTEGER PRIMARY KEY,
                        domain TEXT NOT NULL,
                        step INTEGER NOT NULL,
                        checksum BLOB NOT NULL,
                        UNIQUE(domain, step)
                    )
                "})
                .and_then(|mut stmt| stmt())?;

            let completed_migrations = self
                .select_bound::<&str, (String, usize, [u8; 32])>(indoc! {"
                    SELECT domain, step, checksum FROM migration
                    WHERE domain = ?
                    ORDER BY step
                "})
                .and_then(|mut stmt| stmt(domain))?;

            let mut store_completed_migration =
                self.exec_bound("INSERT INTO migration (domain, step, checksum) VALUES (?, ?, ?)")?;

            let mut did_migrate = false;
            for (index, migration) in migrations.iter().enumerate() {
                let (migration, proposed_checksum) = prepare_migration(migration);
                if let Some((_, _, stored_checksum)) = completed_migrations.get(index) {
                    let stored_checksum = MigrationChecksum::from(stored_checksum);
                    if stored_checksum == proposed_checksum {
                        continue;
                    }

                    let stored_checksum = stored_checksum.to_string();
                    let proposed_checksum = proposed_checksum.to_string();
                    if should_allow_migration_change(index, &stored_checksum, &proposed_checksum) {
                        continue;
                    }

                    anyhow::bail!(formatdoc! {"
                        Migration changed for {domain} at step {index}

                        Stored checksum:
                        {stored_checksum}

                        Proposed checksum:
                        {proposed_checksum}
                    "});
                }

                log::info!("Running migration {domain} step {index} ({proposed_checksum})");
                self.eager_exec(&migration)?;
                did_migrate = true;
                store_completed_migration((domain, index, proposed_checksum.as_bytes()))?;
            }

            if did_migrate {
                self.delete_rows_with_orphaned_foreign_key_references()?;
                self.exec("PRAGMA foreign_key_check;")
                    .and_then(|mut stmt| stmt())?;
            }

            Ok(())
        })
    }

    fn delete_rows_with_orphaned_foreign_key_references(&self) -> anyhow::Result<()> {
        let foreign_key_info: Vec<(String, String, String, String)> = self
            .select(
                r#"
                SELECT DISTINCT
                    schema.name as child_table,
                    foreign_keys.[from] as child_key,
                    foreign_keys.[table] as parent_table,
                    foreign_keys.[to] as parent_key
                FROM sqlite_schema schema
                JOIN pragma_foreign_key_list(schema.name) foreign_keys
                WHERE
                    schema.type = 'table' AND
                    schema.name NOT LIKE "sqlite_%"
            "#,
            )
            .and_then(|mut stmt| stmt())?;

        if !foreign_key_info.is_empty() {
            log::info!(
                "Found {} foreign key relationships to check",
                foreign_key_info.len()
            );
        }

        for (child_table, child_key, parent_table, parent_key) in foreign_key_info {
            self.exec(&format!(
                "
                DELETE FROM {child_table}
                WHERE {child_key} IS NOT NULL AND {child_key} NOT IN
                (SELECT {parent_key} FROM {parent_table})
                "
            ))
            .and_then(|mut stmt| stmt())?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disallow_migration_change(_index: usize, _old_checksum: &str, _new_checksum: &str) -> bool {
        false
    }

    #[test]
    fn test_migrations_are_added_to_table() {
        let connection = Connection::open_memory(Some("test_migrations_are_added_to_table"));
        let first_migration = indoc! {"
            CREATE TABLE test1 (
                a TEXT,
                b TEXT
            )
        "};
        let second_migration = indoc! {"
            CREATE TABLE test2 (
                c TEXT,
                d TEXT
            )
        "};

        connection
            .migrate("test", &[first_migration], &mut disallow_migration_change)
            .unwrap();

        let (_, first_checksum) = prepare_migration(first_migration);
        assert_eq!(
            &connection
                .select::<[u8; 32]>("SELECT (checksum) FROM migration")
                .and_then(|mut stmt| stmt())
                .unwrap()[..],
            &[*first_checksum.as_bytes()],
        );

        connection
            .migrate(
                "test",
                &[first_migration, second_migration],
                &mut disallow_migration_change,
            )
            .unwrap();

        let (_, second_checksum) = prepare_migration(second_migration);
        assert_eq!(
            &connection
                .select::<[u8; 32]>("SELECT (checksum) FROM migration")
                .and_then(|mut stmt| stmt())
                .unwrap()[..],
            &[*first_checksum.as_bytes(), *second_checksum.as_bytes()],
        );
    }

    #[test]
    fn test_migration_setup() {
        let connection = Connection::open_memory(Some("test_migration_setup"));

        connection
            .exec(indoc! {"
                CREATE TABLE IF NOT EXISTS migration (
                    id INTEGER PRIMARY KEY,
                    domain TEXT NOT NULL,
                    step INTEGER NOT NULL,
                    checksum BLOB NOT NULL,
                    UNIQUE(domain, step)
                );
            "})
            .and_then(|mut stmt| stmt())
            .unwrap();

        let mut store_completed_migration = connection
            .exec_bound(indoc! {"
                INSERT INTO migration (domain, step, checksum)
                VALUES (?, ?, ?)
            "})
            .unwrap();

        let domain = "test_domain";
        for migration_index in 0..5 {
            connection
                .exec(&format!(
                    "CREATE TABLE table{migration_index} ( test TEXT );"
                ))
                .and_then(|mut stmt| stmt())
                .unwrap();

            let checksum = [u8::try_from(migration_index).unwrap(); 32];
            store_completed_migration((domain, migration_index, &checksum)).unwrap();
        }
    }

    #[test]
    fn test_completed_migrations_do_not_rerun() {
        let connection = Connection::open_memory(Some("test_completed_migrations_do_not_rerun"));

        connection
            .exec(indoc! {"
                CREATE TABLE test_table (
                    test_column INTEGER
                );
            "})
            .and_then(|mut stmt| stmt())
            .unwrap();
        connection
            .exec(indoc! {"
                INSERT INTO test_table (test_column) VALUES (1);
            "})
            .and_then(|mut stmt| stmt())
            .unwrap();

        assert_eq!(
            connection
                .select_row::<usize>("SELECT * FROM test_table")
                .and_then(|mut stmt| stmt())
                .unwrap(),
            Some(1)
        );

        connection
            .migrate(
                "test",
                &["DELETE FROM test_table"],
                &mut disallow_migration_change,
            )
            .unwrap();

        assert_eq!(
            connection
                .select_row::<usize>("SELECT * FROM test_table")
                .and_then(|mut stmt| stmt())
                .unwrap(),
            None
        );

        connection
            .exec("INSERT INTO test_table (test_column) VALUES (2)")
            .and_then(|mut stmt| stmt())
            .unwrap();

        connection
            .migrate(
                "test",
                &["DELETE FROM test_table"],
                &mut disallow_migration_change,
            )
            .unwrap();

        assert_eq!(
            connection
                .select_row::<usize>("SELECT * FROM test_table")
                .and_then(|mut stmt| stmt())
                .unwrap(),
            Some(2)
        );
    }

    #[test]
    fn test_changed_migration_fails() {
        let connection = Connection::open_memory(Some("test_changed_migration_fails"));
        let old_migration = "CREATE TABLE test (col INTEGER)";
        let new_migration = "CREATE TABLE test (color INTEGER)";

        connection
            .migrate(
                "test migration",
                &[old_migration, "INSERT INTO test (col) VALUES (1)"],
                &mut disallow_migration_change,
            )
            .unwrap();

        let mut migration_changed = false;
        let (_, old_checksum) = prepare_migration(old_migration);
        let (_, new_checksum) = prepare_migration(new_migration);
        let old_checksum = old_checksum.to_string();
        let new_checksum = new_checksum.to_string();

        let second_migration_result = connection.migrate(
            "test migration",
            &[new_migration, "INSERT INTO test (color) VALUES (1)"],
            &mut |_index, old, new| {
                assert_eq!(old, old_checksum.as_str());
                assert_eq!(new, new_checksum.as_str());
                migration_changed = true;
                false
            },
        );

        assert!(migration_changed);
        assert!(second_migration_result.is_err());
    }

    #[test]
    fn test_create_alter_drop() {
        let connection = Connection::open_memory(Some("test_create_alter_drop"));

        connection
            .migrate(
                "first_migration",
                &["CREATE TABLE table1(a TEXT) STRICT;"],
                &mut disallow_migration_change,
            )
            .unwrap();

        connection
            .exec("INSERT INTO table1(a) VALUES (\"test text\");")
            .and_then(|mut stmt| stmt())
            .unwrap();

        connection
            .migrate(
                "second_migration",
                &[indoc! {"
                    CREATE TABLE table2(b TEXT) STRICT;

                    INSERT INTO table2 (b)
                    SELECT a FROM table1;

                    DROP TABLE table1;

                    ALTER TABLE table2 RENAME TO table1;
                "}],
                &mut disallow_migration_change,
            )
            .unwrap();

        let result = connection
            .select_row::<String>("SELECT b FROM table1")
            .and_then(|mut stmt| stmt())
            .unwrap();

        assert_eq!(result.as_deref(), Some("test text"));
    }
}
