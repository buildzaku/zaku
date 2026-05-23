#[macro_export]
macro_rules! query {
    ($vis:vis fn $id:ident() -> anyhow::Result<()> { $($sql:tt)+ }) => {
        $vis fn $id(&self) -> $crate::anyhow::Result<()> {
            use $crate::anyhow::Context;

            self.read(|connection| {
                let sql_stmt = $crate::sql_macros::sql!($($sql)+);

                connection.exec(sql_stmt)?().context(::std::format!(
                    "Error in {}, exec failed to execute or parse for: {}",
                    stringify!($id),
                    sql_stmt,
                ))
            })
        }
    };
    ($vis:vis async fn $id:ident() -> anyhow::Result<()> { $($sql:tt)+ }) => {
        $vis async fn $id(&self) -> $crate::anyhow::Result<()> {
            use $crate::anyhow::Context;

            self.write(|connection| {
                let sql_stmt = $crate::sql_macros::sql!($($sql)+);

                connection.exec(sql_stmt)?().context(::std::format!(
                    "Error in {}, exec failed to execute or parse for: {}",
                    stringify!($id),
                    sql_stmt,
                ))
            })
            .await
        }
    };
    ($vis:vis fn $id:ident($($arg:ident: $arg_type:ty),+ $(,)?) -> anyhow::Result<()> { $($sql:tt)+ }) => {
        $vis fn $id(&self, $($arg: $arg_type),+) -> $crate::anyhow::Result<()> {
            use $crate::anyhow::Context;

            self.read(|connection| {
                let sql_stmt = $crate::sql_macros::sql!($($sql)+);

                connection.exec_bound::<($($arg_type),+)>(sql_stmt)?(($($arg),+))
                    .context(::std::format!(
                        "Error in {}, exec_bound failed to execute or parse for: {}",
                        stringify!($id),
                        sql_stmt,
                    ))
            })
        }
    };
    ($vis:vis async fn $id:ident($arg:ident: $arg_type:ty $(,)?) -> anyhow::Result<()> { $($sql:tt)+ }) => {
        $vis async fn $id(&self, $arg: $arg_type) -> $crate::anyhow::Result<()> {
            use $crate::anyhow::Context;

            self.write(move |connection| {
                let sql_stmt = $crate::sql_macros::sql!($($sql)+);

                connection.exec_bound::<$arg_type>(sql_stmt)?($arg)
                    .context(::std::format!(
                        "Error in {}, exec_bound failed to execute or parse for: {}",
                        stringify!($id),
                        sql_stmt,
                    ))
            })
            .await
        }
    };
    ($vis:vis async fn $id:ident($($arg:ident: $arg_type:ty),+ $(,)?) -> anyhow::Result<()> { $($sql:tt)+ }) => {
        $vis async fn $id(&self, $($arg: $arg_type),+) -> $crate::anyhow::Result<()> {
            use $crate::anyhow::Context;

            self.write(move |connection| {
                let sql_stmt = $crate::sql_macros::sql!($($sql)+);

                connection.exec_bound::<($($arg_type),+)>(sql_stmt)?(($($arg),+))
                    .context(::std::format!(
                        "Error in {}, exec_bound failed to execute or parse for: {}",
                        stringify!($id),
                        sql_stmt,
                    ))
            })
            .await
        }
    };
    ($vis:vis fn $id:ident() -> anyhow::Result<Vec<$return_type:ty>> { $($sql:tt)+ }) => {
        $vis fn $id(&self) -> $crate::anyhow::Result<Vec<$return_type>> {
            use $crate::anyhow::Context;

            self.read(|connection| {
                let sql_stmt = $crate::sql_macros::sql!($($sql)+);

                connection.select::<$return_type>(sql_stmt)?()
                    .context(::std::format!(
                        "Error in {}, select failed to execute or parse for: {}",
                        stringify!($id),
                        sql_stmt,
                    ))
            })
        }
    };
    ($vis:vis async fn $id:ident() -> anyhow::Result<Vec<$return_type:ty>> { $($sql:tt)+ }) => {
        $vis async fn $id(&self) -> $crate::anyhow::Result<Vec<$return_type>> {
            use $crate::anyhow::Context;

            self.write(|connection| {
                let sql_stmt = $crate::sql_macros::sql!($($sql)+);

                connection.select::<$return_type>(sql_stmt)?()
                    .context(::std::format!(
                        "Error in {}, select failed to execute or parse for: {}",
                        stringify!($id),
                        sql_stmt,
                    ))
            })
            .await
        }
    };
    ($vis:vis fn $id:ident($($arg:ident: $arg_type:ty),+ $(,)?) -> anyhow::Result<Vec<$return_type:ty>> { $($sql:tt)+ }) => {
        $vis fn $id(&self, $($arg: $arg_type),+) -> $crate::anyhow::Result<Vec<$return_type>> {
            use $crate::anyhow::Context;

            self.read(|connection| {
                let sql_stmt = $crate::sql_macros::sql!($($sql)+);

                connection.select_bound::<($($arg_type),+), $return_type>(sql_stmt)?(($($arg),+))
                    .context(::std::format!(
                        "Error in {}, select_bound failed to execute or parse for: {}",
                        stringify!($id),
                        sql_stmt,
                    ))
            })
        }
    };
    ($vis:vis async fn $id:ident($($arg:ident: $arg_type:ty),+ $(,)?) -> anyhow::Result<Vec<$return_type:ty>> { $($sql:tt)+ }) => {
        $vis async fn $id(&self, $($arg: $arg_type),+) -> $crate::anyhow::Result<Vec<$return_type>> {
            use $crate::anyhow::Context;

            self.write(move |connection| {
                let sql_stmt = $crate::sql_macros::sql!($($sql)+);

                connection.select_bound::<($($arg_type),+), $return_type>(sql_stmt)?(($($arg),+))
                    .context(::std::format!(
                        "Error in {}, select_bound failed to execute or parse for: {}",
                        stringify!($id),
                        sql_stmt,
                    ))
            })
            .await
        }
    };
    ($vis:vis fn $id:ident() -> anyhow::Result<Option<$return_type:ty>> { $($sql:tt)+ }) => {
        $vis fn $id(&self) -> $crate::anyhow::Result<Option<$return_type>> {
            use $crate::anyhow::Context;

            self.read(|connection| {
                let sql_stmt = $crate::sql_macros::sql!($($sql)+);

                connection.select_row::<$return_type>(sql_stmt)?()
                    .context(::std::format!(
                        "Error in {}, select_row failed to execute or parse for: {}",
                        stringify!($id),
                        sql_stmt,
                    ))
            })
        }
    };
    ($vis:vis async fn $id:ident() -> anyhow::Result<Option<$return_type:ty>> { $($sql:tt)+ }) => {
        $vis async fn $id(&self) -> $crate::anyhow::Result<Option<$return_type>> {
            use $crate::anyhow::Context;

            self.write(|connection| {
                let sql_stmt = $crate::sql_macros::sql!($($sql)+);

                connection.select_row::<$return_type>(sql_stmt)?()
                    .context(::std::format!(
                        "Error in {}, select_row failed to execute or parse for: {}",
                        stringify!($id),
                        sql_stmt,
                    ))
            })
            .await
        }
    };
    ($vis:vis fn $id:ident($arg:ident: $arg_type:ty $(,)?) -> anyhow::Result<Option<$return_type:ty>> { $($sql:tt)+ }) => {
        $vis fn $id(&self, $arg: $arg_type) -> $crate::anyhow::Result<Option<$return_type>> {
            use $crate::anyhow::Context;

            self.read(|connection| {
                let sql_stmt = $crate::sql_macros::sql!($($sql)+);

                connection.select_row_bound::<$arg_type, $return_type>(sql_stmt)?($arg)
                    .context(::std::format!(
                        "Error in {}, select_row_bound failed to execute or parse for: {}",
                        stringify!($id),
                        sql_stmt,
                    ))
            })
        }
    };
    ($vis:vis fn $id:ident($($arg:ident: $arg_type:ty),+ $(,)?) -> anyhow::Result<Option<$return_type:ty>> { $($sql:tt)+ }) => {
        $vis fn $id(&self, $($arg: $arg_type),+) -> $crate::anyhow::Result<Option<$return_type>> {
            use $crate::anyhow::Context;

            self.read(|connection| {
                let sql_stmt = $crate::sql_macros::sql!($($sql)+);

                connection.select_row_bound::<($($arg_type),+), $return_type>(sql_stmt)?(($($arg),+))
                    .context(::std::format!(
                        "Error in {}, select_row_bound failed to execute or parse for: {}",
                        stringify!($id),
                        sql_stmt,
                    ))
            })
        }
    };
    ($vis:vis async fn $id:ident($($arg:ident: $arg_type:ty),+ $(,)?) -> anyhow::Result<Option<$return_type:ty>> { $($sql:tt)+ }) => {
        $vis async fn $id(&self, $($arg: $arg_type),+) -> $crate::anyhow::Result<Option<$return_type>> {
            use $crate::anyhow::Context;

            self.write(move |connection| {
                let sql_stmt = $crate::sql_macros::sql!($($sql)+);

                connection.select_row_bound::<($($arg_type),+), $return_type>(sql_stmt)?(($($arg),+))
                    .context(::std::format!(
                        "Error in {}, select_row_bound failed to execute or parse for: {}",
                        stringify!($id),
                        sql_stmt,
                    ))
            })
            .await
        }
    };
    ($vis:vis fn $id:ident() -> anyhow::Result<$return_type:ty> { $($sql:tt)+ }) => {
        $vis fn $id(&self) -> $crate::anyhow::Result<$return_type> {
            use $crate::anyhow::Context;

            self.read(|connection| {
                let sql_stmt = $crate::sql_macros::sql!($($sql)+);

                connection.select_row::<$return_type>(sql_stmt)?()
                    .context(::std::format!(
                        "Error in {}, select_row failed to execute or parse for: {}",
                        stringify!($id),
                        sql_stmt,
                    ))?
                    .context(::std::format!(
                        "Error in {}, select_row expected single row result but found none for: {}",
                        stringify!($id),
                        sql_stmt,
                    ))
            })
        }
    };
    ($vis:vis async fn $id:ident() -> anyhow::Result<$return_type:ty> { $($sql:tt)+ }) => {
        $vis async fn $id(&self) -> $crate::anyhow::Result<$return_type> {
            use $crate::anyhow::Context;

            self.write(|connection| {
                let sql_stmt = $crate::sql_macros::sql!($($sql)+);

                connection.select_row::<$return_type>(sql_stmt)?()
                    .context(::std::format!(
                        "Error in {}, select_row failed to execute or parse for: {}",
                        stringify!($id),
                        sql_stmt,
                    ))?
                    .context(::std::format!(
                        "Error in {}, select_row expected single row result but found none for: {}",
                        stringify!($id),
                        sql_stmt,
                    ))
            })
            .await
        }
    };
    ($vis:vis fn $id:ident($arg:ident: $arg_type:ty $(,)?) -> anyhow::Result<$return_type:ty> { $($sql:tt)+ }) => {
        $vis fn $id(&self, $arg: $arg_type) -> $crate::anyhow::Result<$return_type> {
            use $crate::anyhow::Context;

            self.read(|connection| {
                let sql_stmt = $crate::sql_macros::sql!($($sql)+);

                connection.select_row_bound::<$arg_type, $return_type>(sql_stmt)?($arg)
                    .context(::std::format!(
                        "Error in {}, select_row_bound failed to execute or parse for: {}",
                        stringify!($id),
                        sql_stmt,
                    ))?
                    .context(::std::format!(
                        "Error in {}, select_row_bound expected single row result but found none for: {}",
                        stringify!($id),
                        sql_stmt,
                    ))
            })
        }
    };
    ($vis:vis fn $id:ident($($arg:ident: $arg_type:ty),+ $(,)?) -> anyhow::Result<$return_type:ty> { $($sql:tt)+ }) => {
        $vis fn $id(&self, $($arg: $arg_type),+) -> $crate::anyhow::Result<$return_type> {
            use $crate::anyhow::Context;

            self.read(|connection| {
                let sql_stmt = $crate::sql_macros::sql!($($sql)+);

                connection.select_row_bound::<($($arg_type),+), $return_type>(sql_stmt)?(($($arg),+))
                    .context(::std::format!(
                        "Error in {}, select_row_bound failed to execute or parse for: {}",
                        stringify!($id),
                        sql_stmt,
                    ))?
                    .context(::std::format!(
                        "Error in {}, select_row_bound expected single row result but found none for: {}",
                        stringify!($id),
                        sql_stmt,
                    ))
            })
        }
    };
    ($vis:vis async fn $id:ident($($arg:ident: $arg_type:ty),+ $(,)?) -> anyhow::Result<$return_type:ty> { $($sql:tt)+ }) => {
        $vis async fn $id(&self, $($arg: $arg_type),+) -> $crate::anyhow::Result<$return_type> {
            use $crate::anyhow::Context;

            self.write(move |connection| {
                let sql_stmt = $crate::sql_macros::sql!($($sql)+);

                connection.select_row_bound::<($($arg_type),+), $return_type>(sql_stmt)?(($($arg),+))
                    .context(::std::format!(
                        "Error in {}, select_row_bound failed to execute or parse for: {}",
                        stringify!($id),
                        sql_stmt,
                    ))?
                    .context(::std::format!(
                        "Error in {}, select_row_bound expected single row result but found none for: {}",
                        stringify!($id),
                        sql_stmt,
                    ))
            })
            .await
        }
    };
}
