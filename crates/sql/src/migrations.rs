use anyhow::Context;
use indoc::{formatdoc, indoc};
use libsqlite3_sys as sqlite3;
use sqlformat::{FormatOptions, QueryParams};
use std::ffi::CString;

use crate::connection::Connection;

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
                CREATE TABLE IF NOT EXISTS migrations (
                    domain TEXT,
                    step INTEGER,
                    migration TEXT
                )
            "})?()?;

            let completed_migrations =
                self.select_bound::<&str, (String, usize, String)>(indoc! {"
                    SELECT domain, step, migration FROM migrations
                    WHERE domain = ?
                    ORDER BY step
                "})?(domain)?;

            let mut store_completed_migration = self
                .exec_bound("INSERT INTO migrations (domain, step, migration) VALUES (?, ?, ?)")?;

            let mut did_migrate = false;
            for (index, migration) in migrations.iter().enumerate() {
                let migration =
                    sqlformat::format(migration, &QueryParams::None, &FormatOptions::default());
                if let Some((_, _, completed_migration)) = completed_migrations.get(index) {
                    let completed_migration = sqlformat::format(
                        completed_migration,
                        &QueryParams::None,
                        &FormatOptions::default(),
                    );
                    if completed_migration == migration
                        || should_allow_migration_change(index, &completed_migration, &migration)
                    {
                        continue;
                    }

                    anyhow::bail!(formatdoc! {"
                        Migration changed for {domain} at step {index}

                        Stored migration:
                        {completed_migration}

                        Proposed migration:
                        {migration}
                    "});
                }

                self.eager_exec(&migration)?;
                did_migrate = true;
                store_completed_migration((domain, index, migration))?;
            }

            if did_migrate {
                self.delete_rows_with_orphaned_foreign_key_references()?;
                self.exec("PRAGMA foreign_key_check;")?()?;
            }

            Ok(())
        })
    }

    fn delete_rows_with_orphaned_foreign_key_references(&self) -> anyhow::Result<()> {
        let foreign_key_info: Vec<(String, String, String, String)> = self.select(
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
        )?()?;

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
            ))?()?;
        }

        Ok(())
    }
}
