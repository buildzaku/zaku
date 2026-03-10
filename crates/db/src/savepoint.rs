use indoc::formatdoc;

use crate::connection::Connection;

struct SavepointGuard<'conn> {
    connection: &'conn Connection,
    name: String,
    finished: bool,
}

impl<'conn> SavepointGuard<'conn> {
    fn new(connection: &'conn Connection, name: impl Into<String>) -> anyhow::Result<Self> {
        let name = name.into();
        connection
            .exec(&format!("SAVEPOINT {name}"))
            .and_then(|mut f| f())?;

        Ok(Self {
            connection,
            name,
            finished: false,
        })
    }

    fn release(&mut self) -> anyhow::Result<()> {
        if self.finished {
            return Ok(());
        }

        self.connection
            .exec(&format!("RELEASE {}", self.name))
            .and_then(|mut f| f())?;
        self.finished = true;
        Ok(())
    }

    fn rollback_and_release(&mut self) -> anyhow::Result<()> {
        if self.finished {
            return Ok(());
        }

        self.connection
            .exec(&formatdoc! {"
                ROLLBACK TO {name};
                RELEASE {name}
            ", name = self.name.as_str()})
            .and_then(|mut f| f())?;
        self.finished = true;
        Ok(())
    }

    fn finish(&mut self) -> anyhow::Result<()> {
        if self.finished {
            return Ok(());
        }

        self.rollback_and_release()
    }
}

impl Drop for SavepointGuard<'_> {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

impl Connection {
    pub fn with_savepoint<R, F>(&self, name: impl AsRef<str>, f: F) -> anyhow::Result<R>
    where
        F: FnOnce() -> anyhow::Result<R>,
    {
        let mut savepoint = SavepointGuard::new(self, name.as_ref())?;

        let result = f();
        match result {
            Ok(value) => {
                savepoint.release()?;
                Ok(value)
            }
            Err(error) => {
                savepoint.rollback_and_release()?;
                Err(error)
            }
        }
    }

    pub fn with_savepoint_rollback<R, F>(
        &self,
        name: impl AsRef<str>,
        f: F,
    ) -> anyhow::Result<Option<R>>
    where
        F: FnOnce() -> anyhow::Result<Option<R>>,
    {
        let mut savepoint = SavepointGuard::new(self, name.as_ref())?;

        let result = f();
        match result {
            Ok(Some(value)) => {
                savepoint.release()?;
                Ok(Some(value))
            }
            Ok(None) => {
                savepoint.rollback_and_release()?;
                Ok(None)
            }
            Err(error) => {
                savepoint.rollback_and_release()?;
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::panic::AssertUnwindSafe;

    use crate::connection::Connection;

    #[test]
    fn test_nested_savepoints() {
        let connection = Connection::open_memory(Some("nested_savepoints"));

        connection
            .exec("CREATE TABLE text (text TEXT, idx INTEGER) STRICT")
            .and_then(|mut f| f())
            .expect("text table should initialize");

        let first_savepoint_text = "test save1";
        let second_savepoint_text = "test save2";

        connection
            .with_savepoint("first", || {
                connection
                    .exec_bound("INSERT INTO text(text, idx) VALUES (?1, ?2)")
                    .and_then(|mut f| f((first_savepoint_text, 1)))
                    .expect("first savepoint should insert its row");

                assert!(
                    connection
                        .with_savepoint("second", || -> anyhow::Result<Option<()>> {
                            connection
                                .exec_bound("INSERT INTO text(text, idx) VALUES (?1, ?2)")
                                .and_then(|mut f| f((second_savepoint_text, 2)))?;

                            assert_eq!(
                                connection
                                    .select::<String>("SELECT text FROM text ORDER BY text.idx ASC")
                                    .and_then(|mut f| f())
                                    .expect("nested savepoint rows should be readable"),
                                vec![first_savepoint_text, second_savepoint_text],
                            );

                            anyhow::bail!("failed second savepoint")
                        })
                        .err()
                        .is_some()
                );

                assert_eq!(
                    connection
                        .select::<String>("SELECT text FROM text ORDER BY text.idx ASC")
                        .and_then(|mut f| f())
                        .expect("rows after failed nested savepoint should be readable"),
                    vec![first_savepoint_text],
                );

                connection
                    .with_savepoint_rollback::<(), _>("second", || {
                        connection
                            .exec_bound("INSERT INTO text(text, idx) VALUES (?1, ?2)")
                            .and_then(|mut f| f((second_savepoint_text, 2)))
                            .expect("rollback savepoint should insert its row");

                        assert_eq!(
                            connection
                                .select::<String>("SELECT text FROM text ORDER BY text.idx ASC")
                                .and_then(|mut f| f())
                                .expect("rows during rollback savepoint should be readable"),
                            vec![first_savepoint_text, second_savepoint_text],
                        );

                        Ok(None)
                    })
                    .expect("rollback savepoint should succeed");

                assert_eq!(
                    connection
                        .select::<String>("SELECT text FROM text ORDER BY text.idx ASC")
                        .and_then(|mut f| f())
                        .expect("rows after rollback savepoint should be readable"),
                    vec![first_savepoint_text],
                );

                connection
                    .with_savepoint_rollback("second", || {
                        connection
                            .exec_bound("INSERT INTO text(text, idx) VALUES (?1, ?2)")
                            .and_then(|mut f| f((second_savepoint_text, 2)))
                            .expect("commit savepoint should insert its row");

                        assert_eq!(
                            connection
                                .select::<String>("SELECT text FROM text ORDER BY text.idx ASC")
                                .and_then(|mut f| f())
                                .expect("rows during commit savepoint should be readable"),
                            vec![first_savepoint_text, second_savepoint_text],
                        );

                        Ok(Some(()))
                    })
                    .expect("commit savepoint should succeed");

                assert_eq!(
                    connection
                        .select::<String>("SELECT text FROM text ORDER BY text.idx ASC")
                        .and_then(|mut f| f())
                        .expect("rows after commit savepoint should be readable"),
                    vec![first_savepoint_text, second_savepoint_text],
                );

                Ok(())
            })
            .expect("outer savepoint should succeed");

        assert_eq!(
            connection
                .select::<String>("SELECT text FROM text ORDER BY text.idx ASC")
                .and_then(|mut f| f())
                .expect("final rows should be readable"),
            vec![first_savepoint_text, second_savepoint_text],
        );
    }

    #[test]
    fn test_savepoint_rolls_back_on_panic() {
        let connection = Connection::open_memory(Some("savepoint_rolls_back_on_panic"));

        connection
            .exec("CREATE TABLE test(value INTEGER) STRICT")
            .and_then(|mut f| f())
            .expect("test table should initialize");

        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let _ = connection.with_savepoint("panic_savepoint", || -> anyhow::Result<()> {
                connection
                    .exec("INSERT INTO test(value) VALUES (1)")
                    .and_then(|mut f| f())?;
                panic!("panic inside savepoint");
            });
        }));

        assert!(result.is_err());
        assert_eq!(
            connection
                .select::<i32>("SELECT value FROM test")
                .and_then(|mut f| f())
                .expect("rows after panic should be readable"),
            Vec::<i32>::new(),
        );
    }
}
