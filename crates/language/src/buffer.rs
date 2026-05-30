use futures::channel::oneshot;
use gpui::{App, AppContext, Context, EventEmitter, Task};
use std::{
    any::Any,
    cell::Cell,
    cmp, mem,
    ops::{Deref, Range},
    path::PathBuf,
    sync::Arc,
    time::Instant,
};
use text::{Buffer as TextBuffer, LineEnding, ReplicaId, ToOffset, Transaction, TransactionId};

use fs::MTime;
use settings::WorktreeId;
use util::{path::PathStyle, rel_path::RelPath};

use crate::text_diff::text_diff;

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum Capability {
    ReadWrite,
    Read,
    ReadOnly,
}

impl Capability {
    pub fn editable(self) -> bool {
        matches!(self, Capability::ReadWrite)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum BufferEvent {
    Edited,
    DirtyChanged,
    Saved,
    FileHandleChanged,
    CapabilityChanged,
    Reloaded,
    ReloadNeeded,
}

pub trait File: Send + Sync + Any {
    fn disk_state(&self) -> DiskState;

    fn path(&self) -> &Arc<RelPath>;

    fn abs_path(&self, cx: &App) -> PathBuf;

    fn load(&self, cx: &App) -> Task<anyhow::Result<String>>;

    fn load_bytes(&self, cx: &App) -> Task<anyhow::Result<Vec<u8>>>;

    fn full_path(&self, cx: &App) -> PathBuf;

    fn path_style(&self, cx: &App) -> PathStyle;

    fn file_name<'a>(&'a self, cx: &'a App) -> &'a str;

    fn worktree_id(&self, cx: &App) -> WorktreeId;
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum DiskState {
    New,
    Present { mtime: MTime, size: u64 },
    Deleted,
}

impl DiskState {
    pub fn mtime(self) -> Option<MTime> {
        match self {
            DiskState::Present { mtime, .. } => Some(mtime),
            DiskState::New | DiskState::Deleted => None,
        }
    }

    pub fn is_deleted(&self) -> bool {
        matches!(self, DiskState::Deleted)
    }
}

#[derive(Debug, Clone)]
pub struct Diff {
    pub base_version: clock::Global,
    pub line_ending: LineEnding,
    pub edits: Vec<(Range<usize>, Arc<str>)>,
}

pub struct Buffer {
    text: TextBuffer,
    file: Option<Arc<dyn File>>,
    saved_mtime: Option<MTime>,
    saved_version: clock::Global,
    transaction_depth: usize,
    was_dirty_before_starting_transaction: Option<bool>,
    capability: Capability,
    has_conflict: bool,
    has_unsaved_edits: Cell<(clock::Global, bool)>,
    reload_task: Option<Task<anyhow::Result<()>>>,
}

impl EventEmitter<BufferEvent> for Buffer {}

impl Buffer {
    pub fn local<T: Into<String>>(base_text: T, cx: &Context<Self>) -> Self {
        Self::build(
            TextBuffer::new(
                ReplicaId::LOCAL,
                cx.entity_id().as_non_zero_u64().into(),
                base_text.into(),
            ),
            None,
            Capability::ReadWrite,
        )
    }

    pub fn build(buffer: TextBuffer, file: Option<Arc<dyn File>>, capability: Capability) -> Self {
        let saved_mtime = file.as_ref().and_then(|file| file.disk_state().mtime());
        let saved_version = buffer.version();
        Self {
            text: buffer,
            file,
            saved_mtime,
            saved_version: saved_version.clone(),
            transaction_depth: 0,
            was_dirty_before_starting_transaction: None,
            capability,
            has_conflict: false,
            has_unsaved_edits: Cell::new((saved_version, false)),
            reload_task: None,
        }
    }

    pub fn capability(&self) -> Capability {
        self.capability
    }

    pub fn read_only(&self) -> bool {
        !self.capability.editable()
    }

    pub fn set_capability(&mut self, capability: Capability, cx: &mut Context<Self>) {
        if self.capability != capability {
            self.capability = capability;
            cx.emit(BufferEvent::CapabilityChanged);
        }
    }

    pub fn file(&self) -> Option<&Arc<dyn File>> {
        self.file.as_ref()
    }

    pub fn saved_version(&self) -> &clock::Global {
        &self.saved_version
    }

    pub fn saved_mtime(&self) -> Option<MTime> {
        self.saved_mtime
    }

    pub fn did_save(
        &mut self,
        version: clock::Global,
        mtime: Option<MTime>,
        cx: &mut Context<Self>,
    ) {
        self.saved_version.clone_from(&version);
        self.has_unsaved_edits.set((version, false));
        self.has_conflict = false;
        self.saved_mtime = mtime;
        cx.emit(BufferEvent::Saved);
        cx.notify();
    }

    pub fn did_reload(
        &mut self,
        version: clock::Global,
        line_ending: LineEnding,
        mtime: Option<MTime>,
        cx: &mut Context<Self>,
    ) {
        self.saved_version = version;
        self.has_unsaved_edits
            .set((self.saved_version.clone(), false));
        self.text.set_line_ending(line_ending);
        self.saved_mtime = mtime;
        cx.emit(BufferEvent::Reloaded);
        cx.notify();
    }

    pub fn has_unsaved_edits(&self) -> bool {
        let (last_version, has_unsaved_edits) = self.has_unsaved_edits.take();

        if last_version == self.version {
            self.has_unsaved_edits
                .set((last_version, has_unsaved_edits));
            return has_unsaved_edits;
        }

        let has_edits = self.has_edits_since(&self.saved_version);
        self.has_unsaved_edits
            .set((self.version.clone(), has_edits));
        has_edits
    }

    pub fn is_dirty(&self) -> bool {
        if self.capability == Capability::ReadOnly {
            return false;
        }
        if self.has_conflict {
            return true;
        }
        match self.file.as_ref().map(|file| file.disk_state()) {
            Some(DiskState::New | DiskState::Deleted) => {
                !self.is_empty() && self.has_unsaved_edits()
            }
            _ => self.has_unsaved_edits(),
        }
    }

    pub fn set_conflict(&mut self) {
        self.has_conflict = true;
    }

    pub fn has_conflict(&self) -> bool {
        if self.has_conflict {
            return true;
        }
        let Some(file) = self.file.as_ref() else {
            return false;
        };
        match file.disk_state() {
            DiskState::New | DiskState::Deleted => false,
            DiskState::Present { mtime, .. } => match self.saved_mtime {
                Some(saved_mtime) => {
                    mtime.bad_is_greater_than(saved_mtime) && self.has_unsaved_edits()
                }
                None => true,
            },
        }
    }

    pub fn file_updated(&mut self, new_file: Arc<dyn File>, cx: &mut Context<Self>) {
        let was_dirty = self.is_dirty();
        let mut file_changed = false;

        if let Some(old_file) = self.file.as_ref() {
            if new_file.path() != old_file.path() {
                file_changed = true;
            }

            let old_state = old_file.disk_state();
            let new_state = new_file.disk_state();
            if old_state != new_state {
                file_changed = true;
                if !was_dirty && matches!(new_state, DiskState::Present { .. }) {
                    cx.emit(BufferEvent::ReloadNeeded);
                }
            }
        } else {
            file_changed = true;
        }

        self.file = Some(new_file);
        if file_changed {
            if was_dirty != self.is_dirty() {
                cx.emit(BufferEvent::DirtyChanged);
            }
            cx.emit(BufferEvent::FileHandleChanged);
            cx.notify();
        }
    }

    pub fn edit<I, S, T>(&mut self, edits_iter: I, cx: &mut Context<Self>) -> Option<clock::Lamport>
    where
        I: IntoIterator<Item = (Range<S>, T)>,
        S: ToOffset,
        T: Into<Arc<str>>,
    {
        if self.read_only() {
            return None;
        }

        let mut edits: Vec<(Range<usize>, Arc<str>)> = Vec::new();
        for (range, new_text) in edits_iter {
            let mut range = range.start.to_offset(self)..range.end.to_offset(self);
            if range.start > range.end {
                mem::swap(&mut range.start, &mut range.end);
            }

            let new_text = new_text.into();
            if !new_text.is_empty() || !range.is_empty() {
                let previous_edit = edits.last_mut();
                let should_coalesce = previous_edit
                    .as_ref()
                    .is_some_and(|(previous_range, _)| previous_range.end >= range.start);

                if let Some((previous_range, previous_text)) = previous_edit
                    && should_coalesce
                {
                    previous_range.end = cmp::max(previous_range.end, range.end);
                    *previous_text = format!("{previous_text}{new_text}").into();
                } else {
                    edits.push((range, new_text));
                }
            }
        }

        if edits.is_empty() {
            return None;
        }

        self.start_transaction();
        let edit_operation = self.text.edit(edits.iter().cloned());
        let edit_id = edit_operation.timestamp();
        self.end_transaction(cx);
        Some(edit_id)
    }

    pub fn start_transaction(&mut self) -> Option<TransactionId> {
        self.start_transaction_at(Instant::now())
    }

    pub fn start_transaction_at(&mut self, now: Instant) -> Option<TransactionId> {
        self.transaction_depth += 1;
        if self.was_dirty_before_starting_transaction.is_none() {
            self.was_dirty_before_starting_transaction = Some(self.is_dirty());
        }
        self.text.start_transaction_at(now)
    }

    pub fn end_transaction(&mut self, cx: &mut Context<Self>) -> Option<TransactionId> {
        self.end_transaction_at(Instant::now(), cx)
    }

    pub fn end_transaction_at(
        &mut self,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> Option<TransactionId> {
        assert!(self.transaction_depth > 0);
        self.transaction_depth -= 1;
        let was_dirty = if self.transaction_depth == 0 {
            self.was_dirty_before_starting_transaction.take().unwrap()
        } else {
            false
        };

        if let Some((transaction_id, start_version)) = self.text.end_transaction_at(now) {
            self.did_edit(&start_version, was_dirty, cx);
            Some(transaction_id)
        } else {
            None
        }
    }

    pub fn undo(&mut self, cx: &mut Context<Self>) -> Option<TransactionId> {
        let was_dirty = self.is_dirty();
        let old_version = self.version.clone();

        if let Some((transaction_id, _)) = self.text.undo() {
            self.did_edit(&old_version, was_dirty, cx);
            Some(transaction_id)
        } else {
            None
        }
    }

    pub fn redo(&mut self, cx: &mut Context<Self>) -> Option<TransactionId> {
        let was_dirty = self.is_dirty();
        let old_version = self.version.clone();

        if let Some((transaction_id, _)) = self.text.redo() {
            self.did_edit(&old_version, was_dirty, cx);
            Some(transaction_id)
        } else {
            None
        }
    }

    pub fn finalize_last_transaction(&mut self) -> Option<&Transaction> {
        self.text.finalize_last_transaction()
    }

    pub fn forget_transaction(&mut self, transaction_id: TransactionId) -> Option<Transaction> {
        self.text.forget_transaction(transaction_id)
    }

    pub fn group_until_transaction(&mut self, transaction_id: TransactionId) {
        self.text.group_until_transaction(transaction_id);
    }

    pub fn reload(&mut self, cx: &Context<Self>) -> oneshot::Receiver<Option<Transaction>> {
        let (tx, rx) = oneshot::channel();
        let prev_version = self.text.version();

        self.reload_task = Some(cx.spawn(async move |this, cx| {
            let Some((new_mtime, load_file)) = this.update(cx, |this, cx| {
                let file = this.file.as_ref()?;
                Some((file.disk_state().mtime(), file.load(cx)))
            })?
            else {
                return anyhow::Ok(());
            };

            let new_text = load_file.await?;
            let diff = this.update(cx, |this, cx| this.diff(new_text, cx))?.await;
            this.update(cx, |this, cx| {
                if this.version() == diff.base_version {
                    this.finalize_last_transaction();
                    this.apply_diff(diff, cx);
                    let transaction = this.finalize_last_transaction().cloned();
                    tx.send(transaction).ok();
                    this.has_conflict = false;
                    this.did_reload(this.version(), this.line_ending(), new_mtime, cx);
                } else {
                    if !diff.edits.is_empty()
                        || this
                            .edits_since::<usize>(&diff.base_version)
                            .next()
                            .is_some()
                    {
                        this.has_conflict = true;
                    }

                    this.did_reload(prev_version, this.line_ending(), this.saved_mtime, cx);
                }

                this.reload_task.take();
            })?;
            anyhow::Ok(())
        }));

        rx
    }

    pub fn diff<T>(&self, new_text: T, cx: &App) -> Task<Diff>
    where
        T: AsRef<str> + Send + 'static,
    {
        let old_text = self.as_rope().clone();
        let base_version = self.version();
        cx.background_spawn(async move {
            let old_text = old_text.to_string();
            let mut new_text = new_text.as_ref().to_owned();
            let line_ending = LineEnding::detect(&new_text);
            LineEnding::normalize(&mut new_text);
            let edits = text_diff(&old_text, &new_text);
            Diff {
                base_version,
                line_ending,
                edits,
            }
        })
    }

    pub fn apply_diff(&mut self, diff: Diff, cx: &mut Context<Self>) -> Option<TransactionId> {
        let snapshot = self.snapshot().clone();
        let mut edits_since = snapshot.edits_since::<usize>(&diff.base_version).peekable();
        let mut delta = 0isize;
        let adjusted_edits = diff.edits.into_iter().filter_map(|(range, new_text)| {
            while let Some(edit_since) = edits_since.peek() {
                if edit_since.old.start > range.end {
                    break;
                } else if edit_since.old.end < range.start {
                    let new_len = isize::try_from(edit_since.new_len()).ok()?;
                    let old_len = isize::try_from(edit_since.old_len()).ok()?;
                    delta = delta.checked_add(new_len.checked_sub(old_len)?)?;
                    edits_since.next();
                } else {
                    return None;
                }
            }

            let start = range.start.checked_add_signed(delta)?;
            let end = range.end.checked_add_signed(delta)?;
            Some((start..end, new_text))
        });

        self.start_transaction();
        self.text.set_line_ending(diff.line_ending);
        self.edit(adjusted_edits, cx);
        self.end_transaction(cx)
    }

    fn did_edit(&mut self, old_version: &clock::Global, was_dirty: bool, cx: &mut Context<Self>) {
        if self.edits_since::<usize>(old_version).next().is_none() {
            return;
        }

        cx.emit(BufferEvent::Edited);
        let is_dirty = self.is_dirty();
        if was_dirty != is_dirty {
            cx.emit(BufferEvent::DirtyChanged);
        }
        cx.notify();
    }
}

impl Deref for Buffer {
    type Target = TextBuffer;

    fn deref(&self) -> &Self::Target {
        &self.text
    }
}
