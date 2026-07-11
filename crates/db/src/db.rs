pub mod kv;
pub mod query;

pub use anyhow;
pub use gpui::App;
pub use inventory;
pub use sql::{
    self,
    bindable::{Bind, Column, StaticColumnCount},
    connection::Connection,
    statement::{Row, SqlType, Statement},
    thread_safe_connection::{ThreadSafeConnection, background_thread_queue, locking_queue},
};
pub use sql_macros;

use anyhow::Context;
use gpui::Global;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{
        LazyLock,
        atomic::{AtomicBool, Ordering},
    },
};

use sql::{domain::Migrator, thread_safe_connection::ConnectionTarget};
use sql_macros::sql;

pub struct DomainMigration {
    pub name: &'static str,
    pub migrations: &'static [&'static str],
    pub dependencies: &'static [&'static str],
    pub should_allow_migration_change: fn(usize, &str, &str) -> bool,
}

inventory::collect!(DomainMigration);

pub struct AppMigrator;

impl Migrator for AppMigrator {
    fn migrate(connection: &Connection) -> anyhow::Result<()> {
        let registrations: Vec<&DomainMigration> = inventory::iter::<DomainMigration>().collect();
        let sorted = topological_sort(&registrations);
        for registration in &sorted {
            let mut should_allow_migration_change = registration.should_allow_migration_change;
            connection.migrate(
                registration.name,
                registration.migrations,
                &mut should_allow_migration_change,
            )?;
        }
        Ok(())
    }
}

fn topological_sort<'a>(registrations: &[&'a DomainMigration]) -> Vec<&'a DomainMigration> {
    fn visit<'a>(
        name: &str,
        registrations: &[&'a DomainMigration],
        sorted: &mut Vec<&'a DomainMigration>,
        visited: &mut HashSet<&'a str>,
    ) {
        if visited.contains(name) {
            return;
        }
        if let Some(registration) = registrations
            .iter()
            .find(|registration| registration.name == name)
        {
            for dependency in registration.dependencies {
                visit(dependency, registrations, sorted, visited);
            }
            visited.insert(registration.name);
            sorted.push(registration);
        }
    }

    let mut sorted: Vec<&'a DomainMigration> = Vec::new();
    let mut visited: HashSet<&str> = HashSet::new();

    for registration in registrations {
        visit(registration.name, registrations, &mut sorted, &mut visited);
    }
    sorted
}

const CONNECTION_INIT_QUERY: &str = sql!(
    PRAGMA foreign_keys = ON;
);

const DB_INIT_QUERY: &str = sql!(
    PRAGMA journal_mode = WAL;
    PRAGMA busy_timeout = 500;
    PRAGMA case_sensitive_like = TRUE;
    PRAGMA synchronous = NORMAL;
);

const FALLBACK_MEMORY_DB_NAME: &str = "FALLBACK_MEMORY_DB";
const DB_NAME: &str = "db.sqlite";

#[cfg(any(test, feature = "test"))]
static TEST_APP_DATABASE: LazyLock<AppDatabase> = LazyLock::new(AppDatabase::test_new);

static FILE_DB_FAILED: LazyLock<AtomicBool> = LazyLock::new(|| AtomicBool::new(false));

pub async fn open_db<M: Migrator + 'static>(db_dir: &Path) -> ThreadSafeConnection {
    if let Some(connection) = try_open_db::<M>(db_dir).await {
        return connection;
    }

    FILE_DB_FAILED.store(true, Ordering::Release);
    open_fallback_db::<M>().await
}

async fn try_open_db<M: Migrator>(db_dir: &Path) -> Option<ThreadSafeConnection> {
    match ensure_directory(db_dir)
        .await
        .and_then(|()| database_path(db_dir))
    {
        Ok(db_path) => open_main_db::<M>(&db_path).await,
        Err(error) => {
            log::error!(
                "Failed to prepare sqlite database directory {}: {error}",
                db_dir.display()
            );
            None
        }
    }
}

pub fn file_db_failed() -> bool {
    FILE_DB_FAILED.load(Ordering::Acquire)
}

async fn open_main_db<M: Migrator>(path: &Path) -> Option<ThreadSafeConnection> {
    log::trace!("Opening database {}", path.display());
    ThreadSafeConnection::builder::<M>(ConnectionTarget::file(path))
        .with_db_init_query(DB_INIT_QUERY)
        .with_connection_init_query(CONNECTION_INIT_QUERY)
        .build()
        .await
        .map_err(|error| {
            log::error!(
                "Failed to open sqlite database at {}: {error}",
                path.display()
            );
            error
        })
        .ok()
}

async fn open_fallback_db<M: Migrator>() -> ThreadSafeConnection {
    log::warn!("Opening fallback in-memory database");
    ThreadSafeConnection::builder::<M>(ConnectionTarget::memory(FALLBACK_MEMORY_DB_NAME))
        .with_db_init_query(DB_INIT_QUERY)
        .with_connection_init_query(CONNECTION_INIT_QUERY)
        .build()
        .await
        .expect("fallback in-memory database should open")
}

#[cfg(any(test, feature = "test"))]
pub async fn open_test_db<M: Migrator>(db_name: &str) -> ThreadSafeConnection {
    ThreadSafeConnection::builder::<M>(ConnectionTarget::memory(db_name))
        .with_db_init_query(DB_INIT_QUERY)
        .with_connection_init_query(CONNECTION_INIT_QUERY)
        .with_write_queue_constructor(sql::thread_safe_connection::locking_queue())
        .build()
        .await
        .expect("test in-memory database should open")
}

async fn ensure_directory(path: &Path) -> anyhow::Result<()> {
    smol::fs::create_dir_all(path)
        .await
        .with_context(|| format!("failed to create database directory {}", path.display()))
}

fn database_path(db_dir: &Path) -> anyhow::Result<PathBuf> {
    if db_dir.as_os_str().is_empty() {
        anyhow::bail!("database directory path is empty");
    }

    Ok(db_dir.join(DB_NAME))
}

pub fn database_dir() -> PathBuf {
    path::data_dir().join("db")
}

pub struct AppDatabase(pub ThreadSafeConnection);

impl AppDatabase {
    pub fn new() -> Self {
        let db_dir = database_dir();
        let connection = smol::block_on(open_db::<AppMigrator>(&db_dir));
        Self(connection)
    }

    #[cfg(any(test, feature = "test"))]
    pub fn test_new() -> Self {
        let name = format!("test-db-{}", uuid::Uuid::new_v4());
        let connection = smol::block_on(open_test_db::<AppMigrator>(&name));
        Self(connection)
    }

    pub fn global(cx: &App) -> &ThreadSafeConnection {
        #[cfg(any(test, feature = "test"))]
        {
            if let Some(db) = cx.try_global::<Self>() {
                &db.0
            } else {
                &TEST_APP_DATABASE.0
            }
        }

        #[cfg(not(any(test, feature = "test")))]
        {
            &cx.global::<Self>().0
        }
    }
}

impl Global for AppDatabase {}

impl Default for AppDatabase {
    fn default() -> Self {
        Self::new()
    }
}

#[macro_export]
macro_rules! static_connection {
    ($t:ident, [ $($d:ty),* ]) => {
        impl ::std::ops::Deref for $t {
            type Target = $crate::sql::thread_safe_connection::ThreadSafeConnection;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl ::std::clone::Clone for $t {
            fn clone(&self) -> Self {
                $t(self.0.clone())
            }
        }

        impl $t {
            pub fn open(db: &$crate::AppDatabase) -> Self {
                $t(db.0.clone())
            }

            pub fn global(cx: &$crate::App) -> Self {
                $t($crate::AppDatabase::global(cx).clone())
            }

            #[cfg(any(test, feature = "test"))]
            pub async fn test_open(name: &'static str) -> Self {
                $t($crate::open_test_db::<$t>(name).await)
            }
        }

        $crate::inventory::submit! {
            $crate::DomainMigration {
                name: <$t as $crate::sql::domain::Domain>::NAME,
                migrations: <$t as $crate::sql::domain::Domain>::MIGRATIONS,
                dependencies: &[$(<$d as $crate::sql::domain::Domain>::NAME),*],
                should_allow_migration_change: <$t as $crate::sql::domain::Domain>::should_allow_migration_change,
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    use fs::TempFs;
    use gpui::TestAppContext;
    use sql::domain::Domain;

    #[gpui::test]
    async fn test_db_corruption(cx: &mut TestAppContext) {
        enum CorruptedDb {}

        impl Domain for CorruptedDb {
            const NAME: &str = "db_tests";
            const MIGRATIONS: &[&str] = &[sql!(CREATE TABLE test(value TEXT) STRICT)];
        }

        enum GoodDb {}

        impl Domain for GoodDb {
            const NAME: &str = "db_tests";
            const MIGRATIONS: &[&str] = &[sql!(CREATE TABLE test2(value TEXT) STRICT)];
        }

        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let db_dir = temp_fs.path().join("db");
        let db_path = db_dir.join(DB_NAME);

        assert!(!db_dir.exists());

        {
            let corrupted_db = open_db::<CorruptedDb>(&db_dir).await;
            assert!(matches!(corrupted_db.target(), ConnectionTarget::File(_)));
            assert!(db_path.exists());
        }

        let good_db = open_db::<GoodDb>(&db_dir).await;
        assert!(matches!(good_db.target(), ConnectionTarget::Memory(_)));
        assert!(db_path.exists());

        let value = good_db
            .read(|connection| {
                connection
                    .select_row::<String>(sql!(SELECT value FROM test2))
                    .and_then(|mut stmt| stmt())
            })
            .unwrap();

        assert_eq!(value, None);
    }

    #[gpui::test]
    async fn test_db_open_failure_falls_back_to_memory(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let db_dir = temp_fs.path().join("db");
        let db_path = db_dir.join(DB_NAME);

        std::fs::create_dir_all(&db_path).unwrap();

        let recovered_connection = open_db::<AppMigrator>(&db_dir).await;
        assert!(matches!(
            recovered_connection.target(),
            ConnectionTarget::Memory(_)
        ));
        assert!(db_path.is_dir());

        recovered_connection
            .write(|connection| {
                connection
                    .exec(sql!(CREATE TABLE test(value TEXT) STRICT))
                    .and_then(|mut stmt| stmt())?;
                connection
                    .exec_bound::<&str>(sql!(INSERT INTO test(value) VALUES (?1)))
                    .and_then(|mut stmt| stmt("ok"))?;
                Ok(())
            })
            .await
            .unwrap();

        let value = recovered_connection
            .read(|connection| {
                connection
                    .select_row::<String>(sql!(SELECT value FROM test))
                    .and_then(|mut stmt| stmt())
                    .context("test value query returned no row")
            })
            .unwrap();

        assert_eq!(value, Some("ok".to_string()));
    }
}
