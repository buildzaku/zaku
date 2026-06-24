pub mod kv;
pub mod query;

pub use anyhow;
pub use gpui::{self, App};
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
    path::{Path, PathBuf},
    sync::{
        LazyLock,
        atomic::{AtomicBool, Ordering},
    },
};

#[cfg(any(test, feature = "test"))]
use sql::thread_safe_connection;
use sql::thread_safe_connection::ConnectionTarget;
use sql_macros::sql;

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

pub async fn open_db(db_dir: &Path) -> ThreadSafeConnection {
    if let Some(connection) = try_open_db(db_dir).await {
        return connection;
    }

    FILE_DB_FAILED.store(true, Ordering::Release);
    open_fallback_db().await
}

async fn try_open_db(db_dir: &Path) -> Option<ThreadSafeConnection> {
    match ensure_directory(db_dir)
        .await
        .and_then(|()| database_path(db_dir))
    {
        Ok(db_path) => open_main_db(&db_path).await,
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

async fn open_main_db(path: &Path) -> Option<ThreadSafeConnection> {
    log::trace!("Opening database {}", path.display());
    ThreadSafeConnection::builder(ConnectionTarget::file(path))
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

async fn open_fallback_db() -> ThreadSafeConnection {
    log::warn!("Opening fallback in-memory database");
    ThreadSafeConnection::builder(ConnectionTarget::memory(FALLBACK_MEMORY_DB_NAME))
        .with_db_init_query(DB_INIT_QUERY)
        .with_connection_init_query(CONNECTION_INIT_QUERY)
        .build()
        .await
        .expect("fallback in-memory database should open")
}

#[cfg(any(test, feature = "test"))]
pub async fn open_test_db(db_name: &str) -> ThreadSafeConnection {
    ThreadSafeConnection::builder(ConnectionTarget::memory(db_name))
        .with_db_init_query(DB_INIT_QUERY)
        .with_connection_init_query(CONNECTION_INIT_QUERY)
        .with_write_queue_constructor(thread_safe_connection::locking_queue())
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

impl Global for AppDatabase {}

impl Default for AppDatabase {
    fn default() -> Self {
        Self::new()
    }
}

impl AppDatabase {
    pub fn new() -> Self {
        let db_dir = database_dir();
        let connection = smol::block_on(open_db(&db_dir));
        let app_db = Self(connection);
        smol::block_on(kv::KeyValueStore::open(&app_db).initialize_schema())
            .expect("key-value store schema should initialize");
        app_db
    }

    #[cfg(any(test, feature = "test"))]
    pub fn test_new() -> Self {
        let name = format!("test-db-{}", uuid::Uuid::new_v4());
        let connection = smol::block_on(open_test_db(&name));
        let app_db = Self(connection);
        smol::block_on(kv::KeyValueStore::open(&app_db).initialize_schema())
            .expect("key-value store schema should initialize");
        app_db
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
                let connection = $t($crate::open_test_db(name).await);
                connection
                    .initialize_schema()
                    .await
                    .expect("database schema should initialize");
                connection
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    use fs::TempFs;
    use gpui::TestAppContext;

    #[gpui::test]
    async fn test_db_corruption(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let db_dir = temp_fs.path().join("db");
        let db_path = db_dir.join(DB_NAME);

        assert!(!db_dir.exists());

        {
            let connection = open_db(&db_dir).await;
            assert!(matches!(connection.target(), ConnectionTarget::File(_)));
            assert!(db_path.exists());
        }
        std::fs::write(&db_path, b"not a sqlite database").unwrap();

        let recreated_connection = open_db(&db_dir).await;
        assert!(matches!(
            recreated_connection.target(),
            ConnectionTarget::Memory(_)
        ));
        assert!(db_path.exists());

        recreated_connection
            .write(|connection| {
                connection
                    .exec(sql!(CREATE TABLE test(value TEXT) STRICT))
                    .and_then(|mut f| f())?;
                connection
                    .exec_bound::<&str>(sql!(INSERT INTO test(value) VALUES (?1)))
                    .and_then(|mut f| f("ok"))?;
                Ok(())
            })
            .await
            .unwrap();

        let value = recreated_connection
            .read(|connection| {
                connection
                    .select_row::<String>(sql!(SELECT value FROM test))
                    .and_then(|mut f| f())
                    .context("test value query returned no row")
            })
            .unwrap();

        assert_eq!(value, Some("ok".to_string()));
    }

    #[gpui::test]
    async fn test_db_open_failure_falls_back_to_memory(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let db_dir = temp_fs.path().join("db");
        let db_path = db_dir.join(DB_NAME);

        std::fs::create_dir_all(&db_path).unwrap();

        let recovered_connection = open_db(&db_dir).await;
        assert!(matches!(
            recovered_connection.target(),
            ConnectionTarget::Memory(_)
        ));
        assert!(db_path.is_dir());

        recovered_connection
            .write(|connection| {
                connection
                    .exec(sql!(CREATE TABLE test(value TEXT) STRICT))
                    .and_then(|mut f| f())?;
                connection
                    .exec_bound::<&str>(sql!(INSERT INTO test(value) VALUES (?1)))
                    .and_then(|mut f| f("ok"))?;
                Ok(())
            })
            .await
            .unwrap();

        let value = recovered_connection
            .read(|connection| {
                connection
                    .select_row::<String>(sql!(SELECT value FROM test))
                    .and_then(|mut f| f())
                    .context("test value query returned no row")
            })
            .unwrap();

        assert_eq!(value, Some("ok".to_string()));
    }
}
