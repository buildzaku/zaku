use anyhow::Context;

use sql_macros::sql;

use crate::{ThreadSafeConnection, query, sql::domain::Domain};

pub struct KeyValueStore(ThreadSafeConnection);

impl KeyValueStore {
    query! {
        pub fn read_kv(key: &str) -> anyhow::Result<Option<String>> {
            SELECT value FROM kv_store WHERE key = (?)
        }
    }

    query! {
        pub async fn write_kv(key: String, value: String) -> anyhow::Result<()> {
            INSERT OR REPLACE INTO kv_store(key, value) VALUES ((?), (?))
        }
    }

    query! {
        pub async fn delete_kv(key: String) -> anyhow::Result<()> {
            DELETE FROM kv_store WHERE key = (?)
        }
    }

    pub fn scoped<'a>(&'a self, namespace: &'a str) -> ScopedKeyValueStore<'a> {
        ScopedKeyValueStore {
            store: self,
            namespace,
        }
    }
}

impl Domain for KeyValueStore {
    const NAME: &str = stringify!(KeyValueStore);
    const MIGRATIONS: &[&str] = &[
        sql!(
            CREATE TABLE IF NOT EXISTS kv_store(
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            ) STRICT;
        ),
        sql!(
            CREATE TABLE IF NOT EXISTS scoped_kv_store(
                namespace TEXT NOT NULL,
                key TEXT NOT NULL,
                value TEXT NOT NULL,
                PRIMARY KEY(namespace, key)
            ) STRICT;
        ),
    ];
}

crate::static_connection!(KeyValueStore, []);

pub struct ScopedKeyValueStore<'a> {
    store: &'a KeyValueStore,
    namespace: &'a str,
}

impl ScopedKeyValueStore<'_> {
    pub fn read(&self, key: &str) -> anyhow::Result<Option<String>> {
        self.store.read(|connection| {
            connection
                .select_row_bound::<(&str, &str), String>(
                    sql!(SELECT value FROM scoped_kv_store WHERE namespace = (?) AND key = (?)),
                )
                .context("Failed to read from scoped_kv_store")
                .and_then(|mut f| f((self.namespace, key)))
                .context("Failed to read from scoped_kv_store")
        })
    }

    pub async fn write(&self, key: String, value: String) -> anyhow::Result<()> {
        let namespace = self.namespace.to_owned();
        self.store
            .write(move |connection| {
                connection.exec_bound::<(&str, &str, &str)>(sql!(
                    INSERT OR REPLACE INTO scoped_kv_store(namespace, key, value)
                    VALUES ((?), (?), (?))
                ))?((&namespace, &key, &value))
                .context("Failed to write to scoped_kv_store")
            })
            .await
    }

    pub async fn delete(&self, key: String) -> anyhow::Result<()> {
        let namespace = self.namespace.to_owned();
        self.store
            .write(move |connection| {
                connection.exec_bound::<(&str, &str)>(
                    sql!(DELETE FROM scoped_kv_store WHERE namespace = (?) AND key = (?)),
                )?((&namespace, &key))
                .context("Failed to delete from scoped_kv_store")
            })
            .await
    }

    pub async fn delete_all(&self) -> anyhow::Result<()> {
        let namespace = self.namespace.to_owned();
        self.store
            .write(move |connection| {
                connection.exec_bound::<&str>(sql!(
                    DELETE FROM scoped_kv_store WHERE namespace = (?)
                ))?(&namespace)
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
        let kv_store = KeyValueStore::test_open("test_kv").await;

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
        let kv_store = KeyValueStore::test_open("test_scoped_kv").await;

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
