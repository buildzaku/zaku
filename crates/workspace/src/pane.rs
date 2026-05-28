use gpui::{
    AnyElement, App, AsyncWindowContext, ClickEvent, Context, Empty, Entity, EntityId,
    EventEmitter, FocusHandle, FocusOutEvent, Focusable, PromptLevel, ScrollHandle, Subscription,
    Task, WeakEntity, Window, prelude::*,
};
use itertools::Itertools;
use std::{
    collections::{BTreeSet, HashSet},
    mem,
};

use project::{Project, ProjectEntryId, ProjectPath};
use theme::{ActiveTheme, ThemeSettings};
use ui::{
    ButtonCommon, ButtonSize, Clickable, Color, IconButton, IconButtonShape, IconName, IconSize,
    Tab, TabBar, TabPosition, Toggleable, Tooltip, VisibleOnHover,
};
use util::{ResultExt, path::PathStyle};

use crate::{
    ItemBufferKind, ItemEvent, ItemHandle, TabContentParams, TabTooltipContent, WeakItemHandle,
    Workspace, WorkspaceItemBuilder, welcome::WelcomePage,
};

pub enum PaneEvent {
    AddItem {
        item: Box<dyn ItemHandle>,
    },
    ActivateItem {
        focus_changed: bool,
    },
    RemovedItem {
        item: Box<dyn ItemHandle>,
    },
    ChangeItemTitle,
    UserSavedItem {
        item: Box<dyn WeakItemHandle>,
        save_intent: actions::pane::SaveIntent,
    },
}

impl EventEmitter<PaneEvent> for Pane {}

#[derive(Clone)]
pub struct DraggedTab {
    pub pane: Entity<Pane>,
    pub item: Box<dyn ItemHandle>,
    pub index: usize,
    pub detail: usize,
    pub is_active: bool,
}

pub struct Pane {
    focus_handle: FocusHandle,
    was_focused: bool,
    should_display_welcome_page: bool,
    welcome_page: Option<Entity<WelcomePage>>,
    workspace: WeakEntity<Workspace>,
    project: WeakEntity<Project>,
    items: Vec<Box<dyn ItemHandle>>,
    active_item_index: usize,
    preview_item_id: Option<EntityId>,
    tab_bar_scroll_handle: ScrollHandle,
    item_subscriptions: Vec<(EntityId, Subscription)>,
    save_modals_spawned: HashSet<EntityId>,
    _focus_subscriptions: Vec<Subscription>,
}

impl Pane {
    pub fn new(
        workspace: WeakEntity<Workspace>,
        project: &Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let focus_subscriptions = vec![
            cx.on_focus(&focus_handle, window, Pane::focus_in),
            cx.on_focus_in(&focus_handle, window, Pane::focus_in),
            cx.on_focus_out(&focus_handle, window, Pane::focus_out),
        ];

        Self {
            focus_handle,
            was_focused: false,
            should_display_welcome_page: false,
            welcome_page: None,
            workspace,
            project: project.downgrade(),
            items: vec![],
            active_item_index: 0,
            preview_item_id: None,
            tab_bar_scroll_handle: ScrollHandle::new(),
            item_subscriptions: Vec::new(),
            save_modals_spawned: HashSet::default(),
            _focus_subscriptions: focus_subscriptions,
        }
    }

    pub fn workspace(&self) -> WeakEntity<Workspace> {
        self.workspace.clone()
    }

    pub fn has_focus(&self, window: &Window, cx: &App) -> bool {
        self.focus_handle.contains_focused(window, cx)
            || self
                .active_item()
                .is_some_and(|item| item.item_focus_handle(cx).contains_focused(window, cx))
    }

    pub fn items(&self) -> impl DoubleEndedIterator<Item = &Box<dyn ItemHandle>> {
        self.items.iter()
    }

    pub fn items_len(&self) -> usize {
        self.items.len()
    }

    pub fn active_item(&self) -> Option<Box<dyn ItemHandle>> {
        self.items.get(self.active_item_index).cloned()
    }

    fn active_item_id(&self) -> Option<EntityId> {
        self.active_item().map(|item| item.item_id())
    }

    pub fn active_item_index(&self) -> usize {
        self.active_item_index
    }

    pub(crate) fn set_preview_item_id(&mut self, preview_item_id: Option<EntityId>, _cx: &App) {
        self.preview_item_id = preview_item_id;
    }

    pub fn preview_item(&self) -> Option<Box<dyn ItemHandle>> {
        self.preview_item_id
            .and_then(|id| self.items.iter().find(|item| item.item_id() == id))
            .cloned()
    }

    pub fn preview_item_idx(&self) -> Option<usize> {
        if let Some(preview_item_id) = self.preview_item_id {
            self.items
                .iter()
                .position(|item| item.item_id() == preview_item_id)
        } else {
            None
        }
    }

    pub fn is_active_preview_item(&self, item_id: EntityId) -> bool {
        self.preview_item_id == Some(item_id)
    }

    pub fn unpreview_item_if_preview(&mut self, item_id: EntityId) {
        if self.is_active_preview_item(item_id) {
            self.preview_item_id = None;
        }
    }

    pub fn handle_item_edit(&mut self, item_id: EntityId, cx: &App) {
        if let Some(preview_item) = self.preview_item()
            && preview_item.item_id() == item_id
            && !preview_item.preserve_preview(cx)
        {
            self.unpreview_item_if_preview(item_id);
        }
    }

    pub fn item_for_entry(
        &self,
        entry_id: ProjectEntryId,
        cx: &App,
    ) -> Option<Box<dyn ItemHandle>> {
        self.items.iter().find_map(|item| {
            if item.buffer_kind(cx) == ItemBufferKind::Singleton
                && item.project_entry_ids(cx).as_slice() == [entry_id]
            {
                Some(item.boxed_clone())
            } else {
                None
            }
        })
    }

    pub fn item_for_path(
        &self,
        project_path: ProjectPath,
        cx: &App,
    ) -> Option<Box<dyn ItemHandle>> {
        self.items.iter().find_map(move |item| {
            if item.buffer_kind(cx) == ItemBufferKind::Singleton
                && item.project_path(cx).as_slice() == [project_path.clone()]
            {
                Some(item.boxed_clone())
            } else {
                None
            }
        })
    }

    pub fn add_item(
        &mut self,
        item: Box<dyn ItemHandle>,
        activate_pane: bool,
        focus_item: bool,
        activate: bool,
        destination_index: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let insertion_index = destination_index
            .unwrap_or(self.active_item_index + 1)
            .min(self.items.len());

        let project_entry_id = if item.buffer_kind(cx) == ItemBufferKind::Singleton {
            item.project_entry_ids(cx).first().copied()
        } else {
            None
        };
        let existing_item_index = self.items.iter().position(|existing_item| {
            if existing_item.item_id() == item.item_id() {
                true
            } else if existing_item.buffer_kind(cx) == ItemBufferKind::Singleton {
                existing_item
                    .project_entry_ids(cx)
                    .first()
                    .is_some_and(|existing_entry_id| {
                        Some(existing_entry_id) == project_entry_id.as_ref()
                    })
            } else {
                false
            }
        });

        if let Some(existing_item_index) = existing_item_index {
            let mut insertion_index = insertion_index;

            if existing_item_index != insertion_index {
                let existing_item_is_active = existing_item_index == self.active_item_index;

                if existing_item_is_active && destination_index.is_none() {
                    insertion_index = existing_item_index;
                } else {
                    self.items.remove(existing_item_index);

                    if existing_item_index < self.active_item_index {
                        self.active_item_index -= 1;
                    }

                    insertion_index = insertion_index.min(self.items.len());
                    self.items.insert(insertion_index, item.clone());

                    if existing_item_is_active {
                        self.active_item_index = insertion_index;
                    } else if insertion_index <= self.active_item_index {
                        self.active_item_index += 1;
                    }

                    cx.notify();
                }
            }

            if activate {
                self.activate_item(insertion_index, activate_pane, focus_item, window, cx);
            }
        } else {
            let item_id = item.item_id();
            let pane = cx.weak_entity();
            let subscription = item.subscribe_to_item_events(
                window,
                cx,
                Box::new(move |event, window, cx| {
                    if let Err(error) = pane.update(cx, |pane, cx| {
                        pane.handle_item_event(item_id, event, window, cx);
                    }) {
                        log::debug!("Failed to handle pane item event: {error:?}");
                    }
                }),
            );

            self.items.insert(insertion_index, item.clone());
            self.item_subscriptions.push((item_id, subscription));
            cx.notify();

            if activate {
                if insertion_index <= self.active_item_index
                    && self.preview_item_idx() != Some(self.active_item_index)
                {
                    self.active_item_index += 1;
                }

                self.activate_item(insertion_index, activate_pane, focus_item, window, cx);
            } else if insertion_index <= self.active_item_index && self.items.len() > 1 {
                self.active_item_index += 1;
                cx.notify();
            }
        }

        cx.emit(PaneEvent::AddItem { item });
    }

    pub(crate) fn open_item(
        &mut self,
        project_entry_id: Option<ProjectEntryId>,
        project_path: &ProjectPath,
        focus_item: bool,
        allow_preview: bool,
        activate: bool,
        suggested_position: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
        build_item: WorkspaceItemBuilder,
    ) -> Box<dyn ItemHandle> {
        let mut existing_item = None;
        if let Some(project_entry_id) = project_entry_id {
            for (index, item) in self.items.iter().enumerate() {
                if item.buffer_kind(cx) == ItemBufferKind::Singleton
                    && item.project_entry_ids(cx).as_slice() == [project_entry_id]
                {
                    existing_item = Some((index, item.boxed_clone()));
                    break;
                }
            }
        } else {
            for (index, item) in self.items.iter().enumerate() {
                if item.buffer_kind(cx) == ItemBufferKind::Singleton
                    && item.project_path(cx).as_ref() == Some(project_path)
                {
                    existing_item = Some((index, item.boxed_clone()));
                    break;
                }
            }
        }

        let preview_was_active = self.preview_item_idx() == Some(self.active_item_index);

        let set_up_existing_item =
            |index: usize, pane: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
                if !allow_preview && let Some(item) = pane.items.get(index) {
                    pane.unpreview_item_if_preview(item.item_id());
                }

                if activate {
                    pane.activate_item(index, focus_item, focus_item, window, cx);
                }
            };
        let set_up_new_item = |new_item: Box<dyn ItemHandle>,
                               destination_index: Option<usize>,
                               pane: &mut Self,
                               window: &mut Window,
                               cx: &mut Context<Self>| {
            let new_item_id = new_item.item_id();

            if allow_preview && preview_was_active {
                pane.set_preview_item_id(Some(new_item_id), cx);
            }

            pane.add_item(
                new_item,
                true,
                focus_item,
                activate,
                destination_index,
                window,
                cx,
            );

            if allow_preview && !preview_was_active {
                pane.set_preview_item_id(Some(new_item_id), cx);
            }
        };

        if let Some((index, existing_item)) = existing_item {
            set_up_existing_item(index, self, window, cx);
            existing_item
        } else {
            let destination_index = if allow_preview {
                self.close_current_preview_item(window, cx)
            } else {
                suggested_position
            };
            let new_item = build_item(self, window, cx);
            set_up_new_item(new_item.clone(), destination_index, self, window, cx);
            new_item
        }
    }

    pub fn remove_item(
        &mut self,
        item_id: EntityId,
        activate: bool,
        focus_item: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Box<dyn ItemHandle>> {
        let item_index = self.index_for_item_id(item_id)?;
        let was_active = item_index == self.active_item_index;
        let should_focus_pane =
            was_active && self.items.len() == 1 && (focus_item || self.has_focus(window, cx));
        let item = self.items.remove(item_index);
        cx.emit(PaneEvent::RemovedItem { item: item.clone() });

        if should_focus_pane {
            self.focus_handle.focus(window, cx);
        }

        self.item_subscriptions
            .retain(|(subscription_item_id, _)| *subscription_item_id != item_id);

        item.deactivated(window, cx);
        item.on_removed(cx);

        if self.is_active_preview_item(item_id) {
            self.preview_item_id = None;
        }

        if self.items.is_empty() {
            self.active_item_index = 0;
        } else if item_index < self.active_item_index {
            self.active_item_index -= 1;
        } else if self.active_item_index >= self.items.len() {
            self.active_item_index = self.items.len() - 1;
        }

        if activate && was_active && !self.items.is_empty() {
            self.activate_item(self.active_item_index, false, focus_item, window, cx);
        } else {
            cx.notify();
        }

        Some(item)
    }

    pub fn close_current_preview_item(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let item_index = self.preview_item_idx()?;
        let item_id = self.preview_item_id?;

        self.preview_item_id = None;
        let previous_active_item_index = self.active_item_index;
        self.remove_item(item_id, false, false, window, cx);
        self.active_item_index = previous_active_item_index;

        if item_index < previous_active_item_index {
            self.active_item_index -= 1;
        }

        if item_index < self.items.len() {
            Some(item_index)
        } else {
            None
        }
    }

    pub fn activate_item(
        &mut self,
        item_index: usize,
        _activate_pane: bool,
        focus_item: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if item_index >= self.items.len() {
            return;
        }

        let previous_active_item_index = mem::replace(&mut self.active_item_index, item_index);
        if previous_active_item_index != self.active_item_index
            && let Some(previous_item) = self.items.get(previous_active_item_index)
        {
            previous_item.deactivated(window, cx);
        }

        if focus_item {
            self.focus_active_item(window, cx);
        }

        cx.emit(PaneEvent::ActivateItem {
            focus_changed: focus_item,
        });
        self.update_active_tab(item_index);
        cx.notify();
    }

    fn update_active_tab(&mut self, item_index: usize) {
        self.tab_bar_scroll_handle.scroll_to_item(item_index);
    }

    fn focus_active_item(&self, window: &mut Window, cx: &mut App) {
        if let Some(active_item) = self.active_item() {
            active_item.item_focus_handle(cx).focus(window, cx);
        }
    }

    pub fn index_for_item(&self, item: &dyn ItemHandle) -> Option<usize> {
        self.index_for_item_id(item.item_id())
    }

    fn index_for_item_id(&self, item_id: EntityId) -> Option<usize> {
        self.items.iter().position(|item| item.item_id() == item_id)
    }

    pub fn item_for_index(&self, index: usize) -> Option<&dyn ItemHandle> {
        self.items.get(index).map(|item| item.as_ref())
    }

    pub fn close_item_by_id(
        &mut self,
        item_id_to_close: EntityId,
        save_intent: actions::pane::SaveIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        self.close_items(window, cx, save_intent, &move |item_id| {
            item_id == item_id_to_close
        })
    }

    pub fn close_active_item(
        &mut self,
        action: &actions::pane::CloseActiveItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        let Some(active_item_id) = self.active_item_id() else {
            return Task::ready(Ok(()));
        };

        self.close_item_by_id(
            active_item_id,
            action
                .save_intent
                .unwrap_or(actions::pane::SaveIntent::Close),
            window,
            cx,
        )
    }

    pub fn close_items(
        &self,
        window: &mut Window,
        cx: &mut Context<Pane>,
        mut save_intent: actions::pane::SaveIntent,
        should_close: &dyn Fn(EntityId) -> bool,
    ) -> Task<anyhow::Result<()>> {
        let mut items_to_close = Vec::new();
        for item in &self.items {
            if should_close(item.item_id()) {
                items_to_close.push(item.boxed_clone());
            }
        }

        let active_item_id = self.active_item_id();

        items_to_close.sort_by_key(|item| {
            let path = item.project_path(cx);
            (active_item_id == Some(item.item_id()), path.is_none(), path)
        });

        let Some(project) = self.project.upgrade() else {
            return Task::ready(Ok(()));
        };

        cx.spawn_in(window, async move |pane, cx| {
            let dirty_items = cx.update(|_, cx| {
                items_to_close
                    .iter()
                    .filter(|item| item.is_dirty(cx))
                    .map(|item| item.boxed_clone())
                    .collect::<Vec<_>>()
            })?;

            if save_intent == actions::pane::SaveIntent::Close && dirty_items.len() > 1 {
                let answer = pane.update_in(cx, |_, window, cx| {
                    let detail = Self::file_names_for_prompt(&mut dirty_items.iter(), cx);
                    window.prompt(
                        PromptLevel::Warning,
                        "Do you want to save changes to the following files?",
                        Some(&detail),
                        &["Save all", "Discard all", "Cancel"],
                        cx,
                    )
                })?;
                match answer.await {
                    Ok(0) => save_intent = actions::pane::SaveIntent::SaveAll,
                    Ok(1) => save_intent = actions::pane::SaveIntent::Skip,
                    Ok(2) => return Ok(()),
                    _ => {}
                }
            }

            for item_to_close in items_to_close {
                let mut should_close = true;
                match Self::save_item(
                    project.clone(),
                    &pane,
                    item_to_close.as_ref(),
                    save_intent,
                    cx,
                )
                .await
                {
                    Ok(success) => {
                        if !success {
                            should_close = false;
                        }
                    }
                    Err(error) => {
                        let answer = pane.update_in(cx, |_, window, cx| {
                            let detail =
                                Self::file_names_for_prompt(&mut [&item_to_close].into_iter(), cx);
                            window.prompt(
                                PromptLevel::Warning,
                                &format!("Unable to save file: {error}"),
                                Some(&detail),
                                &["Close Without Saving", "Cancel"],
                                cx,
                            )
                        })?;
                        match answer.await {
                            Ok(0) => {}
                            Ok(1..) | Err(_) => should_close = false,
                        }
                    }
                }

                if should_close {
                    pane.update_in(cx, |pane, window, cx| {
                        pane.remove_item(item_to_close.item_id(), true, true, window, cx);
                    })?;
                }
            }

            pane.update(cx, |_, cx| cx.notify())?;
            Ok(())
        })
    }

    pub fn close_all_items(
        &mut self,
        action: &actions::pane::CloseAllItems,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        if self.items.is_empty() {
            return Task::ready(Ok(()));
        }

        self.close_items(
            window,
            cx,
            action
                .save_intent
                .unwrap_or(actions::pane::SaveIntent::Close),
            &|_| true,
        )
    }

    fn file_names_for_prompt(
        items: &mut dyn Iterator<Item = &Box<dyn ItemHandle>>,
        cx: &App,
    ) -> String {
        let mut file_names = BTreeSet::default();
        for item in items {
            item.for_each_project_item(cx, &mut |_, project_item| {
                if !project_item.is_dirty() {
                    return;
                }
                let file_name = project_item
                    .project_path(cx)
                    .and_then(|path| path.path.file_name().map(ToOwned::to_owned));
                file_names.insert(file_name.unwrap_or("untitled".to_string()));
            });
        }
        if file_names.len() > 6 {
            format!(
                "{}\n.. and {} more",
                file_names.iter().take(5).join("\n"),
                file_names.len() - 5
            )
        } else {
            file_names.into_iter().join("\n")
        }
    }

    pub fn handle_deleted_project_item(
        &mut self,
        entry_id: ProjectEntryId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<()> {
        let item_id = self.items().find_map(|item| {
            if item.buffer_kind(cx) == ItemBufferKind::Singleton
                && item.project_entry_ids(cx).as_slice() == [entry_id]
            {
                Some(item.item_id())
            } else {
                None
            }
        })?;

        self.remove_item(item_id, false, true, window, cx);

        Some(())
    }

    pub async fn save_item(
        project: Entity<Project>,
        pane: &WeakEntity<Pane>,
        item: &dyn ItemHandle,
        save_intent: actions::pane::SaveIntent,
        cx: &mut AsyncWindowContext,
    ) -> anyhow::Result<bool> {
        if save_intent == actions::pane::SaveIntent::Skip {
            let is_saveable_singleton = cx.update(|_, cx| {
                item.can_save(cx) && item.buffer_kind(cx) == ItemBufferKind::Singleton
            })?;
            if is_saveable_singleton {
                pane.update_in(cx, |_, window, cx| item.reload(project, window, cx))?
                    .await
                    .log_err();
            }
            return Ok(true);
        }

        let path_style = project.read_with(cx, |project, cx| project.path_style(cx));
        let Some(item_index) = pane
            .read_with(cx, |pane, _| pane.index_for_item(item))
            .ok()
            .flatten()
        else {
            return Ok(true);
        };

        let (mut is_dirty, can_save, is_singleton) = cx.update(|_, cx| {
            (
                item.is_dirty(cx),
                item.can_save(cx),
                item.buffer_kind(cx) == ItemBufferKind::Singleton,
            )
        })?;

        if save_intent == actions::pane::SaveIntent::Save {
            is_dirty = true;
        }

        if is_dirty && can_save {
            if save_intent == actions::pane::SaveIntent::Close {
                let item_id = item.item_id();
                let answer_task = pane.update_in(cx, |pane, window, cx| {
                    if pane.save_modals_spawned.insert(item_id) {
                        pane.activate_item(item_index, true, true, window, cx);
                        let prompt = dirty_message_for(item.project_path(cx), path_style);
                        Some(window.prompt(
                            PromptLevel::Warning,
                            &prompt,
                            None,
                            &["Save", "Don't Save", "Cancel"],
                            cx,
                        ))
                    } else {
                        None
                    }
                })?;
                let Some(answer_task) = answer_task else {
                    return Ok(false);
                };
                let answer = answer_task.await;
                pane.update(cx, |pane, _| {
                    pane.save_modals_spawned.remove(&item_id);
                })?;
                match answer {
                    Ok(0) => {}
                    Ok(1) => {
                        if is_singleton {
                            pane.update_in(cx, |_, window, cx| item.reload(project, window, cx))?
                                .await
                                .log_err();
                        }
                        return Ok(true);
                    }
                    _ => return Ok(false),
                }
            }

            pane.update_in(cx, |pane, window, cx| {
                pane.unpreview_item_if_preview(item.item_id());
                item.save(project, window, cx)
            })?
            .await?;
        }

        pane.update(cx, |_, cx| {
            cx.emit(PaneEvent::UserSavedItem {
                item: item.downgrade_item(),
                save_intent,
            });
        })?;

        Ok(true)
    }

    fn handle_item_event(
        &mut self,
        item_id: EntityId,
        event: ItemEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ItemEvent::CloseItem => {
                self.close_item_by_id(item_id, actions::pane::SaveIntent::Close, window, cx)
                    .detach_and_log_err(cx);
            }
            ItemEvent::UpdateTab => {
                cx.emit(PaneEvent::ChangeItemTitle);
                cx.notify();
            }
            ItemEvent::Edit => {
                self.handle_item_edit(item_id, cx);
                cx.notify();
            }
        }
    }

    fn render_tab_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if self.workspace.upgrade().is_none() {
            return Empty.into_any();
        }

        let tab_items = self
            .items
            .iter()
            .enumerate()
            .zip(tab_details(&self.items, window, cx))
            .map(|((item_index, item), detail)| {
                self.render_tab(item_index, item.as_ref(), detail, window, cx)
            })
            .collect::<Vec<_>>();

        TabBar::new("tab-bar")
            .track_scroll(&self.tab_bar_scroll_handle)
            .children(tab_items)
            .child(Self::render_tab_bar_drop_target(cx))
            .into_any_element()
    }

    fn render_tab_bar_drop_target(cx: &mut Context<Pane>) -> AnyElement {
        gpui::div()
            .id("tab-bar-drop-target")
            .min_w_6()
            .h(Tab::container_height(cx))
            .flex_1()
            .drag_over::<DraggedTab>(|bar, _, _, cx| {
                bar.bg(cx.theme().colors().drop_target_background)
            })
            .on_drop(
                cx.listener(move |pane, dragged_tab: &DraggedTab, window, cx| {
                    pane.handle_tab_drop(dragged_tab, pane.items.len(), window, cx);
                }),
            )
            .into_any_element()
    }

    fn render_tab(
        &self,
        item_index: usize,
        item: &dyn ItemHandle,
        detail: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_active = item_index == self.active_item_index;
        let is_preview = self
            .preview_item_id
            .is_some_and(|preview_item_id| preview_item_id == item.item_id());
        let item_id = item.item_id();
        let is_first_item = item_index == 0;
        let is_last_item = item_index + 1 == self.items.len();
        let position_relative_to_active_item = item_index.cmp(&self.active_item_index);

        let label = item.tab_content(
            TabContentParams {
                detail: Some(detail),
                selected: is_active,
                preview: is_preview,
                deemphasized: !self.has_focus(window, cx),
            },
            window,
            cx,
        );
        let icon = item
            .tab_icon(window, cx)
            .map(|icon| icon.size(IconSize::Small).color(Color::Muted));
        let is_dirty = item.is_dirty(cx);
        let tab_tooltip_content = item.tab_tooltip_content(cx);
        let tab_control_group_name = format!("tab-control-{item_index}");

        let close_button = IconButton::new(("close-tab", item_index), IconName::Close)
            .shape(IconButtonShape::Square)
            .icon_color(Color::Muted)
            .size(ButtonSize::None)
            .icon_size(IconSize::Small)
            .tooltip(Tooltip::text("Close Tab"))
            .on_click(cx.listener(move |pane, _, window, cx| {
                pane.close_item_by_id(item_id, actions::pane::SaveIntent::Close, window, cx)
                    .detach_and_log_err(cx);
            }));
        let tab_control = if is_dirty {
            ui::h_flex()
                .group(tab_control_group_name.clone())
                .relative()
                .size(gpui::px(14.0))
                .justify_center()
                .child(render_item_indicator(tab_control_group_name.clone(), cx))
                .child(
                    ui::h_flex()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .justify_center()
                        .visible_on_hover(tab_control_group_name)
                        .child(close_button),
                )
                .into_any_element()
        } else if is_active {
            close_button.into_any_element()
        } else {
            close_button.visible_on_hover("tab").into_any_element()
        };

        Tab::new(item_index)
            .position(if is_first_item {
                TabPosition::First
            } else if is_last_item {
                TabPosition::Last
            } else {
                TabPosition::Middle(position_relative_to_active_item)
            })
            .toggle_state(is_active)
            .on_click(
                cx.listener(move |pane: &mut Self, event: &ClickEvent, window, cx| {
                    if event.click_count() > 1 {
                        pane.unpreview_item_if_preview(item_id);
                    }

                    pane.activate_item(item_index, true, true, window, cx);
                }),
            )
            .on_drag(
                DraggedTab {
                    item: item.boxed_clone(),
                    pane: cx.entity(),
                    detail,
                    is_active,
                    index: item_index,
                },
                |tab, _, _, cx| cx.new(|_| tab.clone()),
            )
            .drag_over::<DraggedTab>(move |tab, dragged_tab: &DraggedTab, _, cx| {
                let mut styled_tab = tab
                    .bg(cx.theme().colors().drop_target_background)
                    .border_color(cx.theme().colors().drop_target_border)
                    .border_0();

                if item_index < dragged_tab.index {
                    styled_tab = styled_tab.border_l_2();
                } else if item_index > dragged_tab.index {
                    styled_tab = styled_tab.border_r_2();
                }

                styled_tab
            })
            .on_drop(
                cx.listener(move |pane, dragged_tab: &DraggedTab, window, cx| {
                    pane.handle_tab_drop(dragged_tab, item_index, window, cx);
                }),
            )
            .end_slot(tab_control)
            .child(
                ui::h_flex()
                    .id(("pane-tab-content", item_index))
                    .gap_1()
                    .when_some(icon, |this, icon| this.child(icon))
                    .child(label)
                    .map(|this| match tab_tooltip_content {
                        Some(TabTooltipContent::Text(text)) => this.tooltip(Tooltip::text(text)),
                        Some(TabTooltipContent::Custom(element_fn)) => {
                            this.tooltip(move |window, cx| element_fn(window, cx))
                        }
                        None => this,
                    }),
            )
            .into_any_element()
    }

    pub fn handle_tab_drop(
        &mut self,
        dragged_tab: &DraggedTab,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if dragged_tab.pane != cx.entity() {
            return;
        }

        let item_id = dragged_tab.item.item_id();
        self.unpreview_item_if_preview(item_id);
        self.add_item(
            dragged_tab.item.clone(),
            true,
            true,
            true,
            Some(index),
            window,
            cx,
        );
        window.focus(&self.focus_handle(cx), cx);
    }

    pub fn set_should_display_welcome_page(&mut self, should_display_welcome_page: bool) {
        self.should_display_welcome_page = should_display_welcome_page;
    }

    pub fn should_display_welcome_page(&self) -> bool {
        self.should_display_welcome_page
    }

    fn focus_in(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.was_focused {
            self.was_focused = true;
            if self.items.get(self.active_item_index).is_some() {
                self.update_active_tab(self.active_item_index);
            }
            cx.notify();
        }

        if let Some(active_item) = self.active_item() {
            if self.focus_handle.is_focused(window) {
                cx.on_next_frame(window, |_, _, cx| {
                    cx.notify();
                });

                active_item.item_focus_handle(cx).focus(window, cx);
            }
        } else if self.should_display_welcome_page()
            && let Some(welcome_page) = self.welcome_page.as_ref()
            && self.focus_handle.is_focused(window)
        {
            welcome_page.read(cx).focus_handle(cx).focus(window, cx);
        }
    }

    fn focus_out(&mut self, _event: FocusOutEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.was_focused = false;
        cx.notify();
    }
}

impl Focusable for Pane {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for DraggedTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let label = self.item.tab_content(
            TabContentParams {
                detail: Some(self.detail),
                selected: false,
                preview: false,
                deemphasized: false,
            },
            window,
            cx,
        );
        let ui_font = ThemeSettings::get_global(cx).ui_font.clone();

        Tab::new("dragged-tab")
            .toggle_state(self.is_active)
            .child(label)
            .render(window, cx)
            .font(ui_font)
    }
}

impl Render for Pane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_worktree = self
            .project
            .upgrade()
            .is_some_and(|project| project.read(cx).root_worktree(cx).is_some());

        if !has_worktree && self.should_display_welcome_page() {
            if self.welcome_page.is_none() {
                let workspace = self.workspace.clone();
                self.welcome_page = Some(cx.new(|cx| WelcomePage::new(workspace, window, cx)));
            }

            return gpui::div()
                .track_focus(&self.focus_handle)
                .size_full()
                .overflow_hidden()
                .bg(cx.theme().colors().panel_background)
                .child(
                    ui::h_flex()
                        .size_full()
                        .justify_center()
                        .when_some(self.welcome_page.clone(), |container, welcome_page| {
                            container.child(welcome_page)
                        }),
                );
        }

        let active_item = self.active_item().map(|item| item.to_any_view());
        let should_render_tab_bar = !self.items.is_empty();

        gpui::div()
            .track_focus(&self.focus_handle)
            .key_context("Pane")
            .on_action(cx.listener(
                |pane: &mut Self, action: &actions::pane::CloseActiveItem, window, cx| {
                    pane.close_active_item(action, window, cx)
                        .detach_and_log_err(cx);
                },
            ))
            .on_action(cx.listener(
                |pane: &mut Self, action: &actions::pane::CloseAllItems, window, cx| {
                    pane.close_all_items(action, window, cx)
                        .detach_and_log_err(cx);
                },
            ))
            .flex()
            .flex_col()
            .size_full()
            .overflow_hidden()
            .bg(cx.theme().colors().panel_background)
            .when(should_render_tab_bar, |this| {
                this.child(self.render_tab_bar(window, cx))
            })
            .child(
                gpui::div()
                    .flex_1()
                    .overflow_hidden()
                    .when_some(active_item, |this, active_item| this.child(active_item)),
            )
    }
}

fn render_item_indicator(group_name: String, cx: &App) -> AnyElement {
    gpui::div()
        .size(gpui::px(6.0))
        .rounded_sm()
        .bg(cx.theme().colors().text_accent)
        .group_hover(group_name, |style| style.invisible())
        .into_any_element()
}

fn dirty_message_for(buffer_path: Option<ProjectPath>, path_style: PathStyle) -> String {
    let path = buffer_path
        .and_then(|project_path| {
            let path = project_path.path.display(path_style);
            if path.is_empty() {
                None
            } else {
                Some(path.into_owned())
            }
        })
        .unwrap_or_else(|| "This buffer".to_string());
    let path = util::truncate_and_remove_front(&path, 80);
    format!("{path} contains unsaved edits. Do you want to save it?")
}

fn tab_details(items: &[Box<dyn ItemHandle>], _window: &Window, cx: &App) -> Vec<usize> {
    util::disambiguate::compute_disambiguation_details(items, |item, detail| {
        item.tab_content_text(detail, cx)
    })
}

#[cfg(test)]
pub mod test {
    use super::*;

    use gpui::VisualTestContext;

    use crate::item::test::TestItem;

    pub fn add_labeled_item(
        pane: &Entity<Pane>,
        label: &str,
        is_dirty: bool,
        cx: &mut VisualTestContext,
    ) -> Box<Entity<TestItem>> {
        pane.update_in(cx, |pane, window, cx| {
            let labeled_item =
                Box::new(cx.new(|cx| TestItem::new(cx).with_label(label).with_dirty(is_dirty)));
            pane.add_item(labeled_item.clone(), false, false, true, None, window, cx);
            labeled_item
        })
    }

    #[track_caller]
    pub fn assert_item_labels<const COUNT: usize>(
        pane: &Entity<Pane>,
        expected_states: [&str; COUNT],
        cx: &mut VisualTestContext,
    ) {
        let actual_states = pane.update(cx, |pane, cx| {
            pane.items()
                .enumerate()
                .map(|(index, item)| {
                    let mut state = item
                        .to_any_view()
                        .downcast::<TestItem>()
                        .unwrap()
                        .read(cx)
                        .label
                        .clone();
                    if index == pane.active_item_index() {
                        state.push('*');
                    }
                    if item.is_dirty(cx) {
                        state.push('^');
                    }
                    state
                })
                .collect::<Vec<_>>()
        });

        assert_eq!(
            actual_states, expected_states,
            "pane items do not match expectation"
        );
    }
}
