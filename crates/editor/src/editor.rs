use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler,
    Entity, EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId, KeyBinding,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, Render,
    ShapedLine, SharedString, Subscription, TextRun, UTF16Selection, UnderlineStyle, Window,
    actions, div, fill, point, prelude::*, px, rgb, rgba, size,
};
use text::{Buffer as TextBuffer, BufferId, BufferSnapshot, OffsetUtf16, ReplicaId, TransactionId};
use unicode_segmentation::UnicodeSegmentation;

use input::{ERASED_EDITOR_FACTORY, ErasedEditor, ErasedEditorEvent};

actions!(
    input,
    [
        Backspace,
        Copy,
        Cut,
        DeleteToBeginningOfLine,
        DeleteToEndOfLine,
        Delete,
        DeleteToNextWordEnd,
        DeleteToPreviousWordStart,
        End,
        Home,
        Left,
        MoveToNextWord,
        MoveToPreviousWord,
        MoveToBeginningOfLine,
        MoveToEndOfLine,
        Paste,
        Redo,
        Right,
        SelectAll,
        SelectLeft,
        SelectRight,
        SelectToBeginningOfLine,
        SelectToEndOfLine,
        Undo,
    ]
);

const KEY_CONTEXT: &str = "Input";
static NEXT_BUFFER_ID: AtomicU64 = AtomicU64::new(1);

fn next_buffer_id() -> BufferId {
    let id = NEXT_BUFFER_ID.fetch_add(1, Ordering::Relaxed);
    BufferId::new(id).expect("BufferId to be non-zero")
}

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, Some(KEY_CONTEXT)),
        KeyBinding::new("delete", Delete, Some(KEY_CONTEXT)),
        KeyBinding::new("left", Left, Some(KEY_CONTEXT)),
        KeyBinding::new("right", Right, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-left", SelectLeft, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-right", SelectRight, Some(KEY_CONTEXT)),
        KeyBinding::new("home", Home, Some(KEY_CONTEXT)),
        KeyBinding::new("end", End, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("alt-left", MoveToPreviousWord, Some(KEY_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-left", MoveToPreviousWord, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("alt-right", MoveToNextWord, Some(KEY_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-right", MoveToNextWord, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-backspace", DeleteToBeginningOfLine, Some(KEY_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-backspace",
            DeleteToPreviousWordStart,
            Some(KEY_CONTEXT),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-delete", DeleteToEndOfLine, Some(KEY_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-delete", DeleteToNextWordEnd, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-left", MoveToBeginningOfLine, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-right", MoveToEndOfLine, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-left", SelectToBeginningOfLine, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-right", SelectToEndOfLine, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-up", Home, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-down", End, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "alt-backspace",
            DeleteToPreviousWordStart,
            Some(KEY_CONTEXT),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new("ctrl-w", DeleteToPreviousWordStart, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("alt-delete", DeleteToNextWordEnd, Some(KEY_CONTEXT)),
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
        KeyBinding::new("cmd-shift-z", Redo, Some(KEY_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-y", Redo, Some(KEY_CONTEXT)),
    ]);

    ERASED_EDITOR_FACTORY
        .set(|window, cx| {
            Arc::new(ErasedEditorImpl(
                cx.new(|cx| Editor::single_line(window, cx)),
            )) as Arc<dyn ErasedEditor>
        })
        .expect("ErasedEditorFactory to be initialized");
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorEvent {
    BufferEdited,
    Blurred,
}

#[derive(Clone, Debug)]
struct SelectionState {
    range: Range<usize>,
    reversed: bool,
}

#[derive(Clone, Debug)]
struct SelectionHistoryEntry {
    before: SelectionState,
    after: SelectionState,
}

pub struct Editor {
    focus_handle: FocusHandle,
    buffer: TextBuffer,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    selection_history: HashMap<TransactionId, SelectionHistoryEntry>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    selecting: bool,
    masked: bool,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<EditorEvent> for Editor {}

impl Editor {
    pub fn single_line(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let buffer = TextBuffer::new(ReplicaId::LOCAL, next_buffer_id(), "");

        let subscriptions = vec![
            cx.on_focus(&focus_handle, window, Self::on_focus),
            cx.on_blur(&focus_handle, window, Self::on_blur),
        ];

        Self {
            focus_handle,
            buffer,
            placeholder: SharedString::default(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            selection_history: HashMap::new(),
            last_layout: None,
            last_bounds: None,
            selecting: false,
            masked: false,
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
            .entry(transaction_id)
            .and_modify(|entry| {
                entry.after = after.clone();
            })
            .or_insert(SelectionHistoryEntry { before, after });
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let text_len = self.snapshot().len();
        let offset = offset.min(text_len);
        self.selected_range = offset..offset;
        self.selection_reversed = false;
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
        if offset == 0 {
            return 0;
        }

        let full_text = self.snapshot().text();
        let offset = clamp_offset_to_char_boundary(&full_text, offset);
        let text = full_text.get(..offset).unwrap_or("");
        let mut segments = text.split_word_bound_indices().collect::<Vec<_>>();
        while let Some((start, segment)) = segments.pop() {
            if segment.chars().all(|character| character.is_whitespace()) {
                continue;
            }
            return start;
        }

        0
    }

    fn next_word_end(&self, offset: usize) -> usize {
        let full_text = self.snapshot().text();
        if offset >= full_text.len() {
            return full_text.len();
        }

        let offset = clamp_offset_to_char_boundary(&full_text, offset);
        let text = full_text.get(offset..).unwrap_or("");
        for (start, segment) in text.split_word_bound_indices() {
            if segment.chars().all(|character| character.is_whitespace()) {
                continue;
            }
            return offset + start + segment.len();
        }

        full_text.len()
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
                .entry(transaction_id)
                .or_insert(SelectionHistoryEntry {
                    before: before.clone(),
                    after: before.clone(),
                });
        }

        self.buffer.edit([(range.clone(), sanitized.as_str())]);
        let cursor = range.start + sanitized.len();
        self.selected_range = cursor..cursor;
        self.selection_reversed = false;
        self.marked_range = None;
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
            cx.notify();
            return;
        }

        let now = Instant::now();
        let before = self.selection_state();
        let transaction_id = self.buffer.start_transaction_at(now);
        if let Some(transaction_id) = transaction_id {
            self.selection_history
                .entry(transaction_id)
                .or_insert(SelectionHistoryEntry {
                    before: before.clone(),
                    after: before.clone(),
                });
        }

        self.buffer.edit([(range.clone(), sanitized.as_str())]);
        self.selected_range = selection;
        self.selection_reversed = false;
        self.marked_range = marked_range;
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

        let char_count = text.get(..offset).unwrap_or("").chars().count();
        char_count
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
                .entry(transaction_id)
                .or_insert(SelectionHistoryEntry {
                    before: before.clone(),
                    after: before.clone(),
                });
        }

        let range = 0..snapshot.len();
        self.buffer.edit([(range, sanitized.as_str())]);
        self.selected_range = sanitized.len()..sanitized.len();
        self.selection_reversed = false;
        self.marked_range = None;
        let after = self.selection_state();
        let transaction_id = self.buffer.end_transaction_at(now).map(|(id, _)| id);
        self.record_selection_transaction(transaction_id, before, after);
        cx.emit(EditorEvent::BufferEdited);
        cx.notify();
    }

    fn clear(&mut self, cx: &mut Context<Self>) {
        let snapshot = self.snapshot();
        if snapshot.len() == 0 {
            return;
        }

        let now = Instant::now();
        let before = self.selection_state();
        let transaction_id = self.buffer.start_transaction_at(now);
        if let Some(transaction_id) = transaction_id {
            self.selection_history
                .entry(transaction_id)
                .or_insert(SelectionHistoryEntry {
                    before: before.clone(),
                    after: before.clone(),
                });
        }

        self.buffer.edit([(0..snapshot.len(), "")]);
        self.selected_range = 0..0;
        self.selection_reversed = false;
        self.marked_range = None;
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

    fn move_selection_to_end(&mut self, cx: &mut Context<Self>) {
        let offset = self.snapshot().len();
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        cx.notify();
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx)
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
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
        cx.notify();
    }

    fn move_to_beginning_of_line(
        &mut self,
        _: &MoveToBeginningOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.line_beginning_offset(true);
        self.move_to(offset, cx);
    }

    fn select_to_beginning_of_line(
        &mut self,
        _: &SelectToBeginningOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.line_beginning_offset(true);
        self.select_to(offset, cx);
    }

    fn delete_to_beginning_of_line(
        &mut self,
        _: &DeleteToBeginningOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            let cursor = self.cursor_offset();
            if cursor == 0 {
                return;
            }
            self.selected_range = 0..cursor;
            self.selection_reversed = false;
        }

        self.replace_selection("", cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        let offset = self.line_beginning_offset(true);
        self.move_to(offset, cx);
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

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        let offset = self.line_end_offset();
        self.move_to(offset, cx);
    }

    fn move_to_previous_word(
        &mut self,
        _: &MoveToPreviousWord,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.previous_word_start(self.cursor_offset());
        self.move_to(offset, cx);
    }

    fn move_to_next_word(&mut self, _: &MoveToNextWord, _: &mut Window, cx: &mut Context<Self>) {
        let offset = self.next_word_end(self.cursor_offset());
        self.move_to(offset, cx);
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

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        let transaction_id = self.buffer.undo().map(|(transaction_id, _)| transaction_id);
        let Some(transaction_id) = transaction_id else {
            return;
        };

        let selection_state = self
            .selection_history
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
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
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
            point(
                bounds.left() + last_layout.x_for_index(display_start),
                bounds.top(),
            ),
            point(
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
}

struct TextElement {
    editor: Entity<Editor>,
}

struct PrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (gpui::LayoutId, Self::RequestLayoutState) {
        let mut style = gpui::Style::default();
        style.size.width = gpui::relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let editor = self.editor.read(cx);
        let snapshot = editor.snapshot();
        let content = snapshot.text();
        let selected_range = editor.selected_range.clone();
        let cursor_offset = editor.cursor_offset();
        let style = window.text_style();

        let (display_text, text_color) = if content.is_empty() {
            (editor.placeholder.clone(), rgb(0x8a8a8a).into())
        } else {
            (editor.display_text(), style.color)
        };

        let base_run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };

        let runs = if let Some(marked_range) = editor.marked_range.as_ref() {
            let display_start = editor.display_offset_for_text_offset(marked_range.start);
            let display_end = editor.display_offset_for_text_offset(marked_range.end);
            let mut composed_runs = Vec::new();

            if display_start > 0 {
                composed_runs.push(TextRun {
                    len: display_start,
                    ..base_run.clone()
                });
            }
            if display_end > display_start {
                composed_runs.push(TextRun {
                    len: display_end - display_start,
                    underline: Some(UnderlineStyle {
                        color: Some(base_run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..base_run.clone()
                });
            }
            if display_end < display_text.len() {
                composed_runs.push(TextRun {
                    len: display_text.len() - display_end,
                    ..base_run
                });
            }

            composed_runs
        } else {
            vec![base_run]
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &runs, None);

        let display_cursor = editor.display_offset_for_text_offset(cursor_offset);
        let cursor_pos = line.x_for_index(display_cursor);
        let display_start = editor.display_offset_for_text_offset(selected_range.start);
        let display_end = editor.display_offset_for_text_offset(selected_range.end);

        let selection = if selected_range.is_empty() {
            None
        } else {
            Some(fill(
                Bounds::from_corners(
                    point(
                        bounds.left() + line.x_for_index(display_start),
                        bounds.top(),
                    ),
                    point(
                        bounds.left() + line.x_for_index(display_end),
                        bounds.bottom(),
                    ),
                ),
                rgba(0x77777740),
            ))
        };

        let cursor = Some(fill(
            Bounds::new(
                point(bounds.left() + cursor_pos, bounds.top()),
                size(px(2.), bounds.bottom() - bounds.top()),
            ),
            rgb(0xffffff),
        ));

        PrepaintState {
            line: Some(line),
            cursor,
            selection,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.editor.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );

        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection)
        }

        let line = prepaint.line.take().unwrap();
        line.paint(
            bounds.origin,
            window.line_height(),
            gpui::TextAlign::Left,
            None,
            window,
            cx,
        )
        .ok();

        if focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }

        self.editor.update(cx, |editor, _cx| {
            editor.last_layout = Some(line);
            editor.last_bounds = Some(bounds);
        });
    }
}

impl Render for Editor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .size_full()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::move_to_previous_word))
            .on_action(cx.listener(Self::move_to_next_word))
            .on_action(cx.listener(Self::delete_to_previous_word_start))
            .on_action(cx.listener(Self::delete_to_next_word_end))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::move_to_beginning_of_line))
            .on_action(cx.listener(Self::move_to_end_of_line))
            .on_action(cx.listener(Self::select_to_beginning_of_line))
            .on_action(cx.listener(Self::select_to_end_of_line))
            .on_action(cx.listener(Self::delete_to_beginning_of_line))
            .on_action(cx.listener(Self::delete_to_end_of_line))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .child(TextElement {
                editor: cx.entity(),
            })
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

    fn render(&self, _: &mut Window, _: &App) -> gpui::AnyElement {
        self.0.clone().into_any_element()
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
