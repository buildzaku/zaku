use anyhow::anyhow;
use futures::{FutureExt, future::Shared};
use gpui::{AppContext, Context, Entity, EventEmitter, Subscription, Task, WeakEntity};
use std::{
    collections::{HashMap, hash_map},
    io,
    sync::Arc,
};
use text::{BufferId, ReplicaId};

use language::{Buffer, BufferEvent, Capability};
use util::{debug_panic, rel_path::RelPath};
use worktree::{DiskState, File, PathChange, ProjectEntryId, Snapshot, Worktree, WorktreeEvent};

use crate::{
    ProjectPath,
    worktree_store::{WorktreeStore, WorktreeStoreEvent},
};

pub struct BufferStore {
    buffer_ids_by_entry_id: HashMap<ProjectEntryId, BufferId>,
    loading_buffers: HashMap<ProjectPath, Shared<Task<Result<Entity<Buffer>, Arc<anyhow::Error>>>>>,
    worktree_store: Entity<WorktreeStore>,
    opened_buffers: HashMap<BufferId, WeakEntity<Buffer>>,
    path_to_buffer_id: HashMap<ProjectPath, BufferId>,
    _worktree_store_subscription: Subscription,
}

pub enum BufferStoreEvent {
    BufferAdded(Entity<Buffer>),
    BufferDropped(BufferId),
    BufferChangedFilePath {
        buffer: Entity<Buffer>,
        old_file: Option<Arc<dyn language::File>>,
    },
}

impl EventEmitter<BufferStoreEvent> for BufferStore {}

impl BufferStore {
    pub fn new(worktree_store: &Entity<WorktreeStore>, cx: &mut Context<Self>) -> Self {
        Self {
            buffer_ids_by_entry_id: HashMap::default(),
            loading_buffers: HashMap::default(),
            worktree_store: worktree_store.clone(),
            opened_buffers: HashMap::default(),
            path_to_buffer_id: HashMap::default(),
            _worktree_store_subscription: cx.subscribe(worktree_store, |_, _, event, cx| {
                if let WorktreeStoreEvent::WorktreeAdded(worktree) = event {
                    Self::subscribe_to_worktree(worktree, cx);
                }
            }),
        }
    }

    pub fn open_buffer(
        &mut self,
        project_path: ProjectPath,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Entity<Buffer>>> {
        if let Some(buffer) = self.get_by_path(&project_path) {
            return Task::ready(Ok(buffer));
        }

        let task = match self.loading_buffers.entry(project_path.clone()) {
            hash_map::Entry::Occupied(entry) => entry.get().clone(),
            hash_map::Entry::Vacant(entry) => {
                let path = project_path.path.clone();
                let Some(worktree) = self
                    .worktree_store
                    .read(cx)
                    .worktree_for_id(project_path.worktree_id, cx)
                else {
                    return Task::ready(Err(anyhow!("no such worktree")));
                };

                if self
                    .worktree_store
                    .read(cx)
                    .entry_for_path(&project_path, cx)
                    .is_some_and(|entry| entry.is_request)
                {
                    return Task::ready(Err(anyhow!("Cannot open request file")));
                }

                let load_buffer = Self::load_buffer(path, worktree, cx);
                entry
                    .insert(
                        cx.spawn(async move |this, cx| {
                            let load_result = load_buffer.await;
                            this.update(cx, |this, _cx| {
                                this.loading_buffers.remove(&project_path);

                                let buffer = load_result.map_err(Arc::new)?;
                                Ok(buffer)
                            })?
                        })
                        .shared(),
                    )
                    .clone()
            }
        };

        cx.background_spawn(async move { task.await.map_err(|error| anyhow!("{error}")) })
    }

    pub fn save_buffer(
        &self,
        buffer: Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        let (worktree, path) = {
            let buffer = buffer.read(cx);
            let Some(file) = File::from_dyn(buffer.file()) else {
                return Task::ready(Err(anyhow!("buffer doesn't have a file")));
            };
            (file.worktree.clone(), file.path.clone())
        };
        Self::save_buffer_at(buffer, &worktree, path, false, cx)
    }

    pub fn reload_buffer(
        &self,
        buffer: &Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        let reload = buffer.update(cx, |buffer, cx| buffer.reload(cx));
        cx.spawn(async move |_, _| {
            reload.await.map_err(|_| anyhow!("reload canceled"))?;
            Ok(())
        })
    }

    pub fn get_by_path(&self, path: &ProjectPath) -> Option<Entity<Buffer>> {
        self.path_to_buffer_id
            .get(path)
            .and_then(|buffer_id| self.get(*buffer_id))
    }

    pub fn get(&self, buffer_id: BufferId) -> Option<Entity<Buffer>> {
        self.opened_buffers.get(&buffer_id)?.upgrade()
    }

    pub fn buffers(&self) -> impl '_ + Iterator<Item = Entity<Buffer>> {
        self.opened_buffers.values().filter_map(WeakEntity::upgrade)
    }

    fn add_buffer(
        &mut self,
        buffer_entity: Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        let (buffer_id, path) = {
            let buffer = buffer_entity.read(cx);
            (
                buffer.remote_id(),
                File::from_dyn(buffer.file()).map(|file| ProjectPath {
                    worktree_id: file.worktree_id(cx),
                    path: file.path.clone(),
                }),
            )
        };
        let open_buffer = buffer_entity.downgrade();

        let handle = cx.entity().downgrade();
        buffer_entity.update(cx, move |_, cx| {
            cx.on_release(move |buffer, cx| {
                handle
                    .update(cx, |_, cx| {
                        cx.emit(BufferStoreEvent::BufferDropped(buffer.remote_id()));
                    })
                    .ok();
            })
            .detach();
        });

        match self.opened_buffers.entry(buffer_id) {
            hash_map::Entry::Vacant(entry) => {
                entry.insert(open_buffer);
            }
            hash_map::Entry::Occupied(mut entry) => {
                if entry.get().upgrade().is_some() {
                    debug_panic!("buffer {buffer_id} was already registered");
                    anyhow::bail!("buffer {buffer_id} was already registered");
                }
                entry.insert(open_buffer);
            }
        }

        if let Some(path) = path {
            self.path_to_buffer_id.insert(path, buffer_id);
        }

        cx.subscribe(&buffer_entity, |this, buffer, event, cx| {
            this.on_buffer_event(&buffer, event, cx);
        })
        .detach();
        cx.emit(BufferStoreEvent::BufferAdded(buffer_entity));
        Ok(())
    }

    fn worktree_entries_changed(
        buffer_store: &mut BufferStore,
        worktree_handle: &Entity<Worktree>,
        changes: &[(Arc<RelPath>, ProjectEntryId, PathChange)],
        cx: &mut Context<BufferStore>,
    ) {
        let snapshot = worktree_handle.read(cx).snapshot();

        for (path, entry_id, _) in changes {
            buffer_store.worktree_entry_changed(*entry_id, path, worktree_handle, &snapshot, cx);
        }
    }

    fn worktree_entry_changed(
        &mut self,
        entry_id: ProjectEntryId,
        path: &Arc<RelPath>,
        worktree: &Entity<Worktree>,
        snapshot: &Snapshot,
        cx: &mut Context<BufferStore>,
    ) -> Option<()> {
        let project_path = ProjectPath {
            worktree_id: snapshot.id(),
            path: path.clone(),
        };

        let buffer_id = self
            .buffer_ids_by_entry_id
            .get(&entry_id)
            .copied()
            .or_else(|| self.path_to_buffer_id.get(&project_path).copied())?;

        let Some(buffer) = self.get(buffer_id) else {
            self.opened_buffers.remove(&buffer_id);
            self.path_to_buffer_id.remove(&project_path);
            self.buffer_ids_by_entry_id.remove(&entry_id);
            return None;
        };

        let events = buffer.update(cx, |buffer, cx| {
            let old_file = buffer.file().cloned();
            let old_file = File::from_dyn(old_file.as_ref())?;
            if old_file.worktree != *worktree {
                return None;
            }

            let snapshot_entry = old_file
                .entry_id
                .and_then(|entry_id| snapshot.entry_for_id(entry_id))
                .or_else(|| snapshot.entry_for_path(old_file.path.as_ref()));

            let new_file = Arc::new(match snapshot_entry {
                Some(entry) => File {
                    disk_state: match entry.mtime {
                        Some(mtime) => DiskState::Present {
                            mtime,
                            size: entry.size,
                        },
                        None => old_file.disk_state,
                    },
                    entry_id: Some(entry.id),
                    path: entry.path.clone(),
                    worktree: worktree.clone(),
                },
                None => File {
                    disk_state: DiskState::Deleted,
                    entry_id: old_file.entry_id,
                    path: old_file.path.clone(),
                    worktree: worktree.clone(),
                },
            });

            if new_file.as_ref() == old_file {
                return None;
            }

            let mut events = Vec::new();
            if new_file.path != old_file.path {
                self.path_to_buffer_id.remove(&ProjectPath {
                    worktree_id: old_file.worktree_id(cx),
                    path: old_file.path.clone(),
                });
                self.path_to_buffer_id.insert(
                    ProjectPath {
                        worktree_id: new_file.worktree_id(cx),
                        path: new_file.path.clone(),
                    },
                    buffer_id,
                );
                events.push(BufferStoreEvent::BufferChangedFilePath {
                    buffer: cx.entity(),
                    old_file: buffer.file().cloned(),
                });
            }

            if new_file.entry_id != old_file.entry_id {
                if let Some(entry_id) = old_file.entry_id {
                    self.buffer_ids_by_entry_id.remove(&entry_id);
                }
                if let Some(entry_id) = new_file.entry_id {
                    self.buffer_ids_by_entry_id.insert(entry_id, buffer_id);
                }
            }

            buffer.file_updated(new_file, cx);
            Some(events)
        })?;

        for event in events {
            cx.emit(event);
        }

        None
    }

    fn on_buffer_event(
        &mut self,
        buffer: &Entity<Buffer>,
        event: &BufferEvent,
        cx: &mut Context<Self>,
    ) {
        if event == &BufferEvent::FileHandleChanged {
            self.buffer_changed_file(buffer, cx);
        }
    }

    fn buffer_changed_file(
        &mut self,
        buffer: &Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Option<()> {
        let (buffer_id, worktree_id, path, entry_id) = {
            let buffer = buffer.read(cx);
            let file = File::from_dyn(buffer.file())?;
            (
                buffer.remote_id(),
                file.worktree_id(cx),
                file.path.clone(),
                file.entry_id,
            )
        };

        if let Some(entry_id) = entry_id {
            if self.buffer_ids_by_entry_id.contains_key(&entry_id) {
                return None;
            }
            self.buffer_ids_by_entry_id.insert(entry_id, buffer_id);

            self.path_to_buffer_id
                .insert(ProjectPath { worktree_id, path }, buffer_id);
        }

        Some(())
    }

    fn subscribe_to_worktree(worktree: &Entity<Worktree>, cx: &mut Context<BufferStore>) {
        cx.subscribe(worktree, |this, worktree, event: &WorktreeEvent, cx| {
            if let WorktreeEvent::UpdatedEntries(changes) = event {
                Self::worktree_entries_changed(this, &worktree, changes, cx);
            }
        })
        .detach();
    }

    fn load_buffer(
        path: Arc<RelPath>,
        worktree: Entity<Worktree>,
        cx: &mut Context<BufferStore>,
    ) -> Task<anyhow::Result<Entity<Buffer>>> {
        let load_file = worktree.update(cx, |worktree, cx| worktree.load_file(path.as_ref(), cx));
        cx.spawn(async move |this, cx| {
            let path = path.clone();
            let buffer = match load_file.await {
                Ok(loaded) => {
                    let reservation = cx.reserve_entity::<Buffer>();
                    let buffer_id = BufferId::from(reservation.entity_id().as_non_zero_u64());
                    let text_buffer = cx
                        .background_spawn(async move {
                            text::Buffer::new(ReplicaId::LOCAL, buffer_id, loaded.text)
                        })
                        .await;
                    let file: Arc<dyn language::File> = loaded.file;
                    cx.insert_entity(reservation, |_| {
                        Buffer::build(text_buffer, Some(file), Capability::ReadWrite)
                    })
                }
                Err(error) if is_not_found_error(&error) => cx.new(|cx| {
                    let buffer_id = BufferId::from(cx.entity_id().as_non_zero_u64());
                    let text_buffer = text::Buffer::new(ReplicaId::LOCAL, buffer_id, "");
                    let file: Arc<dyn language::File> = Arc::new(File {
                        worktree,
                        path,
                        disk_state: DiskState::New,
                        entry_id: None,
                    });
                    Buffer::build(text_buffer, Some(file), Capability::ReadWrite)
                }),
                Err(error) => return Err(error),
            };

            this.update(cx, |this, cx| {
                this.add_buffer(buffer.clone(), cx)?;
                let buffer_id = buffer.read(cx).remote_id();
                let entry_id = {
                    let buffer = buffer.read(cx);
                    File::from_dyn(buffer.file()).and_then(|file| file.entry_id)
                };
                if let Some(entry_id) = entry_id {
                    this.buffer_ids_by_entry_id.insert(entry_id, buffer_id);
                }

                anyhow::Ok(())
            })??;

            Ok(buffer)
        })
    }

    fn save_buffer_at(
        buffer_handle: Entity<Buffer>,
        worktree: &Entity<Worktree>,
        path: Arc<RelPath>,
        mut has_changed_file: bool,
        cx: &mut Context<BufferStore>,
    ) -> Task<anyhow::Result<()>> {
        let buffer = buffer_handle.read(cx);
        let text = buffer.as_rope().clone();
        let line_ending = buffer.line_ending();
        let version = buffer.version();
        let file = buffer.file().cloned();
        if file
            .as_ref()
            .is_some_and(|file| file.disk_state() == DiskState::New)
        {
            has_changed_file = true;
        }

        let save = worktree.update(cx, |worktree, cx| {
            worktree.write_file(path, text, line_ending, cx)
        });

        cx.spawn(async move |_, cx| {
            let new_file = save.await?;
            let mtime = new_file.disk_state.mtime();
            buffer_handle.update(cx, |buffer, cx| {
                if has_changed_file {
                    buffer.file_updated(new_file, cx);
                }
                buffer.did_save(version.clone(), mtime, cx);
            });
            Ok(())
        })
    }
}

fn is_not_found_error(error: &anyhow::Error) -> bool {
    error
        .root_cause()
        .downcast_ref::<io::Error>()
        .is_some_and(|error| error.kind() == io::ErrorKind::NotFound)
}
