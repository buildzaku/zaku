use futures::future;
use gpui::{AsyncWindowContext, Entity, WeakEntity, WindowId};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};
use uuid::Uuid;

use db::{Bind, Column, Row, Statement, StaticColumnCount};
use project::Project;
use util::ResultExt;

use super::SerializedWindowBounds;
use crate::{ItemHandle, SerializableItemRegistry, Workspace, WorkspaceId, pane::Pane};

#[derive(Debug, Clone, PartialEq)]
pub struct SerializedWorkspace {
    pub id: WorkspaceId,
    pub location: PathBuf,
    pub center_pane: SerializedPane,
    pub docks: DockStructure,
    pub window_bounds: Option<SerializedWindowBounds>,
    pub display: Option<Uuid>,
    pub session_id: Option<String>,
    pub window_id: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DockStructure {
    pub left: DockData,
    pub bottom: DockData,
}

impl Bind for DockStructure {
    fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
        let next_index = statement.bind(&self.left, start_index)?;
        statement.bind(&self.bottom, next_index)
    }
}

impl Column for DockStructure {
    fn column(row: &mut Row<'_, '_>, start_index: i32) -> anyhow::Result<(Self, i32)> {
        let (left, next_index) = DockData::column(row, start_index)?;
        let (bottom, next_index) = DockData::column(row, next_index)?;
        Ok((DockStructure { left, bottom }, next_index))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DockData {
    pub visible: bool,
    pub active_panel: Option<String>,
}

impl Bind for DockData {
    fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
        let next_index = statement.bind(&self.visible, start_index)?;
        statement.bind(&self.active_panel, next_index)
    }
}

impl Column for DockData {
    fn column(row: &mut Row<'_, '_>, start_index: i32) -> anyhow::Result<(Self, i32)> {
        let (visible, next_index) = Option::<bool>::column(row, start_index)?;
        let (active_panel, next_index) = Option::<String>::column(row, next_index)?;
        Ok((
            DockData {
                visible: visible.unwrap_or(false),
                active_panel,
            },
            next_index,
        ))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionWorkspace {
    pub location: PathBuf,
    pub window_id: Option<WindowId>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SerializedPane {
    pub(crate) active: bool,
    pub(crate) children: Vec<SerializedItem>,
}

impl SerializedPane {
    pub fn new(children: Vec<SerializedItem>, active: bool) -> Self {
        SerializedPane { active, children }
    }

    pub async fn deserialize_to(
        &self,
        project: &Entity<Project>,
        pane: &WeakEntity<Pane>,
        workspace_id: WorkspaceId,
        workspace: WeakEntity<Workspace>,
        cx: &mut AsyncWindowContext,
    ) -> anyhow::Result<Vec<Option<Box<dyn ItemHandle>>>> {
        let mut item_tasks = Vec::new();
        let mut active_item_index = None;
        let mut preview_item_index = None;

        for (index, item) in self.children.iter().enumerate() {
            let project = project.clone();
            item_tasks.push(pane.update_in(cx, |_, window, cx| {
                SerializableItemRegistry::deserialize(
                    &item.kind,
                    project,
                    workspace.clone(),
                    workspace_id,
                    item.item_id,
                    window,
                    cx,
                )
            })?);

            if item.active {
                active_item_index = Some(index);
            }

            if item.preview {
                preview_item_index = Some(index);
            }
        }

        let mut items = Vec::new();
        for item_handle in future::join_all(item_tasks).await {
            let item_handle = item_handle.log_err();
            items.push(item_handle.clone());

            if let Some(item_handle) = item_handle {
                pane.update_in(cx, |pane, window, cx| {
                    pane.add_item(item_handle, true, true, true, None, window, cx);
                })?;
            }
        }

        if let Some(active_item_index) = active_item_index {
            pane.update_in(cx, |pane, window, cx| {
                pane.activate_item(active_item_index, false, false, window, cx);
            })?;
        }

        if let Some(preview_item_index) = preview_item_index {
            pane.update(cx, |pane, cx| {
                if let Some(item) = pane.item_for_index(preview_item_index) {
                    pane.set_preview_item_id(Some(item.item_id()), cx);
                }
            })?;
        }

        anyhow::Ok(items)
    }
}

pub(crate) type PaneId = i64;
pub type ItemId = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
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

impl Bind for &SerializedItem {
    fn bind(&self, statement: &Statement<'_>, start_index: i32) -> anyhow::Result<i32> {
        let next_index = statement.bind(&self.kind, start_index)?;
        let next_index = statement.bind(&self.item_id, next_index)?;
        let next_index = statement.bind(&self.active, next_index)?;
        statement.bind(&self.preview, next_index)
    }
}
