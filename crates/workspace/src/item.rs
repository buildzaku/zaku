use gpui::{
    AnyElement, AnyView, App, Context, Entity, EntityId, EventEmitter, FocusHandle, Focusable,
    Render, SharedString, Subscription, Task, WeakEntity, Window, prelude::*,
};
use smallvec::SmallVec;
use std::{any::Any, sync::Arc};

use language::Capability;
use project::{Project, ProjectEntryId, ProjectPath};
use ui::{Color, Icon, Label, LabelCommon};

use crate::{
    SerializableItemRegistry, Workspace, WorkspaceId, pane::Pane, persistence::model::ItemId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemEvent {
    CloseItem,
    UpdateTab,
    Edit,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TabContentParams {
    pub detail: Option<usize>,
    pub selected: bool,
    pub preview: bool,
    pub deemphasized: bool,
}

impl TabContentParams {
    pub fn text_color(&self) -> Color {
        if self.deemphasized {
            Color::Hidden
        } else {
            Color::Muted
        }
    }
}

pub enum TabTooltipContent {
    Text(SharedString),
    Custom(Box<dyn Fn(&mut Window, &mut App) -> AnyView>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemBufferKind {
    Singleton,
    None,
}

pub trait Item: Focusable + EventEmitter<Self::Event> + Render + Sized {
    type Event;

    fn tab_content(&self, params: TabContentParams, _window: &Window, cx: &App) -> AnyElement {
        let text = self.tab_content_text(params.detail.unwrap_or_default(), cx);

        Label::new(text)
            .color(params.text_color())
            .when(params.preview, |label| label.italic())
            .into_any_element()
    }

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString;

    fn tab_icon(&self, _window: &Window, _cx: &App) -> Option<Icon> {
        None
    }

    fn tab_tooltip_text(&self, _cx: &App) -> Option<SharedString> {
        None
    }

    fn tab_tooltip_content(&self, cx: &App) -> Option<TabTooltipContent> {
        self.tab_tooltip_text(cx).map(TabTooltipContent::Text)
    }

    fn to_item_events(_event: &Self::Event, _emitter: &mut dyn FnMut(ItemEvent)) {}

    fn deactivated(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}

    fn on_removed(&self, _cx: &mut Context<Self>) {}

    fn navigate(
        &mut self,
        _data: Arc<dyn Any + Send>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> bool {
        false
    }

    fn for_each_project_item(
        &self,
        _cx: &App,
        _visitor: &mut dyn FnMut(EntityId, &dyn project::ProjectItem),
    ) {
    }

    fn buffer_kind(&self, _cx: &App) -> ItemBufferKind {
        ItemBufferKind::None
    }

    fn is_dirty(&self, _cx: &App) -> bool {
        false
    }

    fn capability(&self, _cx: &App) -> Capability {
        Capability::ReadWrite
    }

    fn can_save(&self, _cx: &App) -> bool {
        false
    }

    fn save(
        &mut self,
        _project: Entity<Project>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        unimplemented!("save() must be implemented if can_save() returns true")
    }

    fn reload(
        &mut self,
        _project: Entity<Project>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        unimplemented!("reload() must be implemented if can_save() returns true")
    }

    fn preserve_preview(&self, _cx: &App) -> bool {
        false
    }

    fn include_in_nav_history() -> bool {
        true
    }
}

pub trait SerializableItem: Item + 'static {
    fn serialized_item_kind() -> &'static str;

    fn cleanup(
        workspace_id: WorkspaceId,
        alive_items: Vec<ItemId>,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<()>>;

    fn deserialize(
        project: Entity<Project>,
        workspace: WeakEntity<Workspace>,
        workspace_id: WorkspaceId,
        item_id: ItemId,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<Entity<Self>>>;

    fn serialize(
        &mut self,
        workspace: &mut Workspace,
        item_id: ItemId,
        closing: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<anyhow::Result<()>>>;

    fn should_serialize(&self, event: &Self::Event) -> bool;
}

pub trait SerializableItemHandle: ItemHandle {
    fn serialized_item_kind(&self) -> &'static str;
    fn serialize(
        &self,
        workspace: &mut Workspace,
        closing: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Task<anyhow::Result<()>>>;
    fn should_serialize(&self, event: &dyn Any, cx: &App) -> bool;
}

impl<T> SerializableItemHandle for Entity<T>
where
    T: SerializableItem,
{
    fn serialized_item_kind(&self) -> &'static str {
        T::serialized_item_kind()
    }

    fn serialize(
        &self,
        workspace: &mut Workspace,
        closing: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Task<anyhow::Result<()>>> {
        self.update(cx, |this, cx| {
            this.serialize(workspace, cx.entity_id().as_u64(), closing, window, cx)
        })
    }

    fn should_serialize(&self, event: &dyn Any, cx: &App) -> bool {
        event
            .downcast_ref::<T::Event>()
            .is_some_and(|event| self.read(cx).should_serialize(event))
    }
}

pub trait ItemHandle: 'static + Send {
    fn item_focus_handle(&self, cx: &App) -> FocusHandle;
    fn subscribe_to_item_events(
        &self,
        window: &mut Window,
        cx: &mut App,
        handler: Box<dyn Fn(ItemEvent, &mut Window, &mut App)>,
    ) -> Subscription;
    fn tab_content(&self, params: TabContentParams, window: &Window, cx: &App) -> AnyElement;
    fn tab_content_text(&self, detail: usize, cx: &App) -> SharedString;
    fn tab_icon(&self, window: &Window, cx: &App) -> Option<Icon>;
    fn tab_tooltip_text(&self, cx: &App) -> Option<SharedString>;
    fn tab_tooltip_content(&self, cx: &App) -> Option<TabTooltipContent>;
    fn project_path(&self, cx: &App) -> Option<ProjectPath>;
    fn project_entry_ids(&self, cx: &App) -> SmallVec<[ProjectEntryId; 3]>;
    fn for_each_project_item(
        &self,
        cx: &App,
        visitor: &mut dyn FnMut(EntityId, &dyn project::ProjectItem),
    );
    fn buffer_kind(&self, cx: &App) -> ItemBufferKind;
    fn boxed_clone(&self) -> Box<dyn ItemHandle>;
    fn downgrade_item(&self) -> Box<dyn WeakItemHandle>;
    fn added_to_pane(
        &self,
        workspace: &mut Workspace,
        pane: Entity<Pane>,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    );
    fn deactivated(&self, window: &mut Window, cx: &mut App);
    fn on_removed(&self, cx: &mut App);
    fn navigate(&self, data: Arc<dyn Any + Send>, window: &mut Window, cx: &mut App) -> bool;
    fn item_id(&self) -> EntityId;
    fn to_serializable_item_handle(&self, cx: &App) -> Option<Box<dyn SerializableItemHandle>>;
    fn to_any_view(&self) -> AnyView;
    fn is_dirty(&self, cx: &App) -> bool;
    fn capability(&self, cx: &App) -> Capability;
    fn can_save(&self, cx: &App) -> bool;
    fn save(
        &self,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<()>>;
    fn reload(
        &self,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<()>>;
    fn preserve_preview(&self, cx: &App) -> bool;
    fn include_in_nav_history(&self) -> bool;
}

pub trait WeakItemHandle: Send + Sync {
    fn id(&self) -> EntityId;
    fn boxed_clone(&self) -> Box<dyn WeakItemHandle>;
    fn upgrade(&self) -> Option<Box<dyn ItemHandle>>;
}

impl dyn ItemHandle {
    pub fn downcast<V: 'static>(&self) -> Option<Entity<V>> {
        self.to_any_view().downcast().ok()
    }
}

impl<T: Item> ItemHandle for Entity<T> {
    fn item_focus_handle(&self, cx: &App) -> FocusHandle {
        self.read(cx).focus_handle(cx)
    }

    fn subscribe_to_item_events(
        &self,
        window: &mut Window,
        cx: &mut App,
        handler: Box<dyn Fn(ItemEvent, &mut Window, &mut App)>,
    ) -> Subscription {
        window.subscribe(self, cx, move |_, event, window, cx| {
            T::to_item_events(event, &mut |item_event| handler(item_event, window, cx));
        })
    }

    fn tab_content(&self, params: TabContentParams, window: &Window, cx: &App) -> AnyElement {
        self.read(cx).tab_content(params, window, cx)
    }

    fn tab_content_text(&self, detail: usize, cx: &App) -> SharedString {
        self.read(cx).tab_content_text(detail, cx)
    }

    fn tab_icon(&self, window: &Window, cx: &App) -> Option<Icon> {
        self.read(cx).tab_icon(window, cx)
    }

    fn tab_tooltip_text(&self, cx: &App) -> Option<SharedString> {
        self.read(cx).tab_tooltip_text(cx)
    }

    fn tab_tooltip_content(&self, cx: &App) -> Option<TabTooltipContent> {
        self.read(cx).tab_tooltip_content(cx)
    }

    fn project_path(&self, cx: &App) -> Option<ProjectPath> {
        let this = self.read(cx);
        let mut result = None;
        if this.buffer_kind(cx) == ItemBufferKind::Singleton {
            this.for_each_project_item(cx, &mut |_, item| {
                result = item.project_path(cx);
            });
        }
        result
    }

    fn project_entry_ids(&self, cx: &App) -> SmallVec<[ProjectEntryId; 3]> {
        let mut result = SmallVec::new();
        self.read(cx).for_each_project_item(cx, &mut |_, item| {
            if let Some(id) = item.entry_id(cx) {
                result.push(id);
            }
        });
        result
    }

    fn for_each_project_item(
        &self,
        cx: &App,
        visitor: &mut dyn FnMut(EntityId, &dyn project::ProjectItem),
    ) {
        self.read(cx).for_each_project_item(cx, visitor);
    }

    fn buffer_kind(&self, cx: &App) -> ItemBufferKind {
        self.read(cx).buffer_kind(cx)
    }

    fn boxed_clone(&self) -> Box<dyn ItemHandle> {
        Box::new(self.clone())
    }

    fn downgrade_item(&self) -> Box<dyn WeakItemHandle> {
        Box::new(self.downgrade())
    }

    fn added_to_pane(
        &self,
        workspace: &mut Workspace,
        pane: Entity<Pane>,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        if let Some(serializable_item) = self.to_serializable_item_handle(cx)
            && let Err(error) = workspace.enqueue_item_serialization(serializable_item)
        {
            log::debug!("Failed to enqueue item serialization: {error}");
        }

        let old_item_pane = workspace
            .panes_by_item
            .insert(self.item_id(), pane.downgrade());

        if old_item_pane.is_none() {
            let mut event_subscription = Some(cx.subscribe_in(
                self,
                window,
                move |workspace, item: &Entity<T>, event, window, cx| {
                    let Some(pane) = workspace
                        .panes_by_item
                        .get(&item.item_id())
                        .and_then(|pane| pane.upgrade())
                    else {
                        return;
                    };

                    if let Some(serializable_item) = item.to_serializable_item_handle(cx)
                        && serializable_item.should_serialize(event, cx)
                        && let Err(error) = workspace.enqueue_item_serialization(serializable_item)
                    {
                        log::debug!("Failed to enqueue item serialization: {error}");
                    }

                    T::to_item_events(event, &mut |item_event| {
                        pane.update(cx, |pane, cx| {
                            pane.handle_item_event(item.item_id(), item_event, window, cx);
                        });
                    });
                },
            ));

            let item_id = self.item_id();
            cx.observe_release_in(self, window, move |workspace, _, _, _| {
                workspace.panes_by_item.remove(&item_id);
                event_subscription.take();
            })
            .detach();
        }

        cx.defer_in(window, |workspace, window, cx| {
            workspace.serialize_workspace(window, cx);
        });
    }

    fn deactivated(&self, window: &mut Window, cx: &mut App) {
        self.update(cx, |this, cx| this.deactivated(window, cx));
    }

    fn on_removed(&self, cx: &mut App) {
        self.update(cx, |this, cx| this.on_removed(cx));
    }

    fn navigate(&self, data: Arc<dyn Any + Send>, window: &mut Window, cx: &mut App) -> bool {
        self.update(cx, |this, cx| this.navigate(data, window, cx))
    }

    fn item_id(&self) -> EntityId {
        Entity::entity_id(self)
    }

    fn to_serializable_item_handle(&self, cx: &App) -> Option<Box<dyn SerializableItemHandle>> {
        SerializableItemRegistry::view_to_serializable_item_handle(self.to_any_view(), cx)
    }

    fn to_any_view(&self) -> AnyView {
        self.clone().into()
    }

    fn is_dirty(&self, cx: &App) -> bool {
        self.read(cx).is_dirty(cx)
    }

    fn capability(&self, cx: &App) -> Capability {
        self.read(cx).capability(cx)
    }

    fn can_save(&self, cx: &App) -> bool {
        self.read(cx).can_save(cx)
    }

    fn save(
        &self,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<()>> {
        self.update(cx, |item, cx| item.save(project, window, cx))
    }

    fn reload(
        &self,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<()>> {
        self.update(cx, |item, cx| item.reload(project, window, cx))
    }

    fn preserve_preview(&self, cx: &App) -> bool {
        self.read(cx).preserve_preview(cx)
    }

    fn include_in_nav_history(&self) -> bool {
        T::include_in_nav_history()
    }
}

impl From<Box<dyn ItemHandle>> for AnyView {
    fn from(value: Box<dyn ItemHandle>) -> Self {
        value.to_any_view()
    }
}

impl From<&Box<dyn ItemHandle>> for AnyView {
    fn from(value: &Box<dyn ItemHandle>) -> Self {
        value.to_any_view()
    }
}

impl Clone for Box<dyn ItemHandle> {
    fn clone(&self) -> Box<dyn ItemHandle> {
        self.boxed_clone()
    }
}

impl<T: Item> WeakItemHandle for WeakEntity<T> {
    fn id(&self) -> EntityId {
        self.entity_id()
    }

    fn boxed_clone(&self) -> Box<dyn WeakItemHandle> {
        Box::new(self.clone())
    }

    fn upgrade(&self) -> Option<Box<dyn ItemHandle>> {
        self.upgrade()
            .map(|item| Box::new(item) as Box<dyn ItemHandle>)
    }
}

pub trait ProjectItem: Item {
    type Item: project::ProjectItem;

    fn for_project_item(
        project: Entity<Project>,
        pane: Option<&Pane>,
        item: Entity<Self::Item>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self
    where
        Self: Sized;
}

#[cfg(test)]
pub mod test {
    use super::*;

    use gpui::{Empty, IntoElement};

    use path::rel_path;
    use project::WorktreeId;

    pub struct TestProjectItem {
        pub entry_id: Option<ProjectEntryId>,
        pub project_path: Option<ProjectPath>,
        pub is_dirty: bool,
    }

    impl TestProjectItem {
        pub fn new_dirty(id: usize, path: &str, cx: &mut App) -> Entity<Self> {
            cx.new(|_| Self {
                entry_id: Some(ProjectEntryId::from_usize(id)),
                project_path: Some(ProjectPath {
                    worktree_id: WorktreeId::from_usize(0),
                    path: Arc::from(rel_path(path)),
                }),
                is_dirty: true,
            })
        }
    }

    impl project::ProjectItem for TestProjectItem {
        fn try_open(
            _project: &Entity<Project>,
            _path: &ProjectPath,
            _cx: &mut App,
        ) -> Option<Task<anyhow::Result<Entity<Self>>>> {
            None
        }

        fn entry_id(&self, _cx: &App) -> Option<ProjectEntryId> {
            self.entry_id
        }

        fn project_path(&self, _cx: &App) -> Option<ProjectPath> {
            self.project_path.clone()
        }

        fn is_dirty(&self) -> bool {
            self.is_dirty
        }
    }

    pub struct TestItem {
        pub workspace_id: Option<WorkspaceId>,
        pub label: String,
        pub save_count: usize,
        pub reload_count: usize,
        pub is_dirty: bool,
        pub buffer_kind: ItemBufferKind,
        pub project_items: Vec<Entity<TestProjectItem>>,
        serialize: Option<Box<dyn Fn() -> Option<Task<anyhow::Result<()>>>>>,
        focus_handle: FocusHandle,
    }

    impl TestItem {
        pub fn new(cx: &mut Context<Self>) -> Self {
            Self {
                workspace_id: None,
                label: String::new(),
                save_count: 0,
                reload_count: 0,
                is_dirty: false,
                buffer_kind: ItemBufferKind::Singleton,
                project_items: Vec::new(),
                serialize: None,
                focus_handle: cx.focus_handle(),
            }
        }

        pub fn new_deserialized(workspace_id: WorkspaceId, cx: &mut Context<Self>) -> Self {
            let mut this = Self::new(cx);
            this.workspace_id = Some(workspace_id);
            this
        }

        pub fn with_label(mut self, label: &str) -> Self {
            self.label = label.to_string();
            self
        }

        pub fn with_dirty(mut self, is_dirty: bool) -> Self {
            self.is_dirty = is_dirty;
            self
        }

        pub fn with_buffer_kind(mut self, buffer_kind: ItemBufferKind) -> Self {
            self.buffer_kind = buffer_kind;
            self
        }

        pub fn with_serialize(
            mut self,
            serialize: impl Fn() -> Option<Task<anyhow::Result<()>>> + 'static,
        ) -> Self {
            self.serialize = Some(Box::new(serialize));
            self
        }
    }

    impl Render for TestItem {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            Empty
        }
    }

    impl EventEmitter<ItemEvent> for TestItem {}

    impl Focusable for TestItem {
        fn focus_handle(&self, _cx: &App) -> FocusHandle {
            self.focus_handle.clone()
        }
    }

    impl Item for TestItem {
        type Event = ItemEvent;

        fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
            self.label.clone().into()
        }

        fn to_item_events(event: &Self::Event, emitter: &mut dyn FnMut(ItemEvent)) {
            emitter(*event);
        }

        fn is_dirty(&self, _cx: &App) -> bool {
            self.is_dirty
        }

        fn for_each_project_item(
            &self,
            cx: &App,
            visitor: &mut dyn FnMut(EntityId, &dyn project::ProjectItem),
        ) {
            self.project_items
                .iter()
                .for_each(|item| visitor(item.entity_id(), item.read(cx)));
        }

        fn buffer_kind(&self, _cx: &App) -> ItemBufferKind {
            self.buffer_kind
        }

        fn can_save(&self, cx: &App) -> bool {
            !self.project_items.is_empty()
                && self
                    .project_items
                    .iter()
                    .all(|item| item.read(cx).entry_id.is_some())
        }

        fn save(
            &mut self,
            _project: Entity<Project>,
            _window: &mut Window,
            cx: &mut Context<Self>,
        ) -> Task<anyhow::Result<()>> {
            self.save_count += 1;
            self.is_dirty = false;
            for item in &self.project_items {
                item.update(cx, |item, _| {
                    if item.is_dirty {
                        item.is_dirty = false;
                    }
                });
            }
            Task::ready(Ok(()))
        }

        fn reload(
            &mut self,
            _project: Entity<Project>,
            _window: &mut Window,
            _cx: &mut Context<Self>,
        ) -> Task<anyhow::Result<()>> {
            self.reload_count += 1;
            self.is_dirty = false;
            Task::ready(Ok(()))
        }
    }

    impl SerializableItem for TestItem {
        fn serialized_item_kind() -> &'static str {
            "TestItem"
        }

        fn serialize(
            &mut self,
            _workspace: &mut Workspace,
            _item_id: ItemId,
            _closing: bool,
            _window: &mut Window,
            _cx: &mut Context<Self>,
        ) -> Option<Task<anyhow::Result<()>>> {
            if let Some(serialize) = self.serialize.take() {
                let result = serialize();
                self.serialize = Some(serialize);
                result
            } else {
                None
            }
        }

        fn deserialize(
            _project: Entity<Project>,
            _workspace: WeakEntity<Workspace>,
            workspace_id: WorkspaceId,
            _item_id: ItemId,
            _window: &mut Window,
            cx: &mut App,
        ) -> Task<anyhow::Result<Entity<Self>>> {
            Task::ready(Ok(cx.new(|cx| Self::new_deserialized(workspace_id, cx))))
        }

        fn cleanup(
            _workspace_id: WorkspaceId,
            _alive_items: Vec<ItemId>,
            _window: &mut Window,
            _cx: &mut App,
        ) -> Task<anyhow::Result<()>> {
            Task::ready(Ok(()))
        }

        fn should_serialize(&self, _event: &Self::Event) -> bool {
            false
        }
    }
}
