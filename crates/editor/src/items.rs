use gpui::{App, AppContext, Context, Entity, EntityId, SharedString, Task, WeakEntity, Window};
use std::{borrow::Cow, path::Path, sync::Arc};

use icons::FileIcons;
use language::{Buffer, Capability};
use multi_buffer::MultiBuffer;
use project::Project;
use ui::Icon;
use workspace::{
    Item, ItemBufferKind, ItemEvent, ItemId, ProjectItem, SerializableItem, Workspace, WorkspaceId,
    delete_unloaded_items, pane::Pane,
};

use crate::{
    Editor, EditorEvent,
    persistence::{EditorDb, SerializedEditor},
    scroll::Autoscroll,
};

impl Item for Editor {
    type Event = EditorEvent;

    fn to_item_events(event: &Self::Event, f: &mut dyn FnMut(ItemEvent)) {
        match event {
            EditorEvent::Saved | EditorEvent::TitleChanged | EditorEvent::DirtyChanged => {
                f(ItemEvent::UpdateTab);
            }
            EditorEvent::BufferEdited => f(ItemEvent::Edit),
            EditorEvent::Blurred | EditorEvent::FileHandleChanged => {}
        }
    }

    fn tab_content_text(&self, detail: usize, cx: &App) -> SharedString {
        if let Some(path) = path_for_buffer(&self.buffer, detail, true, cx) {
            path.to_string().into()
        } else {
            self.buffer.read(cx).title(cx).to_string().into()
        }
    }

    fn tab_tooltip_text(&self, cx: &App) -> Option<SharedString> {
        let multi_buffer = self.buffer.read(cx);
        if let Some(file) = multi_buffer
            .as_singleton()
            .and_then(|buffer| buffer.read(cx).file())
            .and_then(|file| project::File::from_dyn(Some(file)))
        {
            Some(
                file.worktree
                    .read(cx)
                    .absolutize(&file.path)
                    .to_string_lossy()
                    .into_owned()
                    .into(),
            )
        } else {
            let title = multi_buffer.title(cx);
            (!title.is_empty()).then(|| title.to_string().into())
        }
    }

    fn tab_icon(&self, _: &Window, cx: &App) -> Option<Icon> {
        path_for_buffer(&self.buffer, 0, true, cx)
            .and_then(|path| FileIcons::get_icon(Path::new(path.as_ref()), cx))
            .map(Icon::from_path)
    }

    fn for_each_project_item(
        &self,
        cx: &App,
        f: &mut dyn FnMut(EntityId, &dyn project::ProjectItem),
    ) {
        if let Some(buffer) = self.buffer.read(cx).as_singleton() {
            f(buffer.entity_id(), buffer.read(cx));
        }
    }

    fn buffer_kind(&self, cx: &App) -> ItemBufferKind {
        if self.buffer.read(cx).as_singleton().is_some() {
            ItemBufferKind::Singleton
        } else {
            ItemBufferKind::None
        }
    }

    fn is_dirty(&self, cx: &App) -> bool {
        self.buffer.read(cx).is_dirty(cx)
    }

    fn capability(&self, cx: &App) -> Capability {
        self.capability(cx)
    }

    fn can_save(&self, cx: &App) -> bool {
        let Some(buffer) = self.buffer.read(cx).as_singleton() else {
            return false;
        };

        !self.read_only(cx) && project::ProjectItem::project_path(buffer.read(cx), cx).is_some()
    }

    fn save(
        &mut self,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        let Some(buffer) = self.buffer.read(cx).as_singleton() else {
            return Task::ready(Err(anyhow::anyhow!("Cannot save multi-buffer editor")));
        };
        let save = project.update(cx, |project, cx| project.save_buffer(buffer, cx));

        cx.spawn_in(window, async move |_, _| {
            save.await?;
            Ok(())
        })
    }

    fn reload(
        &mut self,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        let Some(buffer) = self.buffer.read(cx).as_singleton() else {
            return Task::ready(Err(anyhow::anyhow!("Cannot reload multi-buffer editor")));
        };
        let reload = project.update(cx, |project, cx| project.reload_buffer(&buffer, cx));

        cx.spawn_in(window, async move |this, cx| {
            reload.await?;
            this.update(cx, |editor, cx| {
                editor.request_autoscroll(Autoscroll::fit(), cx);
            })?;
            Ok(())
        })
    }
}

fn path_for_buffer<'a>(
    buffer: &Entity<MultiBuffer>,
    height: usize,
    include_filename: bool,
    cx: &'a App,
) -> Option<Cow<'a, str>> {
    let file = buffer.read(cx).as_singleton()?.read(cx).file()?;
    path_for_file(file, height, include_filename, cx)
}

fn path_for_file<'a>(
    file: &'a Arc<dyn language::File>,
    mut height: usize,
    include_filename: bool,
    cx: &'a App,
) -> Option<Cow<'a, str>> {
    project::File::from_dyn(Some(file))?;

    let file = file.as_ref();
    height += 1;

    let mut prefix = file.path().as_ref();
    while height > 0 {
        if let Some(parent) = prefix.parent() {
            prefix = parent;
            height -= 1;
        } else {
            break;
        }
    }

    if height > 0 {
        let mut full_path = file.full_path(cx);
        if !include_filename && !full_path.pop() {
            return None;
        }
        Some(full_path.to_string_lossy().into_owned().into())
    } else {
        let mut path = file.path().strip_prefix(prefix).ok()?;
        if !include_filename {
            path = path.parent()?;
        }
        Some(path.display(file.path_style(cx)))
    }
}

impl ProjectItem for Editor {
    type Item = Buffer;

    fn for_project_item(
        _: Entity<Project>,
        _: Option<&Pane>,
        item: Entity<Self::Item>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self
    where
        Self: Sized,
    {
        Self::for_buffer(item, window, cx)
    }
}

impl SerializableItem for Editor {
    fn serialized_item_kind() -> &'static str {
        "Editor"
    }

    fn cleanup(
        workspace_id: WorkspaceId,
        alive_items: Vec<ItemId>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<()>> {
        delete_unloaded_items(
            alive_items,
            workspace_id,
            "editor",
            &EditorDb::global(cx),
            cx,
        )
    }

    fn deserialize(
        project: Entity<Project>,
        _workspace: WeakEntity<Workspace>,
        workspace_id: WorkspaceId,
        item_id: ItemId,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<Entity<Self>>> {
        let serialized_editor = match EditorDb::global(cx)
            .load_serialized_editor(item_id, workspace_id)
        {
            Ok(Some(serialized_editor)) => serialized_editor,
            Ok(None) => {
                return Task::ready(Err(anyhow::anyhow!(
                    "Unable to deserialize editor: No entry in database for item_id: {item_id} and workspace_id {workspace_id:?}"
                )));
            }
            Err(error) => return Task::ready(Err(error)),
        };
        let path = serialized_editor.absolute_path;

        let Some(project_path) = project
            .read(cx)
            .project_path_for_absolute_path(path.as_path(), cx)
        else {
            return Task::ready(Err(anyhow::anyhow!(
                "Unable to deserialize editor: path is not in project: {}",
                path.display()
            )));
        };

        let Some(open_buffer) =
            <Buffer as project::ProjectItem>::try_open(&project, &project_path, cx)
        else {
            return Task::ready(Err(anyhow::anyhow!(
                "Unable to deserialize editor: cannot open path: {}",
                path.display()
            )));
        };

        window.spawn(cx, async move |cx| {
            let buffer = open_buffer.await?;
            cx.update(|window, cx| cx.new(|cx| Editor::for_buffer(buffer, window, cx)))
        })
    }

    fn serialize(
        &mut self,
        workspace: &mut Workspace,
        item_id: ItemId,
        _closing: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<anyhow::Result<()>>> {
        let buffer = self.buffer.read(cx).as_singleton()?;
        let project_path = project::ProjectItem::project_path(buffer.read(cx), cx)?;
        let path = workspace
            .project()
            .read(cx)
            .absolute_path(&project_path, cx)?;
        let workspace_id = workspace.database_id()?;
        let editor_db = EditorDb::global(cx);

        Some(cx.spawn_in(window, async move |_, _| {
            editor_db
                .save_serialized_editor(
                    item_id,
                    workspace_id,
                    SerializedEditor {
                        absolute_path: path,
                    },
                )
                .await
        }))
    }

    fn should_serialize(&self, event: &Self::Event) -> bool {
        matches!(
            event,
            EditorEvent::Saved
                | EditorEvent::DirtyChanged
                | EditorEvent::BufferEdited
                | EditorEvent::FileHandleChanged
        )
    }
}
