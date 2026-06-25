use anyhow::anyhow;
use futures::{FutureExt, future::Shared};
use gpui::{AppContext, Context, Entity, EventEmitter, Subscription, Task, WeakEntity};
use std::{
    collections::{HashMap, hash_map},
    sync::Arc,
};
use text::BufferId;

use path::RelPath;
use request_buffer::RequestBuffer;
use util::debug_panic;
use worktree::{
    DiskState, File, PathChange, ProjectEntryId, RequestFileState, Snapshot, Worktree,
    WorktreeEvent,
};

use crate::{
    ProjectPath,
    worktree_store::{WorktreeStore, WorktreeStoreEvent},
};

pub enum RequestBufferStoreEvent {
    BufferAdded(Entity<RequestBuffer>),
    BufferDropped(BufferId),
    BufferChangedFilePath {
        buffer: Entity<RequestBuffer>,
        old_file: Option<Arc<File>>,
    },
}

pub struct RequestBufferStore {
    buffer_ids_by_entry_id: HashMap<ProjectEntryId, BufferId>,
    pending_buffer_opens:
        HashMap<ProjectPath, Shared<Task<Result<Entity<RequestBuffer>, Arc<anyhow::Error>>>>>,
    worktree_store: Entity<WorktreeStore>,
    opened_buffers: HashMap<BufferId, WeakEntity<RequestBuffer>>,
    path_to_buffer_id: HashMap<ProjectPath, BufferId>,
    _worktree_store_subscription: Subscription,
}

impl RequestBufferStore {
    pub fn new(worktree_store: Entity<WorktreeStore>, cx: &mut Context<Self>) -> Self {
        let worktree_store_subscription = cx.subscribe(&worktree_store, |_, _, event, cx| {
            if let WorktreeStoreEvent::WorktreeAdded(worktree) = event {
                Self::subscribe_to_worktree(worktree, cx);
            }
        });

        Self {
            buffer_ids_by_entry_id: HashMap::default(),
            pending_buffer_opens: HashMap::default(),
            worktree_store,
            opened_buffers: HashMap::default(),
            path_to_buffer_id: HashMap::default(),
            _worktree_store_subscription: worktree_store_subscription,
        }
    }

    pub fn open_request_buffer(
        &mut self,
        project_path: ProjectPath,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Entity<RequestBuffer>>> {
        if let Some(buffer) = self.get_by_path(&project_path) {
            return Task::ready(Ok(buffer));
        }

        let open_buffer_task = match self.pending_buffer_opens.entry(project_path.clone()) {
            hash_map::Entry::Occupied(entry) => entry.get().clone(),
            hash_map::Entry::Vacant(pending_entry) => {
                let path = project_path.path.clone();
                let Some(worktree) = self
                    .worktree_store
                    .read(cx)
                    .worktree_for_id(project_path.worktree_id, cx)
                else {
                    return Task::ready(Err(anyhow!("no such worktree")));
                };

                let worktree_entry = self
                    .worktree_store
                    .read(cx)
                    .entry_for_path(&project_path, cx)
                    .cloned()
                    .ok_or_else(|| anyhow!("no such entry"));
                let worktree_entry = match worktree_entry {
                    Ok(entry) => entry,
                    Err(error) => return Task::ready(Err(error)),
                };
                if !worktree_entry.is_request {
                    return Task::ready(Err(anyhow!("Cannot open non-request file")));
                }

                let load_file_task =
                    worktree.update(cx, |worktree, cx| worktree.load_file(path.as_ref(), cx));
                let open_task = cx.spawn(async move |this, cx| {
                    let loaded = load_file_task.await?;
                    let file = loaded.file;
                    let parse_task =
                        cx.background_spawn(
                            async move { worktree::parse_request_file(&loaded.text) },
                        );
                    let request_file = parse_task.await;
                    let reservation = cx.reserve_entity::<RequestBuffer>();
                    let buffer_id = BufferId::from(reservation.entity_id().as_non_zero_u64());
                    let buffer =
                        cx.insert_entity(reservation, |_| RequestBuffer::new(file, request_file));

                    this.update(cx, |this, cx| {
                        this.add_buffer(buffer_id, buffer.clone(), cx)?;
                        if let Some(entry_id) = buffer.read(cx).file().entry_id {
                            this.buffer_ids_by_entry_id.insert(entry_id, buffer_id);
                        }

                        anyhow::Ok(())
                    })??;

                    Ok(buffer)
                });

                pending_entry
                    .insert(
                        cx.spawn(async move |this, cx| {
                            let open_result = open_task.await;
                            this.update(cx, |this, _cx| {
                                this.pending_buffer_opens.remove(&project_path);

                                let buffer = open_result.map_err(Arc::new)?;
                                Ok(buffer)
                            })?
                        })
                        .shared(),
                    )
                    .clone()
            }
        };

        cx.background_spawn(
            async move { open_buffer_task.await.map_err(|error| anyhow!("{error}")) },
        )
    }

    fn add_buffer(
        &mut self,
        buffer_id: BufferId,
        buffer_entity: Entity<RequestBuffer>,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        let path = {
            let buffer = buffer_entity.read(cx);
            let file = buffer.file();
            ProjectPath {
                worktree_id: file.worktree_id(cx),
                path: file.path.clone(),
            }
        };
        let open_buffer = buffer_entity.downgrade();

        let handle = cx.entity().downgrade();
        buffer_entity.update(cx, move |_, cx| {
            cx.on_release(move |_, cx| {
                if let Err(error) = handle.update(cx, |_, cx| {
                    cx.emit(RequestBufferStoreEvent::BufferDropped(buffer_id));
                }) {
                    log::trace!(
                        "Failed to update request buffer store after buffer drop: {error:?}"
                    );
                }
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

        self.path_to_buffer_id.insert(path, buffer_id);
        cx.emit(RequestBufferStoreEvent::BufferAdded(buffer_entity));

        Ok(())
    }

    pub fn save_request_buffer(
        &self,
        buffer: &Entity<RequestBuffer>,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        let buffer = buffer.clone();
        let (worktree, path, request_file, was_dirty) = {
            let buffer = buffer.read(cx);
            let RequestFileState::Parsed(request_file) = buffer.request_file().clone() else {
                return Task::ready(Err(anyhow!("Cannot save invalid request")));
            };
            (
                buffer.file().worktree.clone(),
                buffer.file().path.clone(),
                request_file,
                buffer.is_dirty(),
            )
        };
        let save_task = worktree.update(cx, |worktree, cx| {
            worktree.write_request_file(path, request_file, cx)
        });

        cx.spawn(async move |_, cx| {
            let new_file = save_task.await?;
            buffer.update(cx, |buffer, cx| {
                if was_dirty {
                    buffer.file_updated(new_file, cx);
                }
                buffer.did_save(cx);
            });
            anyhow::Ok(())
        })
    }

    pub fn reload_request_buffer(
        &self,
        buffer: &Entity<RequestBuffer>,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        buffer.update(cx, |buffer, cx| buffer.reload(cx))
    }

    pub fn get_by_path(&self, path: &ProjectPath) -> Option<Entity<RequestBuffer>> {
        self.path_to_buffer_id
            .get(path)
            .and_then(|buffer_id| self.get(*buffer_id))
    }

    pub fn get(&self, buffer_id: BufferId) -> Option<Entity<RequestBuffer>> {
        self.opened_buffers.get(&buffer_id)?.upgrade()
    }

    fn subscribe_to_worktree(worktree: &Entity<Worktree>, cx: &mut Context<RequestBufferStore>) {
        cx.subscribe(worktree, |this, worktree, event: &WorktreeEvent, cx| {
            if let WorktreeEvent::UpdatedEntries(changes) = event {
                Self::worktree_entries_changed(this, &worktree, changes, cx);
            }
        })
        .detach();
    }

    fn worktree_entries_changed(
        buffer_store: &mut RequestBufferStore,
        worktree_handle: &Entity<Worktree>,
        changes: &[(Arc<RelPath>, ProjectEntryId, PathChange)],
        cx: &mut Context<RequestBufferStore>,
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
        cx: &mut Context<RequestBufferStore>,
    ) {
        let project_path = ProjectPath {
            worktree_id: snapshot.id(),
            path: path.clone(),
        };

        let Some(buffer_id) = self
            .buffer_ids_by_entry_id
            .get(&entry_id)
            .copied()
            .or_else(|| self.path_to_buffer_id.get(&project_path).copied())
        else {
            return;
        };

        let Some(buffer) = self.get(buffer_id) else {
            self.opened_buffers.remove(&buffer_id);
            self.path_to_buffer_id.remove(&project_path);
            self.buffer_ids_by_entry_id.remove(&entry_id);

            return;
        };

        let events = buffer.update(cx, |buffer, cx| {
            let old_file = buffer.file().clone();
            if old_file.worktree != *worktree {
                return Vec::new();
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
                    worktree: worktree.clone(),
                    path: old_file.path.clone(),
                    disk_state: DiskState::Deleted,
                    entry_id: old_file.entry_id,
                },
            });

            if new_file == old_file {
                return Vec::new();
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
                events.push(RequestBufferStoreEvent::BufferChangedFilePath {
                    buffer: cx.entity(),
                    old_file: Some(old_file.clone()),
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
            events
        });

        for event in events {
            cx.emit(event);
        }
    }
}

impl EventEmitter<RequestBufferStoreEvent> for RequestBufferStore {}
