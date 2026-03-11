use anyhow::Context;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use util::path::PathExt;

use crate::statement::{Row, SqlType, Statement};

pub trait StaticColumnCount {
    fn column_count() -> usize {
        1
    }
}

pub trait Bind {
    fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32>;
}

pub trait Column: Sized {
    fn column<'stmt, 'conn>(
        row: &mut Row<'stmt, 'conn>,
        start_index: i32,
    ) -> anyhow::Result<(Self, i32)>;
}

impl StaticColumnCount for bool {}

impl Bind for bool {
    fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
        statement
            .bind(&self.then_some(1).unwrap_or(0), start_index)
            .with_context(|| format!("failed to bind bool at index {start_index}"))
    }
}

impl Column for bool {
    fn column<'stmt, 'conn>(
        row: &mut Row<'stmt, 'conn>,
        start_index: i32,
    ) -> anyhow::Result<(Self, i32)> {
        i32::column(row, start_index)
            .map(|(value, next_index)| (value != 0, next_index))
            .with_context(|| format!("failed to read bool at index {start_index}"))
    }
}

impl StaticColumnCount for &[u8] {}

impl Bind for &[u8] {
    fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
        statement
            .bind_blob(start_index, self)
            .with_context(|| format!("failed to bind &[u8] at index {start_index}"))?;
        Ok(start_index + 1)
    }
}

impl<const COUNT: usize> StaticColumnCount for &[u8; COUNT] {}

impl<const COUNT: usize> Bind for &[u8; COUNT] {
    fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
        statement
            .bind_blob(start_index, self.as_slice())
            .with_context(|| format!("failed to bind &[u8; COUNT] at index {start_index}"))?;
        Ok(start_index + 1)
    }
}

impl<const COUNT: usize> Column for [u8; COUNT] {
    fn column<'stmt, 'conn>(
        row: &mut Row<'stmt, 'conn>,
        start_index: i32,
    ) -> anyhow::Result<(Self, i32)> {
        let bytes_slice = row.column_blob(start_index)?;
        let array = bytes_slice.try_into()?;
        Ok((array, start_index + 1))
    }
}

impl StaticColumnCount for Vec<u8> {}

impl Bind for Vec<u8> {
    fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
        statement
            .bind_blob(start_index, self)
            .with_context(|| format!("failed to bind Vec<u8> at index {start_index}"))?;
        Ok(start_index + 1)
    }
}

impl Column for Vec<u8> {
    fn column<'stmt, 'conn>(
        row: &mut Row<'stmt, 'conn>,
        start_index: i32,
    ) -> anyhow::Result<(Self, i32)> {
        let result = row
            .column_blob(start_index)
            .with_context(|| format!("failed to read Vec<u8> at index {start_index}"))?;

        Ok((Vec::from(result), start_index + 1))
    }
}

impl StaticColumnCount for f64 {}

impl Bind for f64 {
    fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
        statement
            .bind_double(start_index, *self)
            .with_context(|| format!("failed to bind f64 at index {start_index}"))?;
        Ok(start_index + 1)
    }
}

impl Column for f64 {
    fn column<'stmt, 'conn>(
        row: &mut Row<'stmt, 'conn>,
        start_index: i32,
    ) -> anyhow::Result<(Self, i32)> {
        let result = row
            .column_double(start_index)
            .with_context(|| format!("failed to parse f64 at index {start_index}"))?;

        Ok((result, start_index + 1))
    }
}

impl StaticColumnCount for f32 {}

impl Bind for f32 {
    fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
        statement
            .bind_double(start_index, *self as f64)
            .with_context(|| format!("failed to bind f32 at index {start_index}"))?;
        Ok(start_index + 1)
    }
}

impl Column for f32 {
    fn column<'stmt, 'conn>(
        row: &mut Row<'stmt, 'conn>,
        start_index: i32,
    ) -> anyhow::Result<(Self, i32)> {
        let result = row
            .column_double(start_index)
            .with_context(|| format!("failed to parse f32 at index {start_index}"))?
            as f32;

        Ok((result, start_index + 1))
    }
}

impl StaticColumnCount for i32 {}

impl Bind for i32 {
    fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
        statement
            .bind_int(start_index, *self)
            .with_context(|| format!("failed to bind i32 at index {start_index}"))?;
        Ok(start_index + 1)
    }
}

impl Column for i32 {
    fn column<'stmt, 'conn>(
        row: &mut Row<'stmt, 'conn>,
        start_index: i32,
    ) -> anyhow::Result<(Self, i32)> {
        let result = row.column_int(start_index)?;
        Ok((result, start_index + 1))
    }
}

impl StaticColumnCount for i64 {}

impl Bind for i64 {
    fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
        statement
            .bind_int64(start_index, *self)
            .with_context(|| format!("failed to bind i64 at index {start_index}"))?;
        Ok(start_index + 1)
    }
}

impl Column for i64 {
    fn column<'stmt, 'conn>(
        row: &mut Row<'stmt, 'conn>,
        start_index: i32,
    ) -> anyhow::Result<(Self, i32)> {
        let result = row.column_int64(start_index)?;
        Ok((result, start_index + 1))
    }
}

impl StaticColumnCount for u64 {}

impl Bind for u64 {
    fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
        i64::try_from(*self)
            .with_context(|| format!("u64 exceeds SQLite INTEGER range at index {start_index}"))?
            .bind(statement, start_index)
            .with_context(|| format!("failed to bind u64 at index {start_index}"))
    }
}

impl Column for u64 {
    fn column<'stmt, 'conn>(
        row: &mut Row<'stmt, 'conn>,
        start_index: i32,
    ) -> anyhow::Result<(Self, i32)> {
        let raw = row.column_int64(start_index)?;
        let result = u64::try_from(raw).with_context(|| {
            format!("negative or out-of-range u64 at index {start_index}: {raw}")
        })?;
        Ok((result, start_index + 1))
    }
}

impl StaticColumnCount for u32 {}

impl Bind for u32 {
    fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
        i64::from(*self)
            .bind(statement, start_index)
            .with_context(|| format!("failed to bind u32 at index {start_index}"))
    }
}

impl Column for u32 {
    fn column<'stmt, 'conn>(
        row: &mut Row<'stmt, 'conn>,
        start_index: i32,
    ) -> anyhow::Result<(Self, i32)> {
        let raw = row.column_int64(start_index)?;
        let result = u32::try_from(raw).with_context(|| {
            format!("negative or out-of-range u32 at index {start_index}: {raw}")
        })?;
        Ok((result, start_index + 1))
    }
}

impl StaticColumnCount for u16 {}

impl Bind for u16 {
    fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
        i64::from(*self)
            .bind(statement, start_index)
            .with_context(|| format!("failed to bind u16 at index {start_index}"))
    }
}

impl Column for u16 {
    fn column<'stmt, 'conn>(
        row: &mut Row<'stmt, 'conn>,
        start_index: i32,
    ) -> anyhow::Result<(Self, i32)> {
        let raw = row.column_int64(start_index)?;
        let result = u16::try_from(raw).with_context(|| {
            format!("negative or out-of-range u16 at index {start_index}: {raw}")
        })?;
        Ok((result, start_index + 1))
    }
}

impl StaticColumnCount for usize {}

impl Bind for usize {
    fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
        i64::try_from(*self)
            .with_context(|| format!("usize exceeds SQLite INTEGER range at index {start_index}"))?
            .bind(statement, start_index)
            .with_context(|| format!("failed to bind usize at index {start_index}"))
    }
}

impl Column for usize {
    fn column<'stmt, 'conn>(
        row: &mut Row<'stmt, 'conn>,
        start_index: i32,
    ) -> anyhow::Result<(Self, i32)> {
        let raw = row.column_int64(start_index)?;
        let result = usize::try_from(raw).with_context(|| {
            format!("negative or out-of-range usize at index {start_index}: {raw}")
        })?;
        Ok((result, start_index + 1))
    }
}

impl StaticColumnCount for &str {}

impl Bind for &str {
    fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
        statement.bind_text(start_index, self)?;
        Ok(start_index + 1)
    }
}

impl StaticColumnCount for Arc<str> {}

impl Bind for Arc<str> {
    fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
        statement.bind_text(start_index, self.as_ref())?;
        Ok(start_index + 1)
    }
}

impl StaticColumnCount for String {}

impl Bind for String {
    fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
        statement.bind_text(start_index, self)?;
        Ok(start_index + 1)
    }
}

impl Column for Arc<str> {
    fn column<'stmt, 'conn>(
        row: &mut Row<'stmt, 'conn>,
        start_index: i32,
    ) -> anyhow::Result<(Self, i32)> {
        let result = row.column_text(start_index)?;
        Ok((Arc::from(result), start_index + 1))
    }
}

impl Column for String {
    fn column<'stmt, 'conn>(
        row: &mut Row<'stmt, 'conn>,
        start_index: i32,
    ) -> anyhow::Result<(Self, i32)> {
        let result = row.column_text(start_index)?;
        Ok((result.to_owned(), start_index + 1))
    }
}

impl<T: StaticColumnCount> StaticColumnCount for Option<T> {
    fn column_count() -> usize {
        T::column_count()
    }
}

impl<T: Bind + StaticColumnCount> Bind for Option<T> {
    fn bind(&self, statement: &Statement<'_>, mut start_index: i32) -> anyhow::Result<i32> {
        if let Some(value) = self {
            value.bind(statement, start_index)
        } else {
            for _ in 0..T::column_count() {
                statement.bind_null(start_index)?;
                start_index += 1;
            }
            Ok(start_index)
        }
    }
}

impl<T: Column + StaticColumnCount> Column for Option<T> {
    fn column<'stmt, 'conn>(
        row: &mut Row<'stmt, 'conn>,
        start_index: i32,
    ) -> anyhow::Result<(Self, i32)> {
        if let SqlType::Null = row.column_type(start_index)? {
            Ok((None, start_index + T::column_count() as i32))
        } else {
            T::column(row, start_index).map(|(result, next_index)| (Some(result), next_index))
        }
    }
}

impl<T: StaticColumnCount, const COUNT: usize> StaticColumnCount for [T; COUNT] {
    fn column_count() -> usize {
        T::column_count() * COUNT
    }
}

impl<T: Bind, const COUNT: usize> Bind for [T; COUNT] {
    fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
        let mut current_index = start_index;
        for binding in self {
            current_index = binding.bind(statement, current_index)?;
        }

        Ok(current_index)
    }
}

impl StaticColumnCount for &Path {}

impl Bind for &Path {
    fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
        self.as_os_str()
            .as_encoded_bytes()
            .bind(statement, start_index)
    }
}

impl StaticColumnCount for Arc<Path> {}

impl Bind for Arc<Path> {
    fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
        self.as_ref().bind(statement, start_index)
    }
}

impl Column for Arc<Path> {
    fn column<'stmt, 'conn>(
        row: &mut Row<'stmt, 'conn>,
        start_index: i32,
    ) -> anyhow::Result<(Self, i32)> {
        let blob = row.column_blob(start_index)?;
        let path = PathBuf::try_from_bytes(blob)?;
        Ok((Arc::from(path.as_path()), start_index + 1))
    }
}

impl StaticColumnCount for PathBuf {}

impl Bind for PathBuf {
    fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
        (self.as_ref() as &Path).bind(statement, start_index)
    }
}

impl Column for PathBuf {
    fn column<'stmt, 'conn>(
        row: &mut Row<'stmt, 'conn>,
        start_index: i32,
    ) -> anyhow::Result<(Self, i32)> {
        let blob = row.column_blob(start_index)?;
        let path = PathBuf::try_from_bytes(blob)?;
        Ok((path, start_index + 1))
    }
}

impl StaticColumnCount for uuid::Uuid {
    fn column_count() -> usize {
        1
    }
}

impl Bind for uuid::Uuid {
    fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
        self.as_bytes().bind(statement, start_index)
    }
}

impl Column for uuid::Uuid {
    fn column<'stmt, 'conn>(
        row: &mut Row<'stmt, 'conn>,
        start_index: i32,
    ) -> anyhow::Result<(Self, i32)> {
        let (bytes, next_index) = Column::column(row, start_index)?;
        Ok((uuid::Uuid::from_bytes(bytes), next_index))
    }
}

impl StaticColumnCount for () {
    fn column_count() -> usize {
        0
    }
}

impl Bind for () {
    fn bind(&self, _statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
        Ok(start_index)
    }
}

impl Column for () {
    fn column<'stmt, 'conn>(
        _row: &mut Row<'stmt, 'conn>,
        start_index: i32,
    ) -> anyhow::Result<(Self, i32)> {
        Ok(((), start_index))
    }
}

macro_rules! impl_tuple_row_traits {
    ( $($local:ident: $type:ident),+ ) => {
        impl<$($type: StaticColumnCount),+> StaticColumnCount for ($($type,)+) {
            fn column_count() -> usize {
                let mut count = 0;
                $(count += $type::column_count();)+
                count
            }
        }

        impl<$($type: Bind),+> Bind for ($($type,)+) {
            fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
                let mut next_index = start_index;
                let ($($local,)+) = self;
                $(next_index = $local.bind(statement, next_index)?;)+
                Ok(next_index)
            }
        }

        impl<$($type: Column),+> Column for ($($type,)+) {
            fn column<'stmt, 'conn>(
                row: &mut Row<'stmt, 'conn>,
                start_index: i32,
            ) -> anyhow::Result<(Self, i32)> {
                let mut next_index = start_index;
                Ok((
                    (
                        $({
                            let value;
                            (value, next_index) = $type::column(row, next_index)?;
                            value
                        },)+
                    ),
                    next_index,
                ))
            }
        }
    }
}

impl_tuple_row_traits!(t1: T1, t2: T2);
impl_tuple_row_traits!(t1: T1, t2: T2, t3: T3);
impl_tuple_row_traits!(t1: T1, t2: T2, t3: T3, t4: T4);
impl_tuple_row_traits!(t1: T1, t2: T2, t3: T3, t4: T4, t5: T5);
impl_tuple_row_traits!(t1: T1, t2: T2, t3: T3, t4: T4, t5: T5, t6: T6);
impl_tuple_row_traits!(t1: T1, t2: T2, t3: T3, t4: T4, t5: T5, t6: T6, t7: T7);
impl_tuple_row_traits!(
    t1: T1,
    t2: T2,
    t3: T3,
    t4: T4,
    t5: T5,
    t6: T6,
    t7: T7,
    t8: T8
);
impl_tuple_row_traits!(
    t1: T1,
    t2: T2,
    t3: T3,
    t4: T4,
    t5: T5,
    t6: T6,
    t7: T7,
    t8: T8,
    t9: T9
);
impl_tuple_row_traits!(
    t1: T1,
    t2: T2,
    t3: T3,
    t4: T4,
    t5: T5,
    t6: T6,
    t7: T7,
    t8: T8,
    t9: T9,
    t10: T10
);
