use anyhow::Context;
use gpui::App;
use indoc::indoc;
use std::ops::Deref;

use crate::{AppDatabase, ThreadSafeConnection};

#[derive(Clone)]
pub struct KeyValueStore(ThreadSafeConnection);

impl KeyValueStore {
    pub fn from_app_db(db: &AppDatabase) -> Self {
        Self(db.0.clone())
    }

    #[cfg(any(test, feature = "test-support"))]
    pub async fn from_test_db(name: &'static str) -> Self {
        let kv_store = Self(crate::open_test_db(name).await);
        kv_store
            .initialize_schema()
            .await
            .expect("key-value store schema should initialize");
        kv_store
    }

    pub fn global(cx: &App) -> Self {
        Self(AppDatabase::global(cx).clone())
    }

    pub fn read_kv(&self, key: &str) -> anyhow::Result<Option<String>> {
        self.read(|connection| {
            connection
                .select_row_bound::<[&str; 1], String>(
                    indoc! {"SELECT value FROM kv_store WHERE key = ?1"},
                )
                .context("failed to prepare key-value lookup query")
                .and_then(|mut statement| statement([key]))
                .context("failed to read key-value pair")
        })
    }

    pub async fn write_kv(&self, key: String, value: String) -> anyhow::Result<()> {
        log::debug!("Writing key-value pair for key {key}");

        self.write(move |connection| {
            connection
                .exec_bound(indoc! {"INSERT OR REPLACE INTO kv_store(key, value) VALUES (?1, ?2)"})
                .context("Failed to write to kv_store")
                .and_then(|mut statement| statement((key, value)))
        })
        .await
    }

    pub async fn delete_kv(&self, key: String) -> anyhow::Result<()> {
        self.write(move |connection| {
            connection
                .exec_bound("DELETE FROM kv_store WHERE key = ?1")
                .context("Failed to delete from kv_store")
                .and_then(|mut statement| statement([key]))
        })
        .await
    }

    pub fn scoped<'a>(&'a self, namespace: &'a str) -> ScopedKeyValueStore<'a> {
        ScopedKeyValueStore {
            store: self,
            namespace,
        }
    }

    pub(crate) async fn initialize_schema(&self) -> anyhow::Result<()> {
        self.write(|connection| {
            connection.with_savepoint("initialize_key_value_store_schema", || {
                connection
                    .exec(indoc! {"
                        CREATE TABLE IF NOT EXISTS kv_store(
                            key TEXT PRIMARY KEY,
                            value TEXT NOT NULL
                        ) STRICT
                    "})
                    .context("failed to prepare key-value store table initialization")
                    .and_then(|mut statement| statement())
                    .context("failed to initialize key-value store table")?;

                connection
                    .exec(indoc! {"
                        CREATE TABLE IF NOT EXISTS scoped_kv_store(
                            namespace TEXT NOT NULL,
                            key TEXT NOT NULL,
                            value TEXT NOT NULL,
                            PRIMARY KEY(namespace, key)
                        ) STRICT
                    "})
                    .context("failed to prepare scoped key-value store table initialization")
                    .and_then(|mut statement| statement())
                    .context("failed to initialize scoped key-value store table")?;

                Ok(())
            })
        })
        .await
    }
}

impl Deref for KeyValueStore {
    type Target = ThreadSafeConnection;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct ScopedKeyValueStore<'a> {
    store: &'a KeyValueStore,
    namespace: &'a str,
}

impl ScopedKeyValueStore<'_> {
    pub fn read(&self, key: &str) -> anyhow::Result<Option<String>> {
        self.store.read(|connection| {
            connection
                .select_row_bound::<(&str, &str), String>(
                    "SELECT value FROM scoped_kv_store WHERE namespace = (?) AND key = (?)",
                )
                .context("Failed to read from scoped_kv_store")
                .and_then(|mut statement| statement((self.namespace, key)))
                .context("Failed to read from scoped_kv_store")
        })
    }

    pub async fn write(&self, key: String, value: String) -> anyhow::Result<()> {
        let namespace = self.namespace.to_owned();
        self.store
            .write(move |connection| {
                connection
                    .exec_bound::<(&str, &str, &str)>(
                        "INSERT OR REPLACE INTO scoped_kv_store(namespace, key, value) VALUES ((?), (?), (?))",
                    )?((&namespace, &key, &value))
                    .context("Failed to write to scoped_kv_store")
            })
            .await
    }

    pub async fn delete(&self, key: String) -> anyhow::Result<()> {
        let namespace = self.namespace.to_owned();
        self.store
            .write(move |connection| {
                connection.exec_bound::<(&str, &str)>(
                    "DELETE FROM scoped_kv_store WHERE namespace = (?) AND key = (?)",
                )?((&namespace, &key))
                .context("Failed to delete from scoped_kv_store")
            })
            .await
    }

    pub async fn delete_all(&self) -> anyhow::Result<()> {
        let namespace = self.namespace.to_owned();
        self.store
            .write(move |connection| {
                connection
                    .exec_bound::<&str>("DELETE FROM scoped_kv_store WHERE namespace = (?)")?(
                    &namespace,
                )
                .context("Failed to delete_all from scoped_kv_store")
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use crate::kv::KeyValueStore;

    #[gpui::test]
    async fn test_kv() {
        let kv_store = KeyValueStore::from_test_db("test_kv").await;

        assert_eq!(kv_store.read_kv("key-1").unwrap(), None);

        kv_store
            .write_kv("key-1".to_string(), "one".to_string())
            .await
            .unwrap();
        assert_eq!(kv_store.read_kv("key-1").unwrap(), Some("one".to_string()));

        kv_store
            .write_kv("key-1".to_string(), "one-2".to_string())
            .await
            .unwrap();
        assert_eq!(
            kv_store.read_kv("key-1").unwrap(),
            Some("one-2".to_string())
        );

        kv_store
            .write_kv("key-2".to_string(), "two".to_string())
            .await
            .unwrap();
        assert_eq!(kv_store.read_kv("key-2").unwrap(), Some("two".to_string()));

        kv_store.delete_kv("key-1".to_string()).await.unwrap();
        assert_eq!(kv_store.read_kv("key-1").unwrap(), None);
    }

    #[gpui::test]
    async fn test_scoped_kv() {
        let kv_store = KeyValueStore::from_test_db("test_scoped_kv").await;

        let scope_a = kv_store.scoped("namespace-a");
        let scope_b = kv_store.scoped("namespace-b");

        assert_eq!(scope_a.read("key-1").unwrap(), None);

        scope_a
            .write("key-1".to_string(), "value-a1".to_string())
            .await
            .unwrap();
        assert_eq!(scope_a.read("key-1").unwrap(), Some("value-a1".to_string()));

        scope_b
            .write("key-1".to_string(), "value-b1".to_string())
            .await
            .unwrap();
        assert_eq!(scope_a.read("key-1").unwrap(), Some("value-a1".to_string()));
        assert_eq!(scope_b.read("key-1").unwrap(), Some("value-b1".to_string()));

        scope_a
            .write("key-2".to_string(), "value-a2".to_string())
            .await
            .unwrap();
        scope_a.delete("key-1".to_string()).await.unwrap();
        assert_eq!(scope_a.read("key-1").unwrap(), None);
        assert_eq!(scope_a.read("key-2").unwrap(), Some("value-a2".to_string()));
        assert_eq!(scope_b.read("key-1").unwrap(), Some("value-b1".to_string()));

        scope_a
            .write("key-3".to_string(), "value-a3".to_string())
            .await
            .unwrap();
        scope_a.delete_all().await.unwrap();
        assert_eq!(scope_a.read("key-2").unwrap(), None);
        assert_eq!(scope_a.read("key-3").unwrap(), None);
        assert_eq!(scope_b.read("key-1").unwrap(), Some("value-b1".to_string()));
    }
}
