use gpui::WindowId;
use std::{path::PathBuf, sync::Arc};
use uuid::Uuid;

use db::{Bind, Column, Row, Statement, StaticColumnCount};

use super::SerializedWindowBounds;
use crate::WorkspaceId;

#[derive(Clone, Debug, PartialEq)]
pub struct SerializedWorkspace {
    pub id: WorkspaceId,
    pub location: PathBuf,
    pub center_pane: SerializedPane,
    pub window_bounds: Option<SerializedWindowBounds>,
    pub display: Option<Uuid>,
    pub session_id: Option<String>,
    pub window_id: Option<u64>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct SessionWorkspace {
    pub location: PathBuf,
    pub window_id: Option<WindowId>,
}

#[derive(Debug, PartialEq, Eq, Default, Clone)]
pub struct SerializedPane {
    pub(crate) active: bool,
    pub(crate) children: Vec<SerializedItem>,
}

impl SerializedPane {
    pub fn new(children: Vec<SerializedItem>, active: bool) -> Self {
        SerializedPane { active, children }
    }
}

pub type PaneId = i64;
pub type ItemId = u64;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct SerializedItem {
    pub kind: Arc<str>,
    pub item_id: ItemId,
    pub active: bool,
    pub preview: bool,
}

impl SerializedItem {
    pub fn new(kind: impl AsRef<str>, item_id: ItemId, active: bool, preview: bool) -> Self {
        Self {
            kind: Arc::from(kind.as_ref()),
            item_id,
            active,
            preview,
        }
    }
}

impl StaticColumnCount for SerializedItem {
    fn column_count() -> usize {
        4
    }
}

impl Bind for &SerializedItem {
    fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
        let next_index = statement.bind(&self.kind, start_index)?;
        let next_index = statement.bind(&self.item_id, next_index)?;
        let next_index = statement.bind(&self.active, next_index)?;
        statement.bind(&self.preview, next_index)
    }
}

impl Column for SerializedItem {
    fn column(row: &mut Row<'_, '_>, start_index: i32) -> anyhow::Result<(Self, i32)> {
        let (kind, next_index) = Arc::<str>::column(row, start_index)?;
        let (item_id, next_index) = ItemId::column(row, next_index)?;
        let (active, next_index) = bool::column(row, next_index)?;
        let (preview, next_index) = bool::column(row, next_index)?;
        Ok((
            SerializedItem {
                kind,
                item_id,
                active,
                preview,
            },
            next_index,
        ))
    }
}
