use futures::future;
use gpui::{AsyncWindowContext, Entity, WeakEntity, WindowId};
use std::{path::PathBuf, sync::Arc};
use uuid::Uuid;

use db::{Bind, Column, Row, Statement, StaticColumnCount};
use project::Project;
use util::ResultExt;

use super::SerializedWindowBounds;
use crate::{ItemHandle, SerializableItemRegistry, Workspace, WorkspaceId, pane::Pane};

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
        let mut active_item = None;
        let mut preview_item = None;
        for (index, item_handle) in future::join_all(item_tasks).await.into_iter().enumerate() {
            let item_handle = item_handle.log_err();

            if let Some(item_handle) = item_handle.clone() {
                if Some(index) == active_item_index {
                    active_item = Some(item_handle.clone());
                }

                if Some(index) == preview_item_index {
                    preview_item = Some(item_handle.clone());
                }

                pane.update_in(cx, |pane, window, cx| {
                    pane.add_item(item_handle, true, false, false, Some(index), window, cx);
                })?;
            }

            items.push(item_handle);
        }

        if let Some(active_item) = active_item {
            pane.update_in(cx, |pane, window, cx| {
                if let Some(active_item_index) = pane.index_for_item(active_item.as_ref()) {
                    pane.activate_item(active_item_index, false, false, window, cx);
                }
            })?;
        }

        if let Some(preview_item) = preview_item {
            pane.update(cx, |pane, cx| {
                if pane.index_for_item(preview_item.as_ref()).is_some() {
                    pane.set_preview_item_id(Some(preview_item.item_id()), cx);
                }
            })?;
        }

        anyhow::Ok(items)
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
