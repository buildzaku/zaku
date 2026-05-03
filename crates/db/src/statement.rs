use anyhow::Context;
use libsqlite3_sys::{
    self as sqlite3, SQLITE_BLOB, SQLITE_DONE, SQLITE_FLOAT, SQLITE_INTEGER, SQLITE_MISUSE,
    SQLITE_NULL, SQLITE_ROW, SQLITE_TEXT, SQLITE_TRANSIENT,
};
use std::{
    ffi::{CStr, CString},
    marker::PhantomData,
};

use crate::{
    bindable::{Bind, Column},
    connection::Connection,
};

pub struct Statement<'conn> {
    raw_statement_ptrs: Vec<*mut sqlite3::sqlite3_stmt>,
    current_statement_idx: usize,
    connection: &'conn Connection,
    _marker: PhantomData<sqlite3::sqlite3_stmt>,
}

pub struct Row<'stmt, 'conn> {
    statement: &'stmt mut Statement<'conn>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum StepResult {
    Row,
    Done,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SqlType {
    Text,
    Integer,
    Blob,
    Float,
    Null,
}

impl<'conn> Statement<'conn> {
    pub fn prepare<T: AsRef<str>>(connection: &'conn Connection, query: T) -> anyhow::Result<Self> {
        let mut statement = Self {
            raw_statement_ptrs: Default::default(),
            current_statement_idx: 0,
            connection,
            _marker: PhantomData,
        };
        let sql = CString::new(query.as_ref()).context("failed to create sqlite cstr")?;
        let mut remaining_sql = sql.as_c_str();
        while {
            let remaining_sql_str = remaining_sql
                .to_str()
                .context("failed to parse remaining sqlite query")?
                .trim();
            remaining_sql_str != ";" && !remaining_sql_str.is_empty()
        } {
            let mut raw_statement_ptr = std::ptr::null_mut::<sqlite3::sqlite3_stmt>();
            let mut remaining_sql_ptr = std::ptr::null();

            // Safety: connection.sqlite3 is a valid SQLite handle, remaining_sql is a
            // NUL-terminated string, raw_statement_ptr and remaining_sql_ptr are
            // valid out-pointers for SQLite to write.
            let result_code = unsafe {
                sqlite3::sqlite3_prepare_v2(
                    connection.sqlite3,
                    remaining_sql.as_ptr(),
                    -1,
                    &raw mut raw_statement_ptr,
                    &raw mut remaining_sql_ptr,
                )
            };

            connection
                .ensure_ok(result_code)
                .with_context(|| format!("prepare call failed for query:\n{}", query.as_ref()))?;

            // Safety: sqlite3_prepare_v2 writes remaining_sql_ptr to point at the unused
            // tail of sql, which stays NUL-terminated and alive for this borrow.
            remaining_sql = unsafe { CStr::from_ptr(remaining_sql_ptr) };

            if raw_statement_ptr.is_null() {
                continue;
            }

            statement.raw_statement_ptrs.push(raw_statement_ptr);

            // Safety: raw_statement_ptr comes from the successful prepare call above and
            // remains valid until sqlite3_finalize in Drop.
            let is_statement_readonly =
                unsafe { sqlite3::sqlite3_stmt_readonly(raw_statement_ptr) != 0 };

            if !connection.can_write() && !is_statement_readonly {
                // Safety: raw_statement_ptr comes from the successful prepare call above and
                // remains valid here. sqlite3_sql returns its NUL-terminated SQL text.
                let sql = unsafe { CStr::from_ptr(sqlite3::sqlite3_sql(raw_statement_ptr)) };

                anyhow::bail!(
                    "write statement prepared with connection that is not write capable. sql:\n{}",
                    sql.to_str()?
                )
            }
        }

        Ok(statement)
    }

    fn current_statement_ptr(&self) -> *mut sqlite3::sqlite3_stmt {
        *self
            .raw_statement_ptrs
            .get(self.current_statement_idx)
            .unwrap()
    }

    pub fn reset(&mut self) -> anyhow::Result<()> {
        let mut reset_error = None;

        for raw_statement_ptr in &self.raw_statement_ptrs {
            // Safety: raw_statement_ptr comes from a successful prepare call and remains
            // valid until sqlite3_finalize in Drop.
            let result_code = unsafe { sqlite3::sqlite3_reset(*raw_statement_ptr) };

            if let Err(error) = self
                .connection
                .ensure_ok(result_code)
                .with_context(|| "failed to reset sqlite statement")
            {
                reset_error.get_or_insert(error);
            }
        }

        self.current_statement_idx = 0;

        if let Some(error) = reset_error {
            return Err(error);
        }

        Ok(())
    }

    fn bind_index_with(
        &self,
        index: i32,
        bind: &dyn Fn(*mut sqlite3::sqlite3_stmt) -> std::ffi::c_int,
    ) -> anyhow::Result<()> {
        let mut any_succeed = false;

        for raw_statement_ptr in &self.raw_statement_ptrs {
            // Safety: raw_statement_ptr comes from a successful prepare call and remains
            // valid until sqlite3_finalize in Drop.
            let parameter_count =
                unsafe { sqlite3::sqlite3_bind_parameter_count(*raw_statement_ptr) };

            if index <= parameter_count {
                let result_code = bind(*raw_statement_ptr);
                self.connection
                    .ensure_ok(result_code)
                    .with_context(|| format!("failed to bind value at index {index}"))?;
                any_succeed = true;
            }
        }

        if any_succeed {
            Ok(())
        } else {
            anyhow::bail!("failed to bind parameters")
        }
    }

    pub fn bind_blob(&self, index: i32, blob: &[u8]) -> anyhow::Result<()> {
        let index = index as std::ffi::c_int;
        let blob_ptr = blob.as_ptr().cast::<std::ffi::c_void>();
        let blob_len = sqlite3::sqlite3_uint64::try_from(blob.len())
            .context("blob length exceeds sqlite3_uint64 range")?;

        self.bind_index_with(index, &|raw_statement_ptr| {
            // Safety: raw_statement_ptr comes from a successful prepare call, blob_ptr
            // and blob_len were derived from blob. SQLITE_TRANSIENT avoids borrowing blob after
            // the bind call returns.
            unsafe {
                if blob_len == 0 {
                    sqlite3::sqlite3_bind_zeroblob(raw_statement_ptr, index, 0)
                } else {
                    sqlite3::sqlite3_bind_blob64(
                        raw_statement_ptr,
                        index,
                        blob_ptr,
                        blob_len,
                        SQLITE_TRANSIENT(),
                    )
                }
            }
        })
    }

    pub fn bind_double(&self, index: i32, double: f64) -> anyhow::Result<()> {
        let index = index as std::ffi::c_int;

        self.bind_index_with(index, &|raw_statement_ptr| {
            // Safety: raw_statement_ptr comes from a successful prepare call and remains
            // valid until sqlite3_finalize in Drop.
            unsafe { sqlite3::sqlite3_bind_double(raw_statement_ptr, index, double) }
        })
    }

    pub fn bind_int(&self, index: i32, int: i32) -> anyhow::Result<()> {
        let index = index as std::ffi::c_int;
        self.bind_index_with(index, &|raw_statement_ptr| {
            // Safety: raw_statement_ptr comes from a successful prepare call and remains
            // valid until sqlite3_finalize in Drop.
            unsafe { sqlite3::sqlite3_bind_int(raw_statement_ptr, index, int) }
        })
    }

    pub fn bind_int64(&self, index: i32, int: i64) -> anyhow::Result<()> {
        let index = index as std::ffi::c_int;
        self.bind_index_with(index, &|raw_statement_ptr| {
            // Safety: raw_statement_ptr comes from a successful prepare call and remains
            // valid until sqlite3_finalize in Drop.
            unsafe { sqlite3::sqlite3_bind_int64(raw_statement_ptr, index, int) }
        })
    }

    pub fn bind_null(&self, index: i32) -> anyhow::Result<()> {
        let index = index as std::ffi::c_int;
        self.bind_index_with(index, &|raw_statement_ptr| {
            // Safety: raw_statement_ptr comes from a successful prepare call and remains
            // valid until sqlite3_finalize in Drop.
            unsafe { sqlite3::sqlite3_bind_null(raw_statement_ptr, index) }
        })
    }

    pub fn bind_text(&self, index: i32, text: &str) -> anyhow::Result<()> {
        let index = index as std::ffi::c_int;
        let text_ptr = text.as_ptr().cast::<std::ffi::c_char>();
        let text_len = sqlite3::sqlite3_uint64::try_from(text.len())
            .context("text length exceeds sqlite3_uint64 range")?;

        self.bind_index_with(index, &|raw_statement_ptr| {
            // Safety: raw_statement_ptr comes from a successful prepare call, text_ptr
            // and text_len were derived from text. SQLITE_TRANSIENT avoids borrowing text after
            // the bind call returns.
            unsafe {
                sqlite3::sqlite3_bind_text64(
                    raw_statement_ptr,
                    index,
                    text_ptr,
                    text_len,
                    SQLITE_TRANSIENT(),
                    sqlite3::SQLITE_UTF8 as std::ffi::c_uchar,
                )
            }
        })
    }

    pub fn bind<T: Bind>(&self, value: &T, index: i32) -> anyhow::Result<i32> {
        debug_assert!(index > 0);
        value.bind(self, index)
    }

    pub fn with_bindings(&mut self, bindings: &impl Bind) -> anyhow::Result<&mut Self> {
        self.bind(bindings, 1)?;
        Ok(self)
    }

    fn with_reset<T>(&mut self, result: anyhow::Result<T>) -> anyhow::Result<T> {
        let reset_result = self.reset();

        match result {
            Ok(value) => {
                reset_result?;
                Ok(value)
            }
            Err(error) => {
                if let Err(e) = reset_result {
                    return Err(error.context(format!("statement reset also failed: {e:#}")));
                }
                Err(error)
            }
        }
    }

    fn step(&mut self) -> anyhow::Result<StepResult> {
        if self.raw_statement_ptrs.is_empty() {
            return Ok(StepResult::Done);
        }

        // Safety: current_statement_ptr() returns a SQLite statement handle from
        // raw_statement_ptrs and that handle remains valid until sqlite3_finalize in Drop.
        let result_code = unsafe { sqlite3::sqlite3_step(self.current_statement_ptr()) };
        match result_code {
            SQLITE_ROW => Ok(StepResult::Row),
            SQLITE_DONE => {
                if self.current_statement_idx >= self.raw_statement_ptrs.len() - 1 {
                    Ok(StepResult::Done)
                } else {
                    self.current_statement_idx += 1;
                    self.step()
                }
            }
            SQLITE_MISUSE => anyhow::bail!("statement step returned SQLITE_MISUSE"),
            _ => {
                self.connection
                    .ensure_ok(result_code)
                    .with_context(|| "failed to step sqlite statement".to_string())?;
                unreachable!("ensure_ok returned Ok for a failing sqlite3_step result code");
            }
        }
    }

    pub fn next_row<'stmt>(&'stmt mut self) -> anyhow::Result<Option<Row<'stmt, 'conn>>> {
        match self.step()? {
            StepResult::Row => Ok(Some(Row { statement: self })),
            StepResult::Done => Ok(None),
        }
    }

    pub fn exec(&mut self) -> anyhow::Result<()> {
        fn inner(statement: &mut Statement<'_>) -> anyhow::Result<()> {
            while statement.step()? == StepResult::Row {}
            Ok(())
        }

        let result = inner(self);
        self.with_reset(result)
    }

    pub fn map<R>(
        &mut self,
        f: impl for<'stmt> FnMut(&mut Row<'stmt, 'conn>) -> anyhow::Result<R>,
    ) -> anyhow::Result<Vec<R>> {
        fn inner<'conn, R>(
            statement: &mut Statement<'conn>,
            mut f: impl for<'stmt> FnMut(&mut Row<'stmt, 'conn>) -> anyhow::Result<R>,
        ) -> anyhow::Result<Vec<R>> {
            let mut mapped_rows = Vec::new();
            while let Some(mut row) = statement.next_row()? {
                mapped_rows.push(f(&mut row)?);
            }
            Ok(mapped_rows)
        }

        let result = inner(self, f);
        self.with_reset(result)
    }

    pub fn rows<R: Column>(&mut self) -> anyhow::Result<Vec<R>> {
        self.map(|row| row.column::<R>())
    }

    pub fn single<R>(
        &mut self,
        f: impl for<'stmt> FnOnce(&mut Row<'stmt, 'conn>) -> anyhow::Result<R>,
    ) -> anyhow::Result<R> {
        fn inner<'conn, R>(
            statement: &mut Statement<'conn>,
            f: impl for<'stmt> FnOnce(&mut Row<'stmt, 'conn>) -> anyhow::Result<R>,
        ) -> anyhow::Result<R> {
            let mut row = statement
                .next_row()?
                .ok_or_else(|| anyhow::anyhow!("single called with query that returns no rows"))?;
            let result = f(&mut row)?;
            anyhow::ensure!(
                statement.next_row()?.is_none(),
                "single called with a query that returns more than one row"
            );

            Ok(result)
        }

        let result = inner(self, f);
        self.with_reset(result)
    }

    pub fn row<R: Column>(&mut self) -> anyhow::Result<R> {
        self.single(|row| row.column::<R>())
    }

    pub fn maybe<R>(
        &mut self,
        f: impl for<'stmt> FnOnce(&mut Row<'stmt, 'conn>) -> anyhow::Result<R>,
    ) -> anyhow::Result<Option<R>> {
        fn inner<'conn, R>(
            statement: &mut Statement<'conn>,
            f: impl for<'stmt> FnOnce(&mut Row<'stmt, 'conn>) -> anyhow::Result<R>,
        ) -> anyhow::Result<Option<R>> {
            let Some(mut row) = statement.next_row().context("failed on step call")? else {
                return Ok(None);
            };
            let result = f(&mut row)
                .map(Some)
                .context("failed to parse row result")?;
            anyhow::ensure!(
                statement.next_row().context("second step call")?.is_none(),
                "maybe called with a query that returns more than one row"
            );

            Ok(result)
        }

        let result = inner(self, f);
        self.with_reset(result)
    }

    pub fn maybe_row<R: Column>(&mut self) -> anyhow::Result<Option<R>> {
        self.maybe(|row| row.column::<R>())
    }
}

impl Row<'_, '_> {
    pub fn column_blob(&mut self, index: i32) -> anyhow::Result<&[u8]> {
        anyhow::ensure!(
            !matches!(self.column_type(index)?, SqlType::Null),
            "NULL blob at index {index}"
        );

        let index = index as std::ffi::c_int;

        // Safety: current_statement_ptr() is a valid SQLite statement handle for the
        // current row and index refers to a column in that row.
        let blob_ptr =
            unsafe { sqlite3::sqlite3_column_blob(self.statement.current_statement_ptr(), index) };

        self.statement
            .connection
            .ensure_last_result_ok()
            .with_context(|| format!("failed to read blob at index {index}"))?;

        // Safety: current_statement_ptr() is a valid SQLite statement handle for the
        // current row and index refers to a column in that row.
        let blob_len = unsafe {
            sqlite3::sqlite3_column_bytes(self.statement.current_statement_ptr(), index) as usize
        };

        self.statement
            .connection
            .ensure_last_result_ok()
            .with_context(|| format!("failed to read blob length at index {index}"))?;

        if blob_len == 0 {
            return Ok(&[]);
        }

        anyhow::ensure!(
            !blob_ptr.is_null(),
            "blob pointer was null at index {index}"
        );

        // Safety: blob_ptr and blob_len came from SQLite for the current row and the
        // checks above guarantee a valid non-null pointer with blob_len > 0.
        let result = unsafe { std::slice::from_raw_parts(blob_ptr.cast::<u8>(), blob_len) };

        Ok(result)
    }

    pub fn column_double(&mut self, index: i32) -> anyhow::Result<f64> {
        let index = index as std::ffi::c_int;

        // Safety: current_statement_ptr() is a valid SQLite statement handle for the
        // current row and index refers to a column in that row.
        let result = unsafe {
            sqlite3::sqlite3_column_double(self.statement.current_statement_ptr(), index)
        };

        self.statement
            .connection
            .ensure_last_result_ok()
            .with_context(|| format!("failed to read double at index {index}"))?;
        Ok(result)
    }

    pub fn column_int(&mut self, index: i32) -> anyhow::Result<i32> {
        let index = index as std::ffi::c_int;

        // Safety: current_statement_ptr() is a valid SQLite statement handle for the
        // current row and index refers to a column in that row.
        let result =
            unsafe { sqlite3::sqlite3_column_int(self.statement.current_statement_ptr(), index) };

        self.statement
            .connection
            .ensure_last_result_ok()
            .with_context(|| format!("failed to read int at index {index}"))?;
        Ok(result)
    }

    pub fn column_int64(&mut self, index: i32) -> anyhow::Result<i64> {
        let index = index as std::ffi::c_int;

        // Safety: current_statement_ptr() is a valid SQLite statement handle for the
        // current row and index refers to a column in that row.
        let result =
            unsafe { sqlite3::sqlite3_column_int64(self.statement.current_statement_ptr(), index) };

        self.statement
            .connection
            .ensure_last_result_ok()
            .with_context(|| format!("failed to read i64 at index {index}"))?;
        Ok(result)
    }

    pub fn column_text(&mut self, index: i32) -> anyhow::Result<&str> {
        anyhow::ensure!(
            !matches!(self.column_type(index)?, SqlType::Null),
            "NULL text at index {index}"
        );

        let index = index as std::ffi::c_int;

        // Safety: current_statement_ptr() is a valid SQLite statement handle for the
        // current row and index refers to a column in that row.
        let text_ptr =
            unsafe { sqlite3::sqlite3_column_text(self.statement.current_statement_ptr(), index) };

        self.statement
            .connection
            .ensure_last_result_ok()
            .with_context(|| format!("failed to read text from column {index}"))?;

        // Safety: current_statement_ptr() is a valid SQLite statement handle for the
        // current row and index refers to a column in that row.
        let text_len = unsafe {
            sqlite3::sqlite3_column_bytes(self.statement.current_statement_ptr(), index) as usize
        };

        self.statement
            .connection
            .ensure_last_result_ok()
            .with_context(|| format!("failed to read text length at {index}"))?;

        if text_len == 0 {
            return Ok("");
        }

        anyhow::ensure!(
            !text_ptr.is_null(),
            "text pointer was null at index {index}"
        );

        // Safety: text_ptr and text_len came from SQLite for the current row and the
        // checks above guarantee a valid non-null pointer with text_len > 0.
        let slice = unsafe { std::slice::from_raw_parts(text_ptr, text_len) };

        let result = std::str::from_utf8(slice)?;
        Ok(result)
    }

    pub fn column<T: Column>(&mut self) -> anyhow::Result<T> {
        Ok(T::column(self, 0)?.0)
    }

    pub fn column_type(&mut self, index: i32) -> anyhow::Result<SqlType> {
        // Safety: current_statement_ptr() is a valid SQLite statement handle for the
        // current row and index refers to a column in that row.
        let result =
            unsafe { sqlite3::sqlite3_column_type(self.statement.current_statement_ptr(), index) };

        self.statement.connection.ensure_last_result_ok()?;
        match result {
            SQLITE_INTEGER => Ok(SqlType::Integer),
            SQLITE_FLOAT => Ok(SqlType::Float),
            SQLITE_TEXT => Ok(SqlType::Text),
            SQLITE_BLOB => Ok(SqlType::Blob),
            SQLITE_NULL => Ok(SqlType::Null),
            _ => anyhow::bail!("column type returned was incorrect"),
        }
    }
}

impl Drop for Statement<'_> {
    fn drop(&mut self) {
        // Safety: Each raw_statement_ptr comes from a successful prepare call and
        // this is the only place that finalizes any statements still owned by Statement.
        unsafe {
            for raw_statement_ptr in &self.raw_statement_ptrs {
                sqlite3::sqlite3_finalize(*raw_statement_ptr);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use indoc::indoc;

    use crate::StaticColumnCount;

    #[test]
    fn test_custom_column_impl_reads_from_row() {
        #[derive(Debug, PartialEq)]
        struct Pair {
            name: String,
            count: i64,
        }

        impl StaticColumnCount for Pair {
            fn column_count() -> usize {
                2
            }
        }

        impl Column for Pair {
            fn column<'stmt, 'conn>(
                row: &mut Row<'stmt, 'conn>,
                start_index: i32,
            ) -> anyhow::Result<(Self, i32)> {
                let (name, next_index) = String::column(row, start_index)?;
                let (count, next_index) = i64::column(row, next_index)?;
                Ok((Self { name, count }, next_index))
            }
        }

        let connection = Connection::open_memory(Some("test_custom_column_impl_reads_from_row"));

        connection
            .exec("CREATE TABLE test(name TEXT, count INTEGER) STRICT")
            .and_then(|mut f| f())
            .unwrap();

        connection
            .exec("INSERT INTO test(name, count) VALUES ('workspace', 3)")
            .and_then(|mut f| f())
            .unwrap();

        assert_eq!(
            connection
                .select_row::<Pair>("SELECT name, count FROM test")
                .and_then(|mut f| f())
                .unwrap(),
            Some(Pair {
                name: "workspace".to_string(),
                count: 3,
            })
        );
    }

    #[test]
    fn test_statement_supports_parameter_gaps() {
        let connection = Connection::open_memory(Some("test_statement_supports_parameter_gaps"));

        connection
            .exec("CREATE TABLE test(col INTEGER) STRICT")
            .and_then(|mut f| f())
            .unwrap();

        let statement = Statement::prepare(
            &connection,
            indoc! {"
                INSERT INTO test(col) VALUES (?3);
                SELECT col FROM test WHERE col = ?1
            "},
        )
        .unwrap();

        statement.bind_int(1, 1).unwrap();
        statement.bind_int(2, 2).unwrap();
        statement.bind_int(3, 3).unwrap();
    }

    #[test]
    fn test_statement_step_and_reset() {
        let connection = Connection::open_memory(Some("test_statement_step_and_reset"));

        connection
            .exec("CREATE TABLE test(value INTEGER) STRICT")
            .and_then(|mut f| f())
            .unwrap();

        connection
            .exec("INSERT INTO test(value) VALUES (7)")
            .and_then(|mut f| f())
            .unwrap();

        let mut statement = Statement::prepare(&connection, "SELECT value FROM test").unwrap();

        let mut row = statement.next_row().unwrap().unwrap();
        assert_eq!(row.column_int64(0).unwrap(), 7);
        assert!(statement.next_row().unwrap().is_none());

        statement.reset().unwrap();

        let mut row = statement.next_row().unwrap().unwrap();
        assert_eq!(row.column_int64(0).unwrap(), 7);
        assert!(statement.next_row().unwrap().is_none());
    }

    #[test]
    fn test_prepare_skips_null_statements() {
        let connection = Connection::open_memory(Some("test_prepare_skips_null_statements"));
        let mut statement = Statement::prepare(&connection, "/* no-op */").unwrap();

        assert!(statement.next_row().unwrap().is_none());
        statement.exec().unwrap();
    }
}
