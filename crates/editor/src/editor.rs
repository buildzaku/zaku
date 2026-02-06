mod actions;
mod element;

pub use actions::*;
pub use element::EditorElement;

use gpui::{
    App, Bounds, ClipboardItem, Context, Entity, EntityInputHandler, EventEmitter, FocusHandle,
    Focusable, Hsla, KeyBinding, KeyContext, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Point, Render, ShapedLine, SharedString, Subscription, TextStyle, UTF16Selection, WeakEntity,
    Window, prelude::*,
};
use std::{
    any::TypeId,
    collections::{HashMap, VecDeque},
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};
use text::{Buffer as TextBuffer, BufferId, BufferSnapshot, OffsetUtf16, ReplicaId, TransactionId};
use unicode_segmentation::UnicodeSegmentation;

use input::{ERASED_EDITOR_FACTORY, ErasedEditor, ErasedEditorEvent};
use theme::ActiveTheme;

pub(crate) const KEY_CONTEXT: &str = "Editor";
static NEXT_BUFFER_ID: AtomicU64 = AtomicU64::new(1);

/// Addons allow storing per-editor state in other crates (e.g. Vim).
pub trait Addon: 'static {
    fn extend_key_context(&self, _: &mut KeyContext, _: &App) {}

    fn to_any(&self) -> &dyn std::any::Any;

    fn to_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        None
    }
}

fn next_buffer_id() -> BufferId {
    let id = NEXT_BUFFER_ID.fetch_add(1, Ordering::Relaxed);
    BufferId::new(id).expect("BufferId to be non-zero")
}

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, Some(KEY_CONTEXT)),
        KeyBinding::new("delete", Delete, Some(KEY_CONTEXT)),
        KeyBinding::new("left", MoveLeft, Some(KEY_CONTEXT)),
        KeyBinding::new("right", MoveRight, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-left", SelectLeft, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-right", SelectRight, Some(KEY_CONTEXT)),
        KeyBinding::new(
            "home",
            MoveToBeginningOfLine {
                stop_at_soft_wraps: true,
                stop_at_indent: true,
            },
            Some(KEY_CONTEXT),
        ),
        KeyBinding::new(
            "end",
            MoveToEndOfLine {
                stop_at_soft_wraps: true,
            },
            Some(KEY_CONTEXT),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new("alt-left", MoveToPreviousWordStart, Some(KEY_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-left", MoveToPreviousWordStart, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("alt-right", MoveToNextWordEnd, Some(KEY_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-right", MoveToNextWordEnd, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "alt-shift-left",
            SelectToPreviousWordStart,
            Some(KEY_CONTEXT),
        ),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-shift-left",
            SelectToPreviousWordStart,
            Some(KEY_CONTEXT),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new("alt-shift-right", SelectToNextWordEnd, Some(KEY_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-right", SelectToNextWordEnd, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "ctrl-alt-left",
            MoveToPreviousSubwordStart,
            Some(KEY_CONTEXT),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new("ctrl-alt-right", MoveToNextSubwordEnd, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("ctrl-alt-b", MoveToPreviousSubwordStart, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("ctrl-alt-f", MoveToNextSubwordEnd, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "ctrl-alt-shift-left",
            SelectToPreviousSubwordStart,
            Some(KEY_CONTEXT),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "ctrl-alt-shift-right",
            SelectToNextSubwordEnd,
            Some(KEY_CONTEXT),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "ctrl-alt-shift-b",
            SelectToPreviousSubwordStart,
            Some(KEY_CONTEXT),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "ctrl-alt-shift-f",
            SelectToNextSubwordEnd,
            Some(KEY_CONTEXT),
        ),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-alt-left",
            MoveToPreviousSubwordStart,
            Some(KEY_CONTEXT),
        ),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-alt-right", MoveToNextSubwordEnd, Some(KEY_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-alt-shift-left",
            SelectToPreviousSubwordStart,
            Some(KEY_CONTEXT),
        ),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-alt-shift-right",
            SelectToNextSubwordEnd,
            Some(KEY_CONTEXT),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "cmd-backspace",
            DeleteToBeginningOfLine::default(),
            Some(KEY_CONTEXT),
        ),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-backspace",
            DeleteToPreviousWordStart::default(),
            Some(KEY_CONTEXT),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-delete", DeleteToEndOfLine, Some(KEY_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-delete",
            DeleteToNextWordEnd::default(),
            Some(KEY_CONTEXT),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "cmd-left",
            MoveToBeginningOfLine {
                stop_at_soft_wraps: true,
                stop_at_indent: true,
            },
            Some(KEY_CONTEXT),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "cmd-right",
            MoveToEndOfLine {
                stop_at_soft_wraps: true,
            },
            Some(KEY_CONTEXT),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "cmd-shift-left",
            SelectToBeginningOfLine {
                stop_at_soft_wraps: true,
                stop_at_indent: true,
            },
            Some(KEY_CONTEXT),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "cmd-shift-right",
            SelectToEndOfLine {
                stop_at_soft_wraps: true,
            },
            Some(KEY_CONTEXT),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-up", MoveToBeginning, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-down", MoveToEnd, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "alt-backspace",
            DeleteToPreviousWordStart::default(),
            Some(KEY_CONTEXT),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "ctrl-w",
            DeleteToPreviousWordStart::default(),
            Some(KEY_CONTEXT),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "alt-delete",
            DeleteToNextWordEnd::default(),
            Some(KEY_CONTEXT),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-a", SelectAll, Some(KEY_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-a", SelectAll, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-c", Copy, Some(KEY_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-c", Copy, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-x", Cut, Some(KEY_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-x", Cut, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-v", Paste, Some(KEY_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-v", Paste, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-z", Undo, Some(KEY_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-z", Undo, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-u", UndoSelection, Some(KEY_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-u", UndoSelection, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-z", Redo, Some(KEY_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-y", Redo, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-u", RedoSelection, Some(KEY_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-u", RedoSelection, Some(KEY_CONTEXT)),
    ]);

    _ = ERASED_EDITOR_FACTORY.set(|window, cx| {
        Arc::new(ErasedEditorImpl(
            cx.new(|cx| Editor::single_line(window, cx)),
        )) as Arc<dyn ErasedEditor>
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorEvent {
    BufferEdited,
    Blurred,
}

const MAX_SELECTION_HISTORY_LEN: usize = 1024;

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum EditorMode {
    SingleLine,
    AutoHeight {
        min_lines: usize,
        max_lines: Option<usize>,
    },
    Full {
        scale_ui_elements_with_buffer_font_size: bool,
        show_active_line_background: bool,
        sizing_behavior: SizingBehavior,
    },
    Minimap {
        parent: WeakEntity<Editor>,
    },
}

impl EditorMode {
    pub fn full() -> Self {
        Self::Full {
            scale_ui_elements_with_buffer_font_size: true,
            show_active_line_background: true,
            sizing_behavior: SizingBehavior::Default,
        }
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        matches!(self, Self::Full { .. })
    }

    #[inline]
    pub fn is_single_line(&self) -> bool {
        matches!(self, Self::SingleLine { .. })
    }
}

#[derive(Copy, Clone, Default, PartialEq, Eq, Debug)]
pub enum SizingBehavior {
    #[default]
    Default,
    ExcludeOverscrollMargin,
    SizeByContent,
}

#[derive(Clone)]
pub struct EditorStyle {
    pub background: Hsla,
    pub text: TextStyle,
}

impl Default for EditorStyle {
    fn default() -> Self {
        Self {
            background: Hsla::transparent_black(),
            text: TextStyle::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SelectionState {
    range: Range<usize>,
    reversed: bool,
}

#[derive(Clone, Debug)]
struct SelectionHistoryEntry {
    state: SelectionState,
}

#[derive(Clone, Debug)]
struct SelectionTransactionEntry {
    before: SelectionState,
    after: SelectionState,
}

#[derive(Copy, Clone, Default, Debug, PartialEq, Eq)]
enum SelectionHistoryMode {
    #[default]
    Normal,
    Undoing,
    Redoing,
}

#[derive(Default)]
struct SelectionHistory {
    selections_by_transaction: HashMap<TransactionId, SelectionTransactionEntry>,
    mode: SelectionHistoryMode,
    undo_stack: VecDeque<SelectionHistoryEntry>,
    redo_stack: VecDeque<SelectionHistoryEntry>,
}

impl SelectionHistory {
    fn new(initial: SelectionState) -> Self {
        let mut history = Self::default();
        history.push(SelectionHistoryEntry { state: initial });
        history
    }

    fn push(&mut self, entry: SelectionHistoryEntry) {
        match self.mode {
            SelectionHistoryMode::Normal => {
                self.push_undo(entry);
                self.redo_stack.clear();
            }
            SelectionHistoryMode::Undoing => self.push_redo(entry),
            SelectionHistoryMode::Redoing => self.push_undo(entry),
        }
    }

    fn push_undo(&mut self, entry: SelectionHistoryEntry) {
        let should_push = self
            .undo_stack
            .back()
            .is_none_or(|last| last.state != entry.state);
        if should_push {
            self.undo_stack.push_back(entry);
            if self.undo_stack.len() > MAX_SELECTION_HISTORY_LEN {
                self.undo_stack.pop_front();
            }
        }
    }

    fn push_redo(&mut self, entry: SelectionHistoryEntry) {
        let should_push = self
            .redo_stack
            .back()
            .is_none_or(|last| last.state != entry.state);
        if should_push {
            self.redo_stack.push_back(entry);
            if self.redo_stack.len() > MAX_SELECTION_HISTORY_LEN {
                self.redo_stack.pop_front();
            }
        }
    }

    fn undo(&mut self) -> Option<SelectionState> {
        if self.undo_stack.len() <= 1 {
            return None;
        }

        let current = self.undo_stack.pop_back()?;
        self.redo_stack.push_back(current);
        self.undo_stack.back().map(|entry| entry.state.clone())
    }

    fn redo(&mut self) -> Option<SelectionState> {
        let next = self.redo_stack.pop_back()?;
        self.undo_stack.push_back(next.clone());
        Some(next.state)
    }
}

pub struct Editor {
    focus_handle: FocusHandle,
    buffer: TextBuffer,
    mode: EditorMode,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    selection_history: SelectionHistory,
    addons: HashMap<TypeId, Box<dyn Addon>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    selecting: bool,
    input_enabled: bool,
    selection_mark_mode: bool,
    masked: bool,
    word_chars: Arc<[char]>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<EditorEvent> for Editor {}

impl Editor {
    pub fn single_line(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let buffer = TextBuffer::new(ReplicaId::LOCAL, next_buffer_id(), "");
        let selected_range = 0..0;
        let selection_reversed = false;
        let selection_history = SelectionHistory::new(SelectionState {
            range: selected_range.clone(),
            reversed: selection_reversed,
        });

        let subscriptions = vec![
            cx.on_focus(&focus_handle, window, Self::on_focus),
            cx.on_blur(&focus_handle, window, Self::on_blur),
        ];

        Self {
            focus_handle,
            buffer,
            mode: EditorMode::SingleLine,
            placeholder: SharedString::default(),
            selected_range,
            selection_reversed,
            marked_range: None,
            selection_history,
            addons: HashMap::new(),
            last_layout: None,
            last_bounds: None,
            selecting: false,
            input_enabled: true,
            selection_mark_mode: false,
            masked: false,
            word_chars: Arc::from(Vec::new().into_boxed_slice()),
            _subscriptions: subscriptions,
        }
    }

    fn on_focus(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        cx.notify();
    }

    fn on_blur(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.selecting = false;
        self.marked_range = None;
        cx.emit(EditorEvent::Blurred);
        cx.notify();
    }

    fn snapshot(&self) -> BufferSnapshot {
        self.buffer.snapshot()
    }

    fn selection_state(&self) -> SelectionState {
        SelectionState {
            range: self.selected_range.clone(),
            reversed: self.selection_reversed,
        }
    }

    fn restore_selection_state(&mut self, state: &SelectionState) {
        self.selected_range = state.range.clone();
        self.selection_reversed = state.reversed;
        let text_len = self.snapshot().len();
        self.clamp_selection(text_len);
    }

    fn clamp_selection(&mut self, text_len: usize) {
        self.selected_range =
            self.selected_range.start.min(text_len)..self.selected_range.end.min(text_len);
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
    }

    fn record_selection_transaction(
        &mut self,
        transaction_id: Option<TransactionId>,
        before: SelectionState,
        after: SelectionState,
    ) {
        let Some(transaction_id) = transaction_id else {
            return;
        };

        self.selection_history
            .selections_by_transaction
            .entry(transaction_id)
            .and_modify(|entry| {
                entry.after = after.clone();
            })
            .or_insert(SelectionTransactionEntry { before, after });
    }

    fn record_selection_history(&mut self) {
        self.selection_history.push(SelectionHistoryEntry {
            state: self.selection_state(),
        });
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn char_classifier(&self) -> CharClassifier {
        CharClassifier::new(self.word_chars.clone())
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let text_len = self.snapshot().len();
        let offset = offset.min(text_len);
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        self.record_selection_history();
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }

        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }

        self.record_selection_history();
        cx.notify();
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        let text = self.snapshot().text();
        text.grapheme_indices(true)
            .rev()
            .find_map(|(index, _)| (index < offset).then_some(index))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        let text = self.snapshot().text();
        text.grapheme_indices(true)
            .find_map(|(index, _)| (index > offset).then_some(index))
            .unwrap_or(text.len())
    }

    fn previous_word_start(&self, offset: usize) -> usize {
        let text = self.snapshot().text();
        let classifier = self.char_classifier();
        let mut is_first_iteration = true;

        find_preceding_boundary_offset(&text, offset, FindRange::MultiLine, |left, right| {
            if is_first_iteration
                && classifier.is_punctuation(right)
                && !classifier.is_punctuation(left)
                && left != '\n'
            {
                is_first_iteration = false;
                return false;
            }
            is_first_iteration = false;

            (classifier.kind(left) != classifier.kind(right) && !classifier.is_whitespace(right))
                || left == '\n'
        })
    }

    fn next_word_end(&self, offset: usize) -> usize {
        let text = self.snapshot().text();
        let classifier = self.char_classifier();
        let mut is_first_iteration = true;

        find_boundary_offset(&text, offset, FindRange::MultiLine, |left, right| {
            if is_first_iteration
                && classifier.is_punctuation(left)
                && !classifier.is_punctuation(right)
                && right != '\n'
            {
                is_first_iteration = false;
                return false;
            }
            is_first_iteration = false;

            (classifier.kind(left) != classifier.kind(right) && !classifier.is_whitespace(left))
                || right == '\n'
        })
    }

    fn previous_subword_start(&self, offset: usize) -> usize {
        let text = self.snapshot().text();
        let classifier = self.char_classifier();
        find_preceding_boundary_offset(&text, offset, FindRange::MultiLine, |left, right| {
            is_subword_start(left, right, &classifier) || left == '\n'
        })
    }

    fn next_subword_end(&self, offset: usize) -> usize {
        let text = self.snapshot().text();
        let classifier = self.char_classifier();
        find_boundary_offset(&text, offset, FindRange::MultiLine, |left, right| {
            is_subword_end(left, right, &classifier) || right == '\n'
        })
    }

    fn line_indent_offset(&self) -> usize {
        let text = self.snapshot().text();
        for (index, character) in text.char_indices() {
            if !character.is_whitespace() {
                return index;
            }
        }
        text.len()
    }

    fn line_beginning_offset(&self, stop_at_indent: bool) -> usize {
        if !stop_at_indent {
            return 0;
        }

        let indent = self.line_indent_offset();
        let cursor = self.cursor_offset();
        if cursor > indent || cursor == 0 {
            indent
        } else {
            0
        }
    }

    fn line_end_offset(&self) -> usize {
        self.snapshot().len()
    }

    fn replace_range(&mut self, range: Range<usize>, new_text: &str, cx: &mut Context<Self>) {
        let sanitized = sanitize_single_line(new_text);
        let snapshot = self.snapshot();
        let current_text = snapshot.text();
        let range = clamp_range_to_char_boundaries(&current_text, range);
        let range = range.start.min(current_text.len())..range.end.min(current_text.len());
        let existing = current_text.get(range.clone()).unwrap_or("");
        if existing == sanitized {
            return;
        }

        let now = Instant::now();
        let before = self.selection_state();
        let transaction_id = self.buffer.start_transaction_at(now);
        if let Some(transaction_id) = transaction_id {
            self.selection_history
                .selections_by_transaction
                .entry(transaction_id)
                .or_insert(SelectionTransactionEntry {
                    before: before.clone(),
                    after: before.clone(),
                });
        }

        self.buffer.edit([(range.clone(), sanitized.as_str())]);
        let cursor = range.start + sanitized.len();
        self.selected_range = cursor..cursor;
        self.selection_reversed = false;
        self.marked_range = None;
        self.record_selection_history();
        let after = self.selection_state();
        let transaction_id = self.buffer.end_transaction_at(now).map(|(id, _)| id);
        self.record_selection_transaction(transaction_id, before, after);
        cx.emit(EditorEvent::BufferEdited);
        cx.notify();
    }

    fn replace_range_with_selection(
        &mut self,
        range: Range<usize>,
        new_text: &str,
        selection: Range<usize>,
        marked_range: Option<Range<usize>>,
        cx: &mut Context<Self>,
    ) {
        let sanitized = sanitize_single_line(new_text);
        let snapshot = self.snapshot();
        let current_text = snapshot.text();
        let range = clamp_range_to_char_boundaries(&current_text, range);
        let range = range.start.min(current_text.len())..range.end.min(current_text.len());
        let existing = current_text.get(range.clone()).unwrap_or("");

        if existing == sanitized {
            self.selected_range = selection;
            self.selection_reversed = false;
            self.marked_range = marked_range;
            self.record_selection_history();
            cx.notify();
            return;
        }

        let now = Instant::now();
        let before = self.selection_state();
        let transaction_id = self.buffer.start_transaction_at(now);
        if let Some(transaction_id) = transaction_id {
            self.selection_history
                .selections_by_transaction
                .entry(transaction_id)
                .or_insert(SelectionTransactionEntry {
                    before: before.clone(),
                    after: before.clone(),
                });
        }

        self.buffer.edit([(range.clone(), sanitized.as_str())]);
        self.selected_range = selection;
        self.selection_reversed = false;
        self.marked_range = marked_range;
        self.record_selection_history();
        let after = self.selection_state();
        let transaction_id = self.buffer.end_transaction_at(now).map(|(id, _)| id);
        self.record_selection_transaction(transaction_id, before, after);
        cx.emit(EditorEvent::BufferEdited);
        cx.notify();
    }

    fn replace_selection(&mut self, new_text: &str, cx: &mut Context<Self>) {
        let range = self.selected_range.clone();
        self.replace_range(range, new_text, cx);
    }

    fn text_offset_from_utf16(&self, utf16_offset: usize) -> usize {
        let snapshot = self.snapshot();
        snapshot
            .offset_utf16_to_offset(OffsetUtf16(utf16_offset))
            .min(snapshot.len())
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        let start = self.text_offset_from_utf16(range_utf16.start);
        let end = self.text_offset_from_utf16(range_utf16.end);
        start..end
    }

    fn range_to_utf16(&self, range_utf8: &Range<usize>) -> Range<usize> {
        let snapshot = self.snapshot();
        let text = snapshot.text();
        let range = clamp_range_to_char_boundaries(&text, range_utf8.clone());
        let start = snapshot
            .offset_to_offset_utf16(range.start.min(text.len()))
            .0;
        let end = snapshot.offset_to_offset_utf16(range.end.min(text.len())).0;
        start..end
    }

    fn display_offset_for_text_offset(&self, text_offset: usize) -> usize {
        let text = self.snapshot().text();
        let offset = clamp_offset_to_char_boundary(&text, text_offset.min(text.len()));
        if !self.masked {
            return offset;
        }

        text.get(..offset).unwrap_or("").chars().count()
    }

    fn text_offset_for_display_offset(&self, display_offset: usize) -> usize {
        if !self.masked {
            let text_len = self.snapshot().len();
            return display_offset.min(text_len);
        }

        let char_count = display_offset;
        let text = self.snapshot().text();
        byte_offset_from_char_count(&text, char_count)
    }

    fn display_text(&self) -> SharedString {
        let text = self.snapshot().text();
        if self.masked {
            let masked = "*".repeat(text.chars().count());
            return masked.into();
        }

        text.into()
    }

    fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        let sanitized = sanitize_single_line(text);
        let snapshot = self.snapshot();
        if snapshot.text() == sanitized {
            return;
        }

        let now = Instant::now();
        let before = self.selection_state();
        let transaction_id = self.buffer.start_transaction_at(now);
        if let Some(transaction_id) = transaction_id {
            self.selection_history
                .selections_by_transaction
                .entry(transaction_id)
                .or_insert(SelectionTransactionEntry {
                    before: before.clone(),
                    after: before.clone(),
                });
        }

        let range = 0..snapshot.len();
        self.buffer.edit([(range, sanitized.as_str())]);
        self.selected_range = sanitized.len()..sanitized.len();
        self.selection_reversed = false;
        self.marked_range = None;
        self.record_selection_history();
        let after = self.selection_state();
        let transaction_id = self.buffer.end_transaction_at(now).map(|(id, _)| id);
        self.record_selection_transaction(transaction_id, before, after);
        cx.emit(EditorEvent::BufferEdited);
        cx.notify();
    }

    fn clear(&mut self, cx: &mut Context<Self>) {
        let snapshot = self.snapshot();
        if snapshot.is_empty() {
            return;
        }

        let now = Instant::now();
        let before = self.selection_state();
        let transaction_id = self.buffer.start_transaction_at(now);
        if let Some(transaction_id) = transaction_id {
            self.selection_history
                .selections_by_transaction
                .entry(transaction_id)
                .or_insert(SelectionTransactionEntry {
                    before: before.clone(),
                    after: before.clone(),
                });
        }

        self.buffer.edit([(0..snapshot.len(), "")]);
        self.selected_range = 0..0;
        self.selection_reversed = false;
        self.marked_range = None;
        self.record_selection_history();
        let after = self.selection_state();
        let transaction_id = self.buffer.end_transaction_at(now).map(|(id, _)| id);
        self.record_selection_transaction(transaction_id, before, after);
        cx.emit(EditorEvent::BufferEdited);
        cx.notify();
    }

    fn set_placeholder_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.placeholder = SharedString::new(text);
        cx.notify();
    }

    fn set_masked(&mut self, masked: bool, cx: &mut Context<Self>) {
        if self.masked == masked {
            return;
        }

        self.masked = masked;
        cx.notify();
    }

    pub fn set_input_enabled(&mut self, input_enabled: bool) {
        self.input_enabled = input_enabled;
    }

    pub fn set_word_chars(&mut self, word_chars: impl Into<Arc<[char]>>) {
        self.word_chars = word_chars.into();
    }

    fn move_selection_to_end(&mut self, cx: &mut Context<Self>) {
        let offset = self.snapshot().len();
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        self.record_selection_history();
        cx.notify();
    }

    fn move_left(&mut self, _: &MoveLeft, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx)
        }
    }

    fn move_right(&mut self, _: &MoveRight, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.end, cx)
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.selected_range = 0..self.snapshot().len();
        self.selection_reversed = false;
        self.record_selection_history();
        cx.notify();
    }

    fn move_to_beginning_of_line(
        &mut self,
        action: &MoveToBeginningOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.line_beginning_offset(action.stop_at_indent);
        self.move_to(offset, cx);
    }

    fn select_to_beginning_of_line(
        &mut self,
        action: &SelectToBeginningOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.line_beginning_offset(action.stop_at_indent);
        self.select_to(offset, cx);
    }

    fn delete_to_beginning_of_line(
        &mut self,
        action: &DeleteToBeginningOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            let offset = self.line_beginning_offset(action.stop_at_indent);
            let cursor = self.cursor_offset();
            if cursor == offset {
                return;
            }
            self.selected_range = offset..cursor;
            self.selection_reversed = false;
        }

        self.replace_selection("", cx);
    }

    fn move_to_end_of_line(&mut self, _: &MoveToEndOfLine, _: &mut Window, cx: &mut Context<Self>) {
        let offset = self.line_end_offset();
        self.move_to(offset, cx);
    }

    fn select_to_end_of_line(
        &mut self,
        _: &SelectToEndOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.line_end_offset();
        self.select_to(offset, cx);
    }

    fn delete_to_end_of_line(
        &mut self,
        _: &DeleteToEndOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            let cursor = self.cursor_offset();
            let end = self.line_end_offset();
            if cursor == end {
                return;
            }
            self.selected_range = cursor..end;
            self.selection_reversed = false;
        }

        self.replace_selection("", cx);
    }

    fn move_to_beginning(&mut self, _: &MoveToBeginning, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn move_to_end(&mut self, _: &MoveToEnd, _: &mut Window, cx: &mut Context<Self>) {
        let offset = self.snapshot().len();
        self.move_to(offset, cx);
    }

    fn select_to_beginning(
        &mut self,
        _: &SelectToBeginning,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_to(0, cx);
    }

    fn select_to_end(&mut self, _: &SelectToEnd, _: &mut Window, cx: &mut Context<Self>) {
        let offset = self.snapshot().len();
        self.select_to(offset, cx);
    }

    fn move_to_previous_word_start(
        &mut self,
        _: &MoveToPreviousWordStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.previous_word_start(self.cursor_offset());
        self.move_to(offset, cx);
    }

    fn move_to_previous_subword_start(
        &mut self,
        _: &MoveToPreviousSubwordStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.previous_subword_start(self.cursor_offset());
        self.move_to(offset, cx);
    }

    fn move_to_next_word_end(
        &mut self,
        _: &MoveToNextWordEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.next_word_end(self.cursor_offset());
        self.move_to(offset, cx);
    }

    fn move_to_next_subword_end(
        &mut self,
        _: &MoveToNextSubwordEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.next_subword_end(self.cursor_offset());
        self.move_to(offset, cx);
    }

    fn select_to_previous_word_start(
        &mut self,
        _: &SelectToPreviousWordStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.previous_word_start(self.cursor_offset());
        self.select_to(offset, cx);
    }

    fn select_to_next_word_end(
        &mut self,
        _: &SelectToNextWordEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.next_word_end(self.cursor_offset());
        self.select_to(offset, cx);
    }

    fn select_to_previous_subword_start(
        &mut self,
        _: &SelectToPreviousSubwordStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.previous_subword_start(self.cursor_offset());
        self.select_to(offset, cx);
    }

    fn select_to_next_subword_end(
        &mut self,
        _: &SelectToNextSubwordEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.next_subword_end(self.cursor_offset());
        self.select_to(offset, cx);
    }

    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let start = self.previous_boundary(self.cursor_offset());
            self.selected_range = start..self.cursor_offset();
        }
        self.replace_selection("", cx);
    }

    fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let end = self.next_boundary(self.cursor_offset());
            self.selected_range = self.cursor_offset()..end;
        }
        self.replace_selection("", cx);
    }

    fn delete_to_previous_word_start(
        &mut self,
        _: &DeleteToPreviousWordStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            let start = self.previous_word_start(self.cursor_offset());
            self.selected_range = start..self.cursor_offset();
        }
        self.replace_selection("", cx);
    }

    fn delete_to_previous_subword_start(
        &mut self,
        _: &DeleteToPreviousSubwordStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            let start = self.previous_subword_start(self.cursor_offset());
            self.selected_range = start..self.cursor_offset();
        }
        self.replace_selection("", cx);
    }

    fn delete_to_next_word_end(
        &mut self,
        _: &DeleteToNextWordEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            let end = self.next_word_end(self.cursor_offset());
            self.selected_range = self.cursor_offset()..end;
        }
        self.replace_selection("", cx);
    }

    fn delete_to_next_subword_end(
        &mut self,
        _: &DeleteToNextSubwordEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            let end = self.next_subword_end(self.cursor_offset());
            self.selected_range = self.cursor_offset()..end;
        }
        self.replace_selection("", cx);
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            return;
        }

        let current_text = self.snapshot().text();
        let text = current_text
            .get(self.selected_range.clone())
            .unwrap_or("")
            .to_string();
        if text.is_empty() {
            return;
        }

        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    fn cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            return;
        }

        let current_text = self.snapshot().text();
        let text = current_text
            .get(self.selected_range.clone())
            .unwrap_or("")
            .to_string();
        if text.is_empty() {
            return;
        }

        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.replace_selection("", cx);
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };

        self.replace_selection(&text, cx);
    }

    fn handle_input(&mut self, text: &str, _: &mut Window, cx: &mut Context<Self>) {
        if !self.input_enabled {
            return;
        }

        self.replace_selection(text, cx);
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        let transaction_id = self.buffer.undo().map(|(transaction_id, _)| transaction_id);
        let Some(transaction_id) = transaction_id else {
            return;
        };

        let selection_state = self
            .selection_history
            .selections_by_transaction
            .get(&transaction_id)
            .map(|entry| entry.before.clone());
        if let Some(selection_state) = selection_state.as_ref() {
            self.restore_selection_state(selection_state);
        } else {
            let text_len = self.snapshot().len();
            self.clamp_selection(text_len);
        }
        self.marked_range = None;
        cx.emit(EditorEvent::BufferEdited);
        cx.notify();
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        let transaction_id = self.buffer.redo().map(|(transaction_id, _)| transaction_id);
        let Some(transaction_id) = transaction_id else {
            return;
        };

        let selection_state = self
            .selection_history
            .selections_by_transaction
            .get(&transaction_id)
            .map(|entry| entry.after.clone());
        if let Some(selection_state) = selection_state.as_ref() {
            self.restore_selection_state(selection_state);
        } else {
            let text_len = self.snapshot().len();
            self.clamp_selection(text_len);
        }
        self.marked_range = None;
        cx.emit(EditorEvent::BufferEdited);
        cx.notify();
    }

    fn undo_selection(&mut self, _: &UndoSelection, _: &mut Window, cx: &mut Context<Self>) {
        self.selection_history.mode = SelectionHistoryMode::Undoing;
        let state = self.selection_history.undo();
        self.selection_history.mode = SelectionHistoryMode::Normal;
        if let Some(state) = state {
            self.restore_selection_state(&state);
            cx.notify();
        }
    }

    fn redo_selection(&mut self, _: &RedoSelection, _: &mut Window, cx: &mut Context<Self>) {
        self.selection_history.mode = SelectionHistoryMode::Redoing;
        let state = self.selection_history.redo();
        self.selection_history.mode = SelectionHistoryMode::Normal;
        if let Some(state) = state {
            self.restore_selection_state(&state);
            cx.notify();
        }
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        self.focus_handle.focus(window, cx);
        self.selecting = true;

        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        let text = self.snapshot().text();
        if text.is_empty() {
            return 0;
        }

        let Some(bounds) = self.last_bounds else {
            return 0;
        };
        let Some(line) = self.last_layout.as_ref() else {
            return 0;
        };

        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return text.len();
        }

        let display_index = line.closest_index_for_x(position.x - bounds.left());
        self.text_offset_for_display_offset(display_index)
    }

    pub fn key_context(&self, window: &mut Window, cx: &mut App) -> KeyContext {
        self.key_context_internal(window, cx)
    }

    fn key_context_internal(&self, _window: &mut Window, cx: &mut App) -> KeyContext {
        let mut key_context = KeyContext::new_with_defaults();
        key_context.add("Editor");
        let mode = match self.mode {
            EditorMode::SingleLine => "single_line",
            EditorMode::AutoHeight { .. } => "auto_height",
            EditorMode::Minimap { .. } => "minimap",
            EditorMode::Full { .. } => "full",
        };
        key_context.set("mode", mode);

        if self.selection_mark_mode {
            key_context.add("selection_mode");
        }

        if self.mode == EditorMode::SingleLine
            && self.selected_range.is_empty()
            && self.selected_range.end == self.snapshot().len()
        {
            key_context.add("end_of_input");
        }

        for addon in self.addons.values() {
            addon.extend_key_context(&mut key_context, cx);
        }

        key_context
    }

    pub(crate) fn create_style(&self, cx: &App) -> EditorStyle {
        let mut style = EditorStyle::default();
        let theme_colors = cx.theme().colors();
        style.background = theme_colors.editor_background;
        style.text.color = theme_colors.editor_foreground;
        style
    }

    pub fn register_addon<T: Addon>(&mut self, instance: T) {
        self.addons.insert(TypeId::of::<T>(), Box::new(instance));
    }

    pub fn unregister_addon<T: Addon>(&mut self) {
        self.addons.remove(&TypeId::of::<T>());
    }

    pub fn addon<T: Addon>(&self) -> Option<&T> {
        self.addons
            .get(&TypeId::of::<T>())
            .and_then(|addon| addon.to_any().downcast_ref())
    }

    pub fn addon_mut<T: Addon>(&mut self) -> Option<&mut T> {
        self.addons
            .get_mut(&TypeId::of::<T>())
            .and_then(|addon| addon.to_any_mut())
            .and_then(|addon| addon.downcast_mut())
    }
}

impl EntityInputHandler for Editor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        let text = self.snapshot().text();
        Some(text.get(range).unwrap_or("").to_string())
    }

    fn selected_text_range(
        &mut self,
        ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        if !ignore_disabled_input && !self.input_enabled {
            return None;
        }

        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.input_enabled {
            return;
        }

        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.replace_range(range, new_text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.input_enabled {
            return;
        }

        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        let sanitized = sanitize_single_line(new_text);
        let selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| range_from_utf16_in_text(&sanitized, range_utf16))
            .map(|new_range| range.start + new_range.start..range.start + new_range.end)
            .unwrap_or_else(|| range.start + sanitized.len()..range.start + sanitized.len());

        let marked_range = if sanitized.is_empty() {
            None
        } else {
            Some(range.start..range.start + sanitized.len())
        };

        self.replace_range_with_selection(range, &sanitized, selected_range, marked_range, cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let last_layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);

        let display_start = self.display_offset_for_text_offset(range.start);
        let display_end = self.display_offset_for_text_offset(range.end);

        Some(Bounds::from_corners(
            gpui::point(
                bounds.left() + last_layout.x_for_index(display_start),
                bounds.top(),
            ),
            gpui::point(
                bounds.left() + last_layout.x_for_index(display_end),
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let line_point = self.last_bounds?.localize(&point)?;
        let last_layout = self.last_layout.as_ref()?;

        let display_index = last_layout.index_for_x(line_point.x)?;
        let text_offset = self.text_offset_for_display_offset(display_index);
        let snapshot = self.snapshot();
        Some(
            snapshot
                .offset_to_offset_utf16(text_offset.min(snapshot.len()))
                .0,
        )
    }

    fn accepts_text_input(&self, _window: &mut Window, _cx: &mut Context<Self>) -> bool {
        self.input_enabled
    }
}

impl Render for Editor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        EditorElement::new(&cx.entity(), self.create_style(cx))
    }
}

impl Focusable for Editor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[derive(Clone)]
struct ErasedEditorImpl(Entity<Editor>);

impl ErasedEditor for ErasedEditorImpl {
    fn text(&self, cx: &App) -> String {
        self.0.read(cx).snapshot().text()
    }

    fn set_text(&self, text: &str, _: &mut Window, cx: &mut App) {
        self.0.update(cx, |editor, cx| {
            editor.set_text(text, cx);
        });
    }

    fn clear(&self, _: &mut Window, cx: &mut App) {
        self.0.update(cx, |editor, cx| editor.clear(cx));
    }

    fn set_placeholder_text(&self, text: &str, _: &mut Window, cx: &mut App) {
        self.0.update(cx, |editor, cx| {
            editor.set_placeholder_text(text, cx);
        });
    }

    fn move_selection_to_end(&self, _: &mut Window, cx: &mut App) {
        self.0.update(cx, |editor, cx| {
            editor.move_selection_to_end(cx);
        });
    }

    fn set_masked(&self, masked: bool, _: &mut Window, cx: &mut App) {
        self.0.update(cx, |editor, cx| {
            editor.set_masked(masked, cx);
        });
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.0.read(cx).focus_handle(cx)
    }

    fn subscribe(
        &self,
        mut callback: Box<dyn FnMut(ErasedEditorEvent, &mut Window, &mut App) + 'static>,
        window: &mut Window,
        cx: &mut App,
    ) -> Subscription {
        window.subscribe(&self.0, cx, move |_, event: &EditorEvent, window, cx| {
            let event = match event {
                EditorEvent::BufferEdited => ErasedEditorEvent::BufferEdited,
                EditorEvent::Blurred => ErasedEditorEvent::Blurred,
            };
            (callback)(event, window, cx);
        })
    }

    fn render(&self, _: &mut Window, cx: &App) -> gpui::AnyElement {
        let theme_colors = cx.theme().colors();
        let text_style = TextStyle {
            font_size: gpui::rems(0.875).into(),
            line_height: gpui::relative(1.2),
            color: theme_colors.editor_foreground,
            ..Default::default()
        };
        let editor_style = EditorStyle {
            background: theme_colors.editor_background,
            text: text_style,
        };
        EditorElement::new(&self.0, editor_style).into_any()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        &self.0
    }
}

fn sanitize_single_line(text: &str) -> String {
    let mut sanitized = String::with_capacity(text.len());
    for character in text.chars() {
        if character != '\n' && character != '\r' {
            sanitized.push(character);
        }
    }
    sanitized
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CharKind {
    Whitespace,
    Punctuation,
    Word,
}

#[derive(Clone, Debug)]
struct CharClassifier {
    word_chars: Arc<[char]>,
}

impl CharClassifier {
    fn new(word_chars: Arc<[char]>) -> Self {
        Self { word_chars }
    }

    fn is_word_char(&self, character: char) -> bool {
        self.word_chars.contains(&character)
    }

    fn kind(&self, character: char) -> CharKind {
        if character.is_alphanumeric() || character == '_' || self.is_word_char(character) {
            return CharKind::Word;
        }
        if character.is_whitespace() {
            return CharKind::Whitespace;
        }
        CharKind::Punctuation
    }

    fn is_whitespace(&self, character: char) -> bool {
        self.kind(character) == CharKind::Whitespace
    }

    fn is_word(&self, character: char) -> bool {
        self.kind(character) == CharKind::Word
    }

    fn is_punctuation(&self, character: char) -> bool {
        self.kind(character) == CharKind::Punctuation
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FindRange {
    SingleLine,
    MultiLine,
}

fn find_preceding_boundary_offset(
    text: &str,
    from: usize,
    find_range: FindRange,
    mut is_boundary: impl FnMut(char, char) -> bool,
) -> usize {
    let mut prev_ch = None;
    let mut offset = clamp_offset_to_char_boundary(text, from);

    for ch in text[..offset].chars().rev() {
        if find_range == FindRange::SingleLine && ch == '\n' {
            break;
        }
        if let Some(prev_ch) = prev_ch
            && is_boundary(ch, prev_ch)
        {
            break;
        }

        offset -= ch.len_utf8();
        prev_ch = Some(ch);
    }

    offset
}

fn find_boundary_offset(
    text: &str,
    from: usize,
    find_range: FindRange,
    mut is_boundary: impl FnMut(char, char) -> bool,
) -> usize {
    let mut offset = clamp_offset_to_char_boundary(text, from);
    let mut prev_ch = None;

    for ch in text[offset..].chars() {
        if find_range == FindRange::SingleLine && ch == '\n' {
            break;
        }
        if let Some(prev_ch) = prev_ch
            && is_boundary(prev_ch, ch)
        {
            break;
        }

        offset += ch.len_utf8();
        prev_ch = Some(ch);
    }

    offset
}

fn is_subword_start(left: char, right: char, classifier: &CharClassifier) -> bool {
    let is_word_start =
        classifier.kind(left) != classifier.kind(right) && !classifier.is_whitespace(right);
    let is_subword_start = classifier.is_word('-') && left == '-' && right != '-'
        || left == '_' && right != '_'
        || left.is_lowercase() && right.is_uppercase();
    is_word_start || is_subword_start
}

fn is_subword_end(left: char, right: char, classifier: &CharClassifier) -> bool {
    let is_word_end =
        classifier.kind(left) != classifier.kind(right) && !classifier.is_whitespace(left);
    is_word_end || is_subword_boundary_end(left, right, classifier)
}

fn is_subword_boundary_end(left: char, right: char, classifier: &CharClassifier) -> bool {
    classifier.is_word('-') && left != '-' && right == '-'
        || left != '_' && right == '_'
        || left.is_lowercase() && right.is_uppercase()
}

fn utf8_offset_from_utf16(text: &str, utf16_offset: usize) -> usize {
    let mut utf8_offset = 0;
    let mut utf16_count = 0;

    for character in text.chars() {
        if utf16_count >= utf16_offset {
            break;
        }
        utf16_count += character.len_utf16();
        utf8_offset += character.len_utf8();
    }

    utf8_offset
}

fn range_from_utf16_in_text(text: &str, range_utf16: &Range<usize>) -> Range<usize> {
    let start = utf8_offset_from_utf16(text, range_utf16.start);
    let end = utf8_offset_from_utf16(text, range_utf16.end);
    start..end
}

fn clamp_offset_to_char_boundary(text: &str, offset: usize) -> usize {
    let clamped = offset.min(text.len());
    if text.is_char_boundary(clamped) {
        return clamped;
    }

    text.char_indices()
        .take_while(|(index, _)| *index < clamped)
        .map(|(index, _)| index)
        .last()
        .unwrap_or(0)
}

fn clamp_range_to_char_boundaries(text: &str, range: Range<usize>) -> Range<usize> {
    let start = clamp_offset_to_char_boundary(text, range.start);
    let end = clamp_offset_to_char_boundary(text, range.end);
    if end < start { end..start } else { start..end }
}

fn byte_offset_from_char_count(text: &str, char_count: usize) -> usize {
    if char_count == 0 {
        return 0;
    }

    text.char_indices()
        .nth(char_count)
        .map(|(index, _)| index)
        .unwrap_or_else(|| text.len())
}

#[cfg(test)]
mod tests;
