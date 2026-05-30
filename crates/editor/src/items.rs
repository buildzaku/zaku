use gpui::{App, Context, Entity, EntityId, SharedString, Task, Window};
use std::{borrow::Cow, path::Path, sync::Arc};

use icons::FileIcons;
use language::{Buffer, Capability};
use multi_buffer::MultiBuffer;
use project::Project;
use ui::Icon;
use workspace::{Item, ItemBufferKind, ItemEvent, ProjectItem, pane::Pane};

use crate::{Editor, EditorEvent, scroll::Autoscroll};

impl Item for Editor {
    type Event = EditorEvent;

    fn to_item_events(event: &Self::Event, f: &mut dyn FnMut(ItemEvent)) {
        match event {
            EditorEvent::BufferEdited => f(ItemEvent::Edit),
            EditorEvent::DirtyChanged => f(ItemEvent::UpdateTab),
            EditorEvent::Blurred => {}
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
        let was_dirty = buffer.read(cx).is_dirty();
        let save = project.update(cx, |project, cx| project.save_buffer(buffer, cx));

        cx.spawn_in(window, async move |this, cx| {
            save.await?;
            if was_dirty {
                this.update(cx, |_, cx| {
                    cx.emit(EditorEvent::DirtyChanged);
                    cx.notify();
                })?;
            }
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
