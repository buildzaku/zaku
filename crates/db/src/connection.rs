#[cfg(target_os = "windows")]
use anyhow::Context;

use libsqlite3_sys::{
    self as sqlite3, SQLITE_OK, SQLITE_OPEN_CREATE, SQLITE_OPEN_EXRESCODE, SQLITE_OPEN_NOMUTEX,
    SQLITE_OPEN_READWRITE, SQLITE_OPEN_URI, SQLITE_ROW,
};

#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;

use std::{
    cell::Cell,
    ffi::{CStr, CString},
    marker::PhantomData,
    path::Path,
};

pub struct Connection {
    pub(crate) sqlite3: *mut sqlite3::sqlite3,
    pub(crate) write: Cell<bool>,
    _marker: PhantomData<sqlite3::sqlite3>,
}

// Safety: Connection owns its sqlite3 handle, so moving it to another
// thread transfers that ownership instead of sharing the handle.
unsafe impl Send for Connection {}

impl Connection {
    fn open<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path_to_cstring(path.as_ref())?;
        let mut connection = Self {
            sqlite3: std::ptr::null_mut(),
            write: Cell::new(true),
            _marker: PhantomData,
        };

        // Safety: CString path lives for the duration of this call and
        // connection.sqlite3 is a valid out-pointer for SQLite to initialize.
        let result_code = unsafe {
            sqlite3::sqlite3_open_v2(
                path.as_ptr(),
                &mut connection.sqlite3,
                SQLITE_OPEN_CREATE
                    | SQLITE_OPEN_EXRESCODE
                    | SQLITE_OPEN_NOMUTEX
                    | SQLITE_OPEN_READWRITE
                    | SQLITE_OPEN_URI,
                std::ptr::null(),
            )
        };

        connection.ensure_ok(result_code)?;

        Ok(connection)
    }

    pub(crate) fn open_file(path: &Path) -> anyhow::Result<Self> {
        Self::open(path)
    }

    pub(crate) fn open_memory(name: Option<&str>) -> Self {
        let target = if let Some(name) = name {
            format!("file:{name}?mode=memory&cache=shared")
        } else {
            ":memory:".to_string()
        };

        Self::open(target.as_str()).expect("failed to open in-memory sqlite database")
    }

    pub(crate) fn can_write(&self) -> bool {
        self.write.get()
    }

    pub(crate) fn ensure_ok(&self, result_code: std::ffi::c_int) -> anyhow::Result<()> {
        if result_code == SQLITE_OK {
            return Ok(());
        }

        let message = if self.sqlite3.is_null() {
            None
        } else {
            // Safety: self.sqlite3 is a valid SQLite handle owned by Connection
            // and Drop is the only place that closes this handle.
            let message_ptr = unsafe { sqlite3::sqlite3_errmsg(self.sqlite3) };
            if message_ptr.is_null() {
                None
            } else {
                Some(
                    String::from_utf8_lossy(
                        // Safety: The null check above guarantees message_ptr is non-null and SQLite
                        // returns a NUL-terminated error message string for CStr::from_ptr to consume.
                        unsafe { CStr::from_ptr(message_ptr as *const std::ffi::c_char) }
                            .to_bytes(),
                    )
                    .into_owned(),
                )
            }
        };

        anyhow::bail!("sqlite call failed with code {result_code} and message: {message:?}")
    }

    pub(crate) fn ensure_last_result_ok(&self) -> anyhow::Result<()> {
        // Safety: self.sqlite3 is a valid SQLite handle owned by Connection
        // and Drop is the only place that closes this handle.
        let result_code = unsafe { sqlite3::sqlite3_errcode(self.sqlite3) };
        if result_code == SQLITE_ROW {
            return Ok(());
        }

        self.ensure_ok(result_code)
    }

    pub(crate) fn with_write<T>(&self, f: impl FnOnce(&Connection) -> T) -> T {
        struct RestoreWrite<'a> {
            write: &'a Cell<bool>,
            previous: bool,
        }

        impl Drop for RestoreWrite<'_> {
            fn drop(&mut self) {
                self.write.set(self.previous);
            }
        }

        let previous = self.write.replace(true);
        let _restore = RestoreWrite {
            write: &self.write,
            previous,
        };

        f(self)
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        if self.sqlite3.is_null() {
            return;
        }

        // Safety: self.sqlite3 is a valid SQLite handle owned by Connection
        // and this is the only place that closes the handle.
        unsafe { sqlite3::sqlite3_close(self.sqlite3) };
    }
}

#[cfg(unix)]
fn path_to_cstring(path: &Path) -> anyhow::Result<CString> {
    Ok(CString::new(path.as_os_str().as_bytes())?)
}

#[cfg(target_os = "windows")]
fn path_to_cstring(path: &Path) -> anyhow::Result<CString> {
    let path = path.to_str().with_context(|| {
        format!(
            "sqlite database path is not valid UTF-8: {}",
            path.display()
        )
    })?;
    Ok(CString::new(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    use indoc::indoc;
    use std::{panic::AssertUnwindSafe, path::PathBuf};
    use uuid::Uuid;

    #[cfg(unix)]
    use std::{
        ffi::OsString,
        os::unix::ffi::{OsStrExt, OsStringExt},
        path::Path,
    };

    #[cfg(unix)]
    #[test]
    fn test_path_to_cstring_preserves_non_utf8_bytes() {
        let path_bytes = OsString::from_vec(vec![b'd', b'b', 0x80]);
        let path = Path::new(&path_bytes);

        let c_path = path_to_cstring(path).unwrap();

        assert_eq!(c_path.as_bytes(), path.as_os_str().as_bytes());
    }

    #[test]
    fn test_multi_step_statement() {
        let connection = Connection::open_memory(Some("test_multi_step_statement"));

        connection
            .exec("CREATE TABLE test(value INTEGER) STRICT")
            .and_then(|mut f| f())
            .unwrap();

        connection
            .exec("INSERT INTO test(value) VALUES (2)")
            .and_then(|mut f| f())
            .unwrap();

        assert_eq!(
            connection
                .select_row::<usize>("SELECT value FROM test")
                .and_then(|mut f| f())
                .unwrap(),
            Some(2)
        );
    }

    #[test]
    fn test_bound_values_round_trip() {
        let connection = Connection::open_memory(Some("test_bound_values_round_trip"));
        let project_path = PathBuf::from("project");
        let uuid = Uuid::new_v4();

        connection
            .exec(indoc! {"
                CREATE TABLE test(
                    uuid BLOB,
                    session_id TEXT,
                    name TEXT,
                    enabled INTEGER,
                    location BLOB,
                    payload BLOB,
                    created_at INTEGER,
                    updated_at INTEGER
                ) STRICT
            "})
            .and_then(|mut f| f())
            .unwrap();

        connection
            .exec_bound::<(Uuid, Option<&str>, &str, bool, &std::path::Path, &[u8], i32, i32)>(indoc! {"
                INSERT INTO test(uuid, session_id, name, enabled, location, payload, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "})
            .and_then(|mut f| {
                f((
                    uuid,
                    None,
                    "Test",
                    true,
                    project_path.as_path(),
                    &[3, 4, 5],
                    1773054652,
                    1773054652,
                ))
            })
            .unwrap();

        assert_eq!(
            connection
                .select_row::<(Uuid, Option<String>, String, bool, PathBuf, Vec<u8>, i32, i32)>(indoc! {"
                    SELECT uuid, session_id, name, enabled, location, payload, created_at, updated_at
                    FROM test
                "})
                .and_then(|mut f| f())
                .unwrap(),
            Some((
                uuid,
                None,
                "Test".to_string(),
                true,
                project_path,
                vec![3, 4, 5],
                1773054652,
                1773054652,
            ))
        );
    }

    #[test]
    fn test_bound_out_of_range_unsigned_values_fail() {
        let connection =
            Connection::open_memory(Some("test_bound_out_of_range_unsigned_values_fail"));

        connection
            .exec("CREATE TABLE test(value INTEGER) STRICT")
            .and_then(|mut f| f())
            .unwrap();

        assert!(
            connection
                .exec_bound::<u64>("INSERT INTO test(value) VALUES (?1)")
                .and_then(|mut f| f(u64::MAX))
                .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_path_bytes_round_trip() {
        let connection = Connection::open_memory(Some("test_path_bytes_round_trip"));
        let location = PathBuf::from(OsString::from_vec(vec![0x66, 0x6f, 0x80]));

        connection
            .exec("CREATE TABLE test(location BLOB) STRICT")
            .and_then(|mut f| f())
            .unwrap();

        connection
            .exec_bound::<&Path>("INSERT INTO test(location) VALUES (?1)")
            .and_then(|mut f| f(location.as_path()))
            .unwrap();

        assert_eq!(
            connection
                .select_row::<PathBuf>("SELECT location FROM test")
                .and_then(|mut f| f())
                .unwrap(),
            Some(location)
        );
    }

    #[test]
    fn test_read_out_of_range_unsigned_values_fail() {
        let connection =
            Connection::open_memory(Some("test_read_out_of_range_unsigned_values_fail"));

        assert!(
            connection
                .select_row::<u64>("SELECT -1")
                .and_then(|mut f| f())
                .is_err()
        );
        assert!(
            connection
                .select_row::<u32>("SELECT 5000000000")
                .and_then(|mut f| f())
                .is_err()
        );
        assert!(
            connection
                .select_row::<u16>("SELECT 70000")
                .and_then(|mut f| f())
                .is_err()
        );
        assert!(
            connection
                .select_row::<usize>("SELECT -1")
                .and_then(|mut f| f())
                .is_err()
        );
    }

    #[test]
    fn test_null_text_and_blob_require_option_columns() {
        let connection =
            Connection::open_memory(Some("test_null_text_and_blob_require_option_columns"));

        assert!(
            connection
                .select_row::<String>("SELECT CAST(NULL AS TEXT)")
                .and_then(|mut f| f())
                .is_err()
        );
        assert_eq!(
            connection
                .select_row::<Option<String>>("SELECT CAST(NULL AS TEXT)")
                .and_then(|mut f| f())
                .unwrap(),
            Some(None)
        );
        assert_eq!(
            connection
                .select_row::<String>("SELECT ''")
                .and_then(|mut f| f())
                .unwrap(),
            Some(String::new())
        );
        assert!(
            connection
                .select_row::<Vec<u8>>("SELECT CAST(NULL AS BLOB)")
                .and_then(|mut f| f())
                .is_err()
        );
        assert_eq!(
            connection
                .select_row::<Option<Vec<u8>>>("SELECT CAST(NULL AS BLOB)")
                .and_then(|mut f| f())
                .unwrap(),
            Some(None)
        );
        assert_eq!(
            connection
                .select_row::<Vec<u8>>("SELECT zeroblob(0)")
                .and_then(|mut f| f())
                .unwrap(),
            Some(Vec::new())
        );
    }

    #[test]
    fn test_with_write_restores_previous_flag_on_panic() {
        let mut connection =
            Connection::open_memory(Some("test_with_write_restores_previous_flag_on_panic"));

        *connection.write.get_mut() = false;

        let protected_connection = AssertUnwindSafe(&connection);
        let result = std::panic::catch_unwind(move || {
            protected_connection.with_write(|connection| {
                connection
                    .exec("INSERT INTO test(value) VALUES (1)")
                    .and_then(|mut operation| operation())
                    .unwrap();
            });
        });

        assert!(result.is_err());
        assert!(!connection.can_write());
    }
}
