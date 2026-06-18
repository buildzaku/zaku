use futures::channel::oneshot;
use gpui::{App, AppContext, Context, EventEmitter, Task};
use parking_lot::Mutex;
use std::{
    any::Any,
    cell::Cell,
    cmp, mem,
    ops::{Deref, Range},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use text::{LineEnding, ReplicaId, ToOffset, Transaction, TransactionId};

use fs::MTime;
use settings::WorktreeId;
use util::{path::PathStyle, rel_path::RelPath};

use crate::{
    HighlightId, HighlightMap, Language, PLAIN_TEXT, Rope, SyntaxMap, SyntaxMapCapture,
    SyntaxMapCaptures, SyntaxSnapshot, text_diff::text_diff,
};

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
    Reparsed,
    LanguageChanged(bool),
    Reloaded,
    ReloadNeeded,
}

pub struct BufferSnapshot {
    pub text: text::BufferSnapshot,
    pub(crate) syntax: SyntaxSnapshot,
    language: Option<Arc<Language>>,
    file: Option<Arc<dyn File>>,
    non_text_state_update_count: usize,
    pub capability: Capability,
}

impl Clone for BufferSnapshot {
    fn clone(&self) -> Self {
        Self {
            text: self.text.clone(),
            syntax: self.syntax.clone(),
            language: self.language.clone(),
            file: self.file.clone(),
            non_text_state_update_count: self.non_text_state_update_count,
            capability: self.capability,
        }
    }
}

impl Deref for BufferSnapshot {
    type Target = text::BufferSnapshot;

    fn deref(&self) -> &Self::Target {
        &self.text
    }
}

impl BufferSnapshot {
    pub fn language(&self) -> Option<&Arc<Language>> {
        self.language.as_ref()
    }

    pub fn file(&self) -> Option<&Arc<dyn File>> {
        self.file.as_ref()
    }

    pub fn non_text_state_update_count(&self) -> usize {
        self.non_text_state_update_count
    }

    fn get_highlights(&self, range: Range<usize>) -> (SyntaxMapCaptures<'_>, Vec<HighlightMap>) {
        let captures = self.syntax.captures(range, &self.text, |grammar| {
            grammar
                .highlights_config
                .as_ref()
                .map(|config| &config.query)
        });
        let highlight_maps = captures
            .grammars()
            .iter()
            .map(|grammar| grammar.highlight_map())
            .collect();
        (captures, highlight_maps)
    }

    pub fn chunks<T: ToOffset>(
        &self,
        range: Range<T>,
        language_aware: LanguageAwareStyling,
    ) -> BufferChunks<'_> {
        let range = range.start.to_offset(self)..range.end.to_offset(self);
        let mut syntax = None;
        if language_aware.tree_sitter {
            syntax = Some(self.get_highlights(range.clone()));
        }
        BufferChunks::new(self.text.as_rope(), range, syntax)
    }
}

struct BufferChunkHighlights<'a> {
    captures: SyntaxMapCaptures<'a>,
    next_capture: Option<SyntaxMapCapture<'a>>,
    stack: Vec<(usize, HighlightId)>,
    highlight_maps: Vec<HighlightMap>,
}

pub struct BufferChunks<'a> {
    range: Range<usize>,
    chunks: text::Chunks<'a>,
    highlights: Option<BufferChunkHighlights<'a>>,
}

impl<'a> BufferChunks<'a> {
    pub fn new(
        text: &'a Rope,
        range: Range<usize>,
        syntax: Option<(SyntaxMapCaptures<'a>, Vec<HighlightMap>)>,
    ) -> Self {
        let highlights = syntax.map(|(captures, highlight_maps)| BufferChunkHighlights {
            captures,
            next_capture: None,
            stack: Vec::new(),
            highlight_maps,
        });
        let chunks = text.chunks_in_range(range.clone());
        Self {
            range,
            chunks,
            highlights,
        }
    }
}

impl<'a> Iterator for BufferChunks<'a> {
    type Item = Chunk<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.range.start >= self.range.end {
            return None;
        }

        let mut next_capture_start = usize::MAX;
        if let Some(highlights) = self.highlights.as_mut() {
            while let Some((parent_capture_end, _)) = highlights.stack.last() {
                if *parent_capture_end <= self.range.start {
                    highlights.stack.pop();
                } else {
                    break;
                }
            }

            if highlights.next_capture.is_none() {
                highlights.next_capture = highlights.captures.next();
            }

            while let Some(capture) = highlights.next_capture.as_ref() {
                if self.range.start < capture.node.start_byte() {
                    next_capture_start = capture.node.start_byte();
                    break;
                }

                let capture_end = capture.node.end_byte();
                if capture_end > self.range.start
                    && let Some(highlight_id) =
                        highlights.highlight_maps[capture.grammar_index].get(capture.index)
                {
                    highlights.stack.push((capture_end, highlight_id));
                }
                highlights.next_capture = highlights.captures.next();
            }
        }

        if let Some(text::ChunkBitmaps {
            text: chunk,
            chars: chars_map,
            tabs,
            newlines,
        }) = self.chunks.peek_with_bitmaps()
        {
            let chunk_start = self.range.start;
            let mut chunk_end = self
                .chunks
                .offset()
                .saturating_add(chunk.len())
                .min(self.range.end)
                .min(next_capture_start);
            let mut highlight_id = None;
            if let Some(highlights) = self.highlights.as_ref()
                && let Some((parent_capture_end, parent_highlight_id)) = highlights.stack.last()
            {
                chunk_end = chunk_end.min(*parent_capture_end);
                highlight_id = Some(*parent_highlight_id);
            }

            let bit_start = chunk_start - self.chunks.offset();
            let split_index = chunk_end - self.chunks.offset();
            let bit_end = split_index;
            let slice = &chunk[bit_start..bit_end];

            let shift = u32::try_from(bit_start).expect("chunk bit start should fit in u32");
            let mask_len =
                u32::try_from(bit_end - bit_start).expect("chunk bit length should fit in u32");
            let mask = 1u128.unbounded_shl(mask_len).wrapping_sub(1);
            let tabs = (tabs >> shift) & mask;
            let chars = (chars_map >> shift) & mask;
            let newlines = (newlines >> shift) & mask;

            self.range.start = self.chunks.offset() + split_index;
            if self.range.start == self.chunks.offset() + chunk.len() {
                self.chunks.next().unwrap();
            }

            Some(Chunk {
                text: slice,
                syntax_highlight_id: highlight_id,
                chars,
                tabs,
                newlines,
            })
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Chunk<'a> {
    pub text: &'a str,
    pub syntax_highlight_id: Option<HighlightId>,
    pub chars: u128,
    pub tabs: u128,
    pub newlines: u128,
}

#[derive(Clone, Copy)]
pub struct LanguageAwareStyling {
    pub tree_sitter: bool,
    pub diagnostics: bool,
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
    text: text::Buffer,
    syntax_map: Mutex<SyntaxMap>,
    language: Option<Arc<Language>>,
    file: Option<Arc<dyn File>>,
    non_text_state_update_count: usize,
    saved_mtime: Option<MTime>,
    saved_version: clock::Global,
    transaction_depth: usize,
    was_dirty_before_starting_transaction: Option<bool>,
    capability: Capability,
    has_conflict: bool,
    has_unsaved_edits: Cell<(clock::Global, bool)>,
    reload_task: Option<Task<anyhow::Result<()>>>,
    reparse: Option<Task<()>>,
    sync_parse_timeout: Option<Duration>,
}

impl EventEmitter<BufferEvent> for Buffer {}

impl Buffer {
    pub fn local<T: Into<String>>(base_text: T, cx: &Context<Self>) -> Self {
        Self::build(
            text::Buffer::new(
                ReplicaId::LOCAL,
                cx.entity_id().as_non_zero_u64().into(),
                base_text.into(),
            ),
            None,
            Capability::ReadWrite,
        )
    }

    pub fn build(
        buffer: text::Buffer,
        file: Option<Arc<dyn File>>,
        capability: Capability,
    ) -> Self {
        let saved_mtime = file.as_ref().and_then(|file| file.disk_state().mtime());
        let snapshot = buffer.snapshot();
        let saved_version = buffer.version();
        let syntax_map = Mutex::new(SyntaxMap::new(snapshot));
        Self {
            text: buffer,
            syntax_map,
            language: None,
            file,
            non_text_state_update_count: 0,
            saved_mtime,
            saved_version: saved_version.clone(),
            transaction_depth: 0,
            was_dirty_before_starting_transaction: None,
            capability,
            has_conflict: false,
            has_unsaved_edits: Cell::new((saved_version, false)),
            reload_task: None,
            reparse: None,
            sync_parse_timeout: if cfg!(any(test, feature = "test-support")) {
                Some(Duration::from_millis(10))
            } else {
                Some(Duration::from_millis(1))
            },
        }
    }

    pub fn with_language_async(mut self, language: Arc<Language>, cx: &mut Context<Self>) -> Self {
        self.set_language_async(Some(language), cx);
        self
    }

    pub fn with_language(mut self, language: Arc<Language>, cx: &mut Context<Self>) -> Self {
        self.set_language(Some(language), cx);
        self
    }

    pub fn language(&self) -> Option<&Arc<Language>> {
        self.language.as_ref()
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
            self.non_text_state_update_count += 1;
            cx.emit(BufferEvent::CapabilityChanged);
        }
    }

    pub fn file(&self) -> Option<&Arc<dyn File>> {
        self.file.as_ref()
    }

    pub fn snapshot(&self) -> BufferSnapshot {
        let text = self.text.snapshot();
        let syntax = {
            let mut syntax_map = self.syntax_map.lock();
            syntax_map.interpolate(text);
            syntax_map.snapshot()
        };

        BufferSnapshot {
            text: text.clone(),
            syntax,
            language: self.language.clone(),
            file: self.file.clone(),
            non_text_state_update_count: self.non_text_state_update_count,
            capability: self.capability,
        }
    }

    pub fn as_text_snapshot(&self) -> &text::BufferSnapshot {
        self.text.snapshot()
    }

    pub fn text_snapshot(&self) -> text::BufferSnapshot {
        self.text.snapshot().clone()
    }

    pub fn syntax_snapshot(&self) -> SyntaxSnapshot {
        self.syntax_map.lock().snapshot()
    }

    pub fn set_language_async(&mut self, language: Option<Arc<Language>>, cx: &mut Context<Self>) {
        self.set_language_inner(language, cfg!(any(test, feature = "test-support")), cx);
    }

    pub fn set_language(&mut self, language: Option<Arc<Language>>, cx: &mut Context<Self>) {
        self.set_language_inner(language, true, cx);
    }

    fn set_language_inner(
        &mut self,
        language: Option<Arc<Language>>,
        may_block: bool,
        cx: &mut Context<Self>,
    ) {
        if language == self.language {
            return;
        }

        self.syntax_map.lock().clear(self.text.snapshot());
        let old_language = mem::replace(&mut self.language, language);
        self.non_text_state_update_count += 1;
        self.reparse(cx, may_block);
        let has_fresh_language =
            self.language.is_some() && old_language.is_none_or(|old| old.id() == PLAIN_TEXT.id());
        cx.emit(BufferEvent::LanguageChanged(has_fresh_language));
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn is_parsing(&self) -> bool {
        self.reparse.is_some()
    }

    pub fn set_sync_parse_timeout(&mut self, timeout: Option<Duration>) {
        self.sync_parse_timeout = timeout;
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
            self.non_text_state_update_count += 1;
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
        let snapshot = self.snapshot();
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

        self.reparse(cx, true);
        cx.emit(BufferEvent::Edited);
        let is_dirty = self.is_dirty();
        if was_dirty != is_dirty {
            cx.emit(BufferEvent::DirtyChanged);
        }
        cx.notify();
    }

    pub fn reparse(&mut self, cx: &mut Context<Self>, may_block: bool) {
        if self.reparse.is_some() {
            return;
        }
        let Some(language) = self.language.clone() else {
            return;
        };

        let text = self.text_snapshot();
        let parsed_version = self.version();
        let mut syntax_snapshot = {
            let mut syntax_map = self.syntax_map.lock();
            syntax_map.interpolate(&text);
            syntax_map.snapshot()
        };

        if may_block
            && let Some(sync_parse_timeout) = self.sync_parse_timeout
            && syntax_snapshot
                .reparse_with_timeout(&text, language.clone(), sync_parse_timeout)
                .is_ok()
        {
            self.did_finish_parsing(syntax_snapshot, cx);
            self.reparse = None;
            return;
        }

        let parse_task = cx.background_spawn({
            let language = language.clone();
            async move {
                syntax_snapshot.reparse(&text, language);
                syntax_snapshot
            }
        });

        self.reparse = Some(cx.spawn(async move |this, cx| {
            let new_syntax_map = parse_task.await;
            if let Err(error) = this.update(cx, move |this, cx| {
                let grammar_changed = || {
                    this.language
                        .as_ref()
                        .is_none_or(|current_language| !Arc::ptr_eq(&language, current_language))
                };
                let parse_again =
                    this.version().changed_since(&parsed_version) || grammar_changed();
                this.did_finish_parsing(new_syntax_map, cx);
                this.reparse = None;
                if parse_again {
                    this.reparse(cx, false);
                }
            }) {
                log::debug!("Failed to finish parsing buffer: {error}");
            }
        }));
    }

    fn did_finish_parsing(&mut self, syntax_snapshot: SyntaxSnapshot, cx: &mut Context<Self>) {
        self.syntax_map.lock().did_parse(syntax_snapshot);
        self.non_text_state_update_count += 1;
        cx.emit(BufferEvent::Reparsed);
        cx.notify();
    }
}

impl Deref for Buffer {
    type Target = text::Buffer;

    fn deref(&self) -> &Self::Target {
        &self.text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use gpui::{AppContext, Entity, TestAppContext};

    use crate::{html_lang, json_lang};

    fn get_tree_sexp(buffer: &Entity<Buffer>, cx: &mut TestAppContext) -> String {
        buffer.update(cx, |buffer, _| {
            let snapshot = buffer.syntax_snapshot();
            let layers = snapshot.layers(buffer.as_text_snapshot());
            layers[0].node().to_sexp()
        })
    }

    #[gpui::test]
    fn test_reparse(cx: &mut TestAppContext) {
        let buffer = cx.new(|cx| Buffer::local("{}", cx).with_language(json_lang(), cx));

        cx.executor().run_until_parked();
        assert!(!buffer.update(cx, |buffer, _| buffer.is_parsing()));
        assert_eq!(get_tree_sexp(&buffer, cx), "(document (object))");

        buffer.update(cx, |buffer, _| buffer.set_sync_parse_timeout(None));

        buffer.update(cx, |buffer, cx| {
            buffer.start_transaction();

            buffer.edit([(0..1, "[")], cx);
            assert!(!buffer.is_parsing());

            let offset = buffer.text().find('}').unwrap();
            buffer.edit([(offset..offset + 1, "]")], cx);
            assert!(!buffer.is_parsing());

            buffer.end_transaction(cx);
            assert_eq!(buffer.text(), "[]");
            assert!(buffer.is_parsing());
        });
        cx.executor().run_until_parked();
        assert!(!buffer.update(cx, |buffer, _| buffer.is_parsing()));
        assert_eq!(get_tree_sexp(&buffer, cx), "(document (array))");

        buffer.update(cx, |buffer, cx| {
            let offset = buffer.text().find(']').unwrap();
            buffer.edit([(offset..offset, "{}")], cx);
            assert_eq!(buffer.text(), "[{}]");
            assert!(buffer.is_parsing());
        });
        buffer.update(cx, |buffer, cx| {
            let offset = buffer.text().find(']').unwrap();
            buffer.edit([(offset..offset, ",{}")], cx);
            assert_eq!(buffer.text(), "[{},{}]");
            assert!(buffer.is_parsing());
        });
        cx.executor().run_until_parked();
        assert_eq!(
            get_tree_sexp(&buffer, cx),
            "(document (array (object) (object)))"
        );

        buffer.update(cx, |buffer, cx| {
            assert!(buffer.undo(cx).is_some());
            assert!(buffer.undo(cx).is_some());
            assert!(buffer.undo(cx).is_some());
            assert_eq!(buffer.text(), "{}");
            assert!(buffer.is_parsing());
        });

        cx.executor().run_until_parked();
        assert_eq!(get_tree_sexp(&buffer, cx), "(document (object))");

        buffer.update(cx, |buffer, cx| {
            assert!(buffer.redo(cx).is_some());
            assert!(buffer.redo(cx).is_some());
            assert!(buffer.redo(cx).is_some());
            assert_eq!(buffer.text(), "[{},{}]");
            assert!(buffer.is_parsing());
        });
        cx.executor().run_until_parked();
        assert_eq!(
            get_tree_sexp(&buffer, cx),
            "(document (array (object) (object)))"
        );
    }

    #[gpui::test]
    fn test_resetting_language(cx: &mut TestAppContext) {
        let buffer = cx.new(|cx| {
            let mut buffer = Buffer::local("{}", cx).with_language(html_lang(), cx);
            buffer.set_sync_parse_timeout(None);
            buffer
        });

        cx.executor().run_until_parked();
        assert_eq!(get_tree_sexp(&buffer, cx), "(document (text))");

        buffer.update(cx, |buffer, cx| {
            buffer.set_language(Some(json_lang()), cx);
        });
        cx.executor().run_until_parked();
        assert_eq!(get_tree_sexp(&buffer, cx), "(document (object))");
    }

    #[gpui::test]
    fn test_formatted_chunks(cx: &mut App) {
        let buffer = cx.new(|cx| {
            Buffer::local(r#"{ "ui": { "font_size": 13 } }"#, cx).with_language(json_lang(), cx)
        });
        let snapshot = buffer.read(cx).snapshot();

        let chunks = snapshot.chunks(
            0..snapshot.len(),
            LanguageAwareStyling {
                tree_sitter: true,
                diagnostics: false,
            },
        );

        for chunk in chunks {
            let chunk_text = chunk.text;
            let character_bitmap = chunk.chars;
            let chunk_len = chunk_text.len();

            assert!(
                chunk_len <= 128,
                "Chunk text length {chunk_len} exceeds 128 bytes"
            );

            let character_indices = chunk_text
                .char_indices()
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            let character_count = u32::try_from(character_indices.len()).unwrap();
            assert_eq!(character_count, character_bitmap.count_ones());

            for byte_index in 0..chunk_text.len() {
                let should_have_bit = character_indices.contains(&byte_index);
                let has_bit = character_bitmap & (1 << byte_index) != 0;

                assert_eq!(
                    has_bit, should_have_bit,
                    "Chars bitmap mismatch at byte index {byte_index} in chunk {chunk_text:?}. Expected bit: {should_have_bit}, Got bit: {has_bit}",
                );
            }
        }
    }
}
