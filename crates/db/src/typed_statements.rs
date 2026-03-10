use anyhow::Context;

use crate::{
    bindable::{Bind, Column},
    connection::Connection,
    statement::Statement,
};

impl Connection {
    pub fn exec<'a>(
        &'a self,
        query: &str,
    ) -> anyhow::Result<impl 'a + FnMut() -> anyhow::Result<()>> {
        let mut statement = Statement::prepare(self, query)?;
        Ok(move || statement.exec())
    }

    pub fn exec_bound<'a, B: Bind>(
        &'a self,
        query: &str,
    ) -> anyhow::Result<impl 'a + FnMut(B) -> anyhow::Result<()>> {
        let mut statement = Statement::prepare(self, query)?;
        Ok(move |bindings| statement.with_bindings(&bindings)?.exec())
    }

    pub fn select<'a, C: Column>(
        &'a self,
        query: &str,
    ) -> anyhow::Result<impl 'a + FnMut() -> anyhow::Result<Vec<C>>> {
        let mut statement = Statement::prepare(self, query)?;
        Ok(move || statement.rows::<C>())
    }

    pub fn select_bound<'a, B: Bind, C: Column>(
        &'a self,
        query: &str,
    ) -> anyhow::Result<impl 'a + FnMut(B) -> anyhow::Result<Vec<C>>> {
        let mut statement = Statement::prepare(self, query)?;
        Ok(move |bindings| statement.with_bindings(&bindings)?.rows::<C>())
    }

    pub fn select_row<'a, C: Column>(
        &'a self,
        query: &str,
    ) -> anyhow::Result<impl 'a + FnMut() -> anyhow::Result<Option<C>>> {
        let mut statement = Statement::prepare(self, query)?;
        Ok(move || statement.maybe_row::<C>())
    }

    pub fn select_row_bound<'a, B: Bind, C: Column>(
        &'a self,
        query: &str,
    ) -> anyhow::Result<impl 'a + FnMut(B) -> anyhow::Result<Option<C>>> {
        let mut statement = Statement::prepare(self, query)?;
        Ok(move |bindings| {
            statement
                .with_bindings(&bindings)
                .context("bindings failed")?
                .maybe_row::<C>()
                .context("maybe row failed")
        })
    }
}
