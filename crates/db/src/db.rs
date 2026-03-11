mod bindable;
mod connection;
mod savepoint;
mod statement;
mod typed_statements;

use anyhow::Context;
use futures::{Future, channel::oneshot};
use indoc::indoc;
use parking_lot::{Mutex, RwLock};
use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, LazyLock,
        atomic::{AtomicBool, Ordering},
    },
};
use thread_local::ThreadLocal;

pub use bindable::{Bind, Column, StaticColumnCount};
pub use connection::Connection;
pub use statement::{Row, SqlType, Statement};

const CONNECTION_INIT_QUERY: &str = indoc! {"
    PRAGMA foreign_keys = ON;
"};

const DB_INIT_QUERY: &str = indoc! {"
    PRAGMA journal_mode = WAL;
    PRAGMA busy_timeout = 500;
    PRAGMA case_sensitive_like = TRUE;
    PRAGMA synchronous = NORMAL;
"};

const FALLBACK_MEMORY_DB_NAME: &str = "FALLBACK_MEMORY_DB";
const DB_NAME: &str = "db.sqlite";

type QueuedWrite = Box<dyn 'static + Send + FnOnce()>;
type WriteQueue = Box<dyn 'static + Send + Sync + Fn(QueuedWrite) -> anyhow::Result<()>>;
type WriteQueueConstructor = Box<dyn 'static + Send + FnMut() -> WriteQueue>;

#[derive(Clone, Eq, Hash, PartialEq)]
enum ConnectionTarget {
    Memory(Arc<str>),
    File(Arc<PathBuf>),
}

impl ConnectionTarget {
    fn memory(name: &str) -> Self {
        Self::Memory(Arc::from(name))
    }

    fn file(path: &Path) -> Self {
        Self::File(Arc::new(path.to_path_buf()))
    }
}

static QUEUES: LazyLock<RwLock<HashMap<ConnectionTarget, WriteQueue>>> =
    LazyLock::new(Default::default);
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
        .and_then(|_| database_path(db_dir))
    {
        Ok(db_path) => open_main_db(&db_path).await,
        Err(error) => {
            eprintln!(
                "failed to prepare sqlite database directory {}: {error}",
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
    ThreadSafeConnectionBuilder {
        db_init_query: None,
        write_queue_constructor: None,
        connection: ThreadSafeConnection {
            target: ConnectionTarget::file(path),
            connection_init_query: None,
            connections: Default::default(),
        },
    }
    .with_db_init_query(DB_INIT_QUERY)
    .with_connection_init_query(CONNECTION_INIT_QUERY)
    .build()
    .await
    .map_err(|error| {
        eprintln!(
            "failed to open sqlite database at {}: {error}",
            path.display()
        );
        error
    })
    .ok()
}

async fn open_fallback_db() -> ThreadSafeConnection {
    eprintln!("opening fallback in-memory database");
    ThreadSafeConnectionBuilder {
        db_init_query: None,
        write_queue_constructor: None,
        connection: ThreadSafeConnection {
            target: ConnectionTarget::memory(FALLBACK_MEMORY_DB_NAME),
            connection_init_query: None,
            connections: Default::default(),
        },
    }
    .with_db_init_query(DB_INIT_QUERY)
    .with_connection_init_query(CONNECTION_INIT_QUERY)
    .build()
    .await
    .expect("fallback in-memory database should open")
}

#[cfg(any(test, feature = "test-support"))]
pub async fn open_test_db(db_name: &str) -> ThreadSafeConnection {
    ThreadSafeConnectionBuilder {
        db_init_query: None,
        write_queue_constructor: None,
        connection: ThreadSafeConnection {
            target: ConnectionTarget::memory(db_name),
            connection_init_query: None,
            connections: Default::default(),
        },
    }
    .with_db_init_query(DB_INIT_QUERY)
    .with_connection_init_query(CONNECTION_INIT_QUERY)
    .with_write_queue_constructor(locking_queue())
    .build()
    .await
    .expect("test in-memory database should open")
}

#[derive(Clone)]
pub struct ThreadSafeConnection {
    target: ConnectionTarget,
    connection_init_query: Option<&'static str>,
    connections: Arc<ThreadLocal<RefCell<Option<Connection>>>>,
}

struct ThreadSafeConnectionBuilder {
    db_init_query: Option<&'static str>,
    write_queue_constructor: Option<WriteQueueConstructor>,
    connection: ThreadSafeConnection,
}

impl ThreadSafeConnectionBuilder {
    fn with_connection_init_query(mut self, connection_init_query: &'static str) -> Self {
        self.connection.connection_init_query = Some(connection_init_query);
        self
    }

    fn with_db_init_query(mut self, init_query: &'static str) -> Self {
        self.db_init_query = Some(init_query);
        self
    }

    fn with_write_queue_constructor(
        mut self,
        write_queue_constructor: WriteQueueConstructor,
    ) -> Self {
        self.write_queue_constructor = Some(write_queue_constructor);
        self
    }

    async fn build(self) -> anyhow::Result<ThreadSafeConnection> {
        self.connection
            .initialize_queues(self.write_queue_constructor);

        let db_init_query = self.db_init_query;
        self.connection
            .write(move |connection| {
                if let Some(db_init_query) = db_init_query {
                    connection
                        .exec(db_init_query)
                        .with_context(|| {
                            format!(
                                "database initialize query failed to execute: {}",
                                db_init_query
                            )
                        })
                        .and_then(|mut f| f())?;
                }

                Ok(())
            })
            .await?;

        Ok(self.connection)
    }
}

impl ThreadSafeConnection {
    fn initialize_queues(&self, write_queue_constructor: Option<WriteQueueConstructor>) -> bool {
        if !QUEUES.read().contains_key(&self.target) {
            let mut queues = QUEUES.write();
            if !queues.contains_key(&self.target) {
                let mut write_queue_constructor =
                    write_queue_constructor.unwrap_or_else(background_thread_queue);
                queues.insert(self.target.clone(), write_queue_constructor());
                return true;
            }
        }
        false
    }

    pub fn read<T>(
        &self,
        read_operation: impl FnOnce(&Connection) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        self.with_connection(read_operation)
    }

    pub fn write<T: 'static + Send>(
        &self,
        write_operation: impl 'static + Send + FnOnce(&Connection) -> anyhow::Result<T>,
    ) -> impl Future<Output = anyhow::Result<T>> {
        let thread_safe_connection = self.clone();
        let target = self.target.clone();

        async move {
            let receiver = {
                let queues = QUEUES.read();
                let write_channel = queues
                    .get(&target)
                    .context("write queue should exist after thread-safe connection build")?;

                let (sender, receiver) = oneshot::channel();
                write_channel(Box::new(move || {
                    let result = thread_safe_connection.with_connection(|connection| {
                        connection.with_write(|connection| write_operation(connection))
                    });
                    sender.send(result).ok();
                }))?;

                receiver
            };

            receiver
                .await
                .context("write queue unexpectedly closed before sending a result")?
        }
    }

    fn with_connection<T>(
        &self,
        connection_operation: impl FnOnce(&Connection) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let connection_slot = self.connections.get_or(|| RefCell::new(None));

        if connection_slot.borrow().is_none() {
            let connection = self.create_connection()?;
            *connection_slot.borrow_mut() = Some(connection);
        }

        let connection_slot = connection_slot.borrow();
        let connection = connection_slot
            .as_ref()
            .context("thread-local sqlite connection should be initialized")?;
        connection_operation(connection)
    }

    fn create_connection(&self) -> anyhow::Result<Connection> {
        let mut connection = match &self.target {
            ConnectionTarget::File(path) => {
                Connection::open_file(path.as_ref()).with_context(|| {
                    format!("failed to reopen sqlite database at {}", path.display())
                })?
            }
            ConnectionTarget::Memory(name) => Connection::open_memory(Some(name.as_ref())),
        };

        *connection.write.get_mut() = false;
        init_connection(&connection, self.connection_init_query)?;
        Ok(connection)
    }
}

fn init_connection(
    connection: &Connection,
    connection_init_query: Option<&'static str>,
) -> anyhow::Result<()> {
    if let Some(connection_init_query) = connection_init_query {
        connection
            .exec(connection_init_query)
            .with_context(|| {
                format!(
                    "connection initialize query failed to execute: {}",
                    connection_init_query
                )
            })
            .and_then(|mut f| f())?;
    }

    Ok(())
}

pub fn background_thread_queue() -> WriteQueueConstructor {
    Box::new(|| {
        let (sender, receiver) = std::sync::mpsc::channel::<QueuedWrite>();

        std::thread::Builder::new()
            .name("db_worker".to_string())
            .spawn(move || {
                while let Ok(write) = receiver.recv() {
                    write();
                }
            })
            .expect("database worker thread should spawn");

        Box::new(move |queued_write| {
            sender
                .send(queued_write)
                .map_err(|_| anyhow::anyhow!("could not send write action to background thread"))
        })
    })
}

pub fn locking_queue() -> WriteQueueConstructor {
    Box::new(|| {
        let write_mutex = Mutex::new(());
        Box::new(move |queued_write| {
            let _lock = write_mutex.lock();
            queued_write();
            Ok(())
        })
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    use fs::TempFs;
    use gpui::TestAppContext;

    #[gpui::test]
    async fn test_db_corruption(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new();
        let db_dir = temp_fs.path().join("db");
        let db_path = db_dir.join(DB_NAME);

        assert!(!db_dir.exists());

        {
            let connection = open_db(&db_dir).await;
            assert!(matches!(connection.target, ConnectionTarget::File(_)));
            assert!(db_path.exists());
        }
        std::fs::write(&db_path, b"not a sqlite database").unwrap();

        let recreated_connection = open_db(&db_dir).await;
        assert!(matches!(
            recreated_connection.target,
            ConnectionTarget::Memory(_)
        ));
        assert!(db_path.exists());

        recreated_connection
            .write(|connection| {
                connection
                    .exec("CREATE TABLE test(value TEXT) STRICT")
                    .and_then(|mut f| f())?;
                connection
                    .exec("INSERT INTO test(value) VALUES ('ok')")
                    .and_then(|mut f| f())?;
                Ok(())
            })
            .await
            .unwrap();

        let value = recreated_connection
            .read(|connection| {
                connection
                    .select_row::<String>("SELECT value FROM test")
                    .and_then(|mut f| f())
                    .context("test value query returned no row")
            })
            .unwrap();

        assert_eq!(value, Some("ok".to_string()));
    }

    #[gpui::test]
    async fn test_db_open_failure_falls_back_to_memory(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new();
        let db_dir = temp_fs.path().join("db");
        let db_path = db_dir.join(DB_NAME);

        std::fs::create_dir_all(&db_path).unwrap();

        let recovered_connection = open_db(&db_dir).await;
        assert!(matches!(
            recovered_connection.target,
            ConnectionTarget::Memory(_)
        ));
        assert!(db_path.is_dir());

        recovered_connection
            .write(|connection| {
                connection
                    .exec("CREATE TABLE test(value TEXT) STRICT")
                    .and_then(|mut f| f())?;
                connection
                    .exec("INSERT INTO test(value) VALUES ('ok')")
                    .and_then(|mut f| f())?;
                Ok(())
            })
            .await
            .unwrap();

        let value = recovered_connection
            .read(|connection| {
                connection
                    .select_row::<String>("SELECT value FROM test")
                    .and_then(|mut f| f())
                    .context("test value query returned no row")
            })
            .unwrap();

        assert_eq!(value, Some("ok".to_string()));
    }

    #[test]
    fn test_many_initialize_queries_at_once() {
        let mut handles = vec![];

        for _ in 0..100 {
            handles.push(std::thread::spawn(|| {
                let builder = ThreadSafeConnectionBuilder {
                    db_init_query: None,
                    write_queue_constructor: None,
                    connection: ThreadSafeConnection {
                        target: ConnectionTarget::memory("test.sqlite"),
                        connection_init_query: None,
                        connections: Default::default(),
                    },
                }
                .with_db_init_query(DB_INIT_QUERY)
                .with_connection_init_query(CONNECTION_INIT_QUERY);

                let _ = smol::block_on(builder.build()).unwrap();
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[gpui::test]
    async fn test_read_connections_reject_writes(_cx: &mut TestAppContext) {
        let connection = open_test_db("test_read_connections_reject_writes").await;

        connection
            .write(|connection| {
                connection
                    .exec("CREATE TABLE test(value TEXT) STRICT;")
                    .and_then(|mut f| f())?;
                Ok(())
            })
            .await
            .unwrap();

        let write_attempt = connection.read(|connection| {
            connection
                .select_row::<i64>("INSERT INTO test(value) VALUES ('nope') RETURNING rowid")
                .and_then(|mut f| f())?;
            Ok(())
        });
        assert!(write_attempt.is_err());

        let count = connection
            .read(|connection| {
                connection
                    .select_row::<i64>("SELECT COUNT(*) FROM test")
                    .and_then(|mut f| f())
                    .context("test count query returned no row")
            })
            .unwrap();
        assert_eq!(count, Some(0));
    }

    #[gpui::test]
    async fn test_select_row_requires_zero_or_one_row(_cx: &mut TestAppContext) {
        let connection = open_test_db("test_select_row_requires_zero_or_one_row").await;

        connection
            .write(|connection| {
                connection
                    .exec("CREATE TABLE test(value INTEGER) STRICT")
                    .and_then(|mut f| f())?;
                connection
                    .exec("INSERT INTO test(value) VALUES (1)")
                    .and_then(|mut f| f())?;
                connection
                    .exec("INSERT INTO test(value) VALUES (2)")
                    .and_then(|mut f| f())?;
                Ok(())
            })
            .await
            .unwrap();

        let missing_value = connection
            .read(|connection| {
                connection
                    .select_row::<i64>("SELECT value FROM test WHERE value = 3")
                    .and_then(|mut f| f())
            })
            .unwrap();
        assert!(missing_value.is_none());

        let multiple_rows = connection.read(|connection| {
            connection
                .select_row::<i64>("SELECT value FROM test ORDER BY value")
                .and_then(|mut f| f())
        });
        assert!(multiple_rows.is_err());
    }

    #[gpui::test]
    async fn test_select_supports_ten_columns(_cx: &mut TestAppContext) {
        let connection = open_test_db("test_select_supports_ten_columns").await;

        connection
            .write(|connection| {
                connection
                    .exec(indoc! {"
                        CREATE TABLE test(
                            a INTEGER,
                            b INTEGER,
                            c INTEGER,
                            d INTEGER,
                            e INTEGER,
                            f INTEGER,
                            g INTEGER,
                            h INTEGER,
                            i INTEGER,
                            j INTEGER
                        ) STRICT
                    "})
                    .and_then(|mut f| f())?;
                connection
                    .exec("INSERT INTO test(a, b, c, d, e, f, g, h, i, j) VALUES (1, 2, 3, 4, 5, 6, 7, 8, 9, 10)")
                    .and_then(|mut f| f())?;
                Ok(())
            })
            .await
            .unwrap();

        let value = connection
            .read(|connection| {
                connection
                    .select_row::<(i64, i64, i64, i64, i64, i64, i64, i64, i64, i64)>(
                        "SELECT a, b, c, d, e, f, g, h, i, j FROM test",
                    )
                    .and_then(|mut f| f())
                    .context("ten-column query returned no row")
            })
            .unwrap();

        assert_eq!(value, Some((1, 2, 3, 4, 5, 6, 7, 8, 9, 10)));
    }

    #[gpui::test]
    async fn test_db_init_query_applies_to_worker_connection(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let connection = ThreadSafeConnectionBuilder {
            db_init_query: None,
            write_queue_constructor: None,
            connection: ThreadSafeConnection {
                target: ConnectionTarget::memory("test_db_init_query_applies_to_worker_connection"),
                connection_init_query: None,
                connections: Default::default(),
            },
        }
        .with_db_init_query(DB_INIT_QUERY)
        .with_connection_init_query(CONNECTION_INIT_QUERY)
        .build()
        .await
        .unwrap();

        let busy_timeout = connection
            .write(|connection| {
                connection
                    .select_row::<i64>("PRAGMA busy_timeout")
                    .and_then(|mut f| f())
            })
            .await
            .unwrap();

        assert_eq!(busy_timeout, Some(500));
    }

    #[gpui::test]
    async fn test_persistent_connection_retries_after_open_failure(_cx: &mut TestAppContext) {
        let temp_fs = TempFs::new();
        let db_path = temp_fs.path().join("db.sqlite");
        std::fs::create_dir_all(&db_path).unwrap();

        let connection = ThreadSafeConnection {
            target: ConnectionTarget::file(&db_path),
            connection_init_query: None,
            connections: Default::default(),
        };
        connection.initialize_queues(Some(locking_queue()));

        assert!(connection.read(|_| Ok(())).is_err());

        std::fs::remove_dir(&db_path).unwrap();

        connection
            .write(|connection| {
                connection
                    .exec("CREATE TABLE test(value TEXT) STRICT")
                    .and_then(|mut f| f())?;
                connection
                    .exec("INSERT INTO test(value) VALUES ('ok')")
                    .and_then(|mut f| f())?;
                Ok(())
            })
            .await
            .unwrap();

        let value = connection
            .read(|connection| {
                connection
                    .select_row::<String>("SELECT value FROM test")
                    .and_then(|mut f| f())
                    .context("test value query returned no row")
            })
            .unwrap();

        assert_eq!(value, Some("ok".to_string()));
    }
}
