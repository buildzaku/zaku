mod actions;
mod display_map;
mod element;
mod movement;
mod scroll;

pub use actions::*;
pub use element::EditorElement;

use gpui::{
    App, Bounds, ClipboardEntry, ClipboardItem, Context, Entity, EntityInputHandler, EventEmitter,
    FocusHandle, Focusable, Hsla, KeyBinding, KeyContext, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, Render, SharedString, Subscription, TextStyle, UTF16Selection,
    Window, prelude::*,
};
use multi_buffer::{
    MultiBuffer, MultiBufferOffset, MultiBufferOffsetUtf16, MultiBufferRow, MultiBufferSnapshot,
};
use serde::{Deserialize, Serialize};
use std::{
    any::TypeId,
    collections::{HashMap, VecDeque},
    num::NonZeroU32,
    ops::{Range, RangeInclusive},
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};
use text::{Bias, Buffer as TextBuffer, BufferId, OffsetUtf16, ReplicaId, TransactionId};

use input::{ERASED_EDITOR_FACTORY, ErasedEditor, ErasedEditorEvent};
use theme::{ActiveTheme, ThemeSettings};

use crate::element::PositionMap;

pub(crate) const KEY_CONTEXT: &str = "Editor";
static NEXT_BUFFER_ID: AtomicU64 = AtomicU64::new(1);

const DEFAULT_TAB_SIZE: NonZeroU32 = NonZeroU32::new(4).unwrap();

/// Addons allow storing per-editor state in other crates
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
        KeyBinding::new("enter", Newline, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-enter", Newline, Some(KEY_CONTEXT)),
        KeyBinding::new("left", MoveLeft, Some(KEY_CONTEXT)),
        KeyBinding::new("right", MoveRight, Some(KEY_CONTEXT)),
        KeyBinding::new("up", MoveUp, Some(KEY_CONTEXT)),
        KeyBinding::new("down", MoveDown, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-left", SelectLeft, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-right", SelectRight, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-up", SelectUp, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-down", SelectDown, Some(KEY_CONTEXT)),
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
const MAX_LINE_LEN: usize = 1024;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ClipboardSelection {
    pub len: usize,
    pub is_entire_line: bool,
    pub first_line_indent: u32,
    #[serde(default)]
    pub file_path: Option<PathBuf>,
    #[serde(default)]
    pub line_range: Option<RangeInclusive<u32>>,
}

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

#[derive(Clone, Copy, Debug)]
struct ScrollbarDrag {
    axis: gpui::Axis,
    pointer_offset: Pixels,
}

pub struct Editor {
    focus_handle: FocusHandle,
    buffer: Entity<MultiBuffer>,
    display_map: Entity<display_map::DisplayMap>,
    scroll_manager: scroll::ScrollManager,
    mode: EditorMode,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    selection_history: SelectionHistory,
    addons: HashMap<TypeId, Box<dyn Addon>>,
    last_position_map: Option<Rc<PositionMap>>,
    scrollbar_drag: Option<ScrollbarDrag>,
    selecting: bool,
    input_enabled: bool,
    selection_mark_mode: bool,
    masked: bool,
    goal_display_column: Option<u32>,
    word_chars: Arc<[char]>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<EditorEvent> for Editor {}

impl Editor {
    pub fn single_line(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new_with_mode(EditorMode::SingleLine, window, cx)
    }

    pub fn auto_height(
        min_lines: usize,
        max_lines: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_with_mode(
            EditorMode::AutoHeight {
                min_lines,
                max_lines,
            },
            window,
            cx,
        )
    }

    pub fn full(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new_with_mode(EditorMode::full(), window, cx)
    }

    fn new_with_mode(mode: EditorMode, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let text_buffer = cx.new(|_| TextBuffer::new(ReplicaId::LOCAL, next_buffer_id(), ""));
        let buffer = cx.new(|cx| MultiBuffer::singleton(text_buffer.clone(), cx));
        let display_map =
            cx.new(|cx| display_map::DisplayMap::new(buffer.clone(), DEFAULT_TAB_SIZE, cx));
        let scroll_manager = scroll::ScrollManager::new();
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
            display_map,
            scroll_manager,
            mode,
            placeholder: SharedString::default(),
            selected_range,
            selection_reversed,
            marked_range: None,
            selection_history,
            addons: HashMap::new(),
            last_position_map: None,
            scrollbar_drag: None,
            selecting: false,
            input_enabled: true,
            selection_mark_mode: false,
            masked: false,
            goal_display_column: None,
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

    fn snapshot(&self, cx: &App) -> MultiBufferSnapshot {
        self.buffer.read(cx).snapshot(cx)
    }

    fn display_snapshot(&self, cx: &mut Context<Self>) -> display_map::DisplaySnapshot {
        self.display_map
            .update(cx, |display_map, cx| display_map.snapshot(cx))
    }

    fn scroll_position(
        &self,
        snapshot: &display_map::DisplaySnapshot,
    ) -> gpui::Point<scroll::ScrollOffset> {
        self.scroll_manager.scroll_position(snapshot)
    }

    fn set_scroll_position(
        &mut self,
        snapshot: &display_map::DisplaySnapshot,
        position: gpui::Point<scroll::ScrollOffset>,
        cx: &mut Context<Self>,
    ) {
        self.scroll_manager.set_scroll_position(snapshot, position);
        cx.notify();
    }

    fn selection_state(&self) -> SelectionState {
        SelectionState {
            range: self.selected_range.clone(),
            reversed: self.selection_reversed,
        }
    }

    fn restore_selection_state(&mut self, state: &SelectionState, cx: &App) {
        self.selected_range = state.range.clone();
        self.selection_reversed = state.reversed;
        let text_len = self.snapshot(cx).len().0;
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

    fn char_classifier(&self) -> movement::CharClassifier {
        movement::CharClassifier::new(self.word_chars.clone())
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.goal_display_column = None;

        let text_len = self.snapshot(cx).len().0;
        let offset = offset.min(text_len);
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        self.record_selection_history();
        self.request_autoscroll(scroll::Autoscroll::newest(), cx);
        cx.notify();
    }

    fn move_to_vertical(&mut self, offset: usize, cx: &mut Context<Self>) {
        let text_len = self.snapshot(cx).len().0;
        let offset = offset.min(text_len);
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        self.record_selection_history();
        self.request_autoscroll(scroll::Autoscroll::newest(), cx);
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.goal_display_column = None;

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
        self.request_autoscroll(scroll::Autoscroll::newest(), cx);
        cx.notify();
    }

    fn select_to_vertical(&mut self, offset: usize, cx: &mut Context<Self>) {
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
        self.request_autoscroll(scroll::Autoscroll::newest(), cx);
        cx.notify();
    }

    fn offset_for_vertical_move(&mut self, row_delta: i32, cx: &mut Context<Self>) -> usize {
        let display_snapshot = self.display_snapshot(cx);
        let buffer_snapshot = display_snapshot.buffer_snapshot();
        let cursor_offset = self.cursor_offset().min(buffer_snapshot.len().0);
        let cursor_point = buffer_snapshot.offset_to_point(MultiBufferOffset(cursor_offset));
        let cursor_display_point =
            display_snapshot.point_to_display_point(cursor_point, Bias::Left);
        let max_row = buffer_snapshot.max_point().row;

        let target_row = if row_delta.is_negative() {
            cursor_display_point
                .row()
                .0
                .saturating_sub(row_delta.unsigned_abs())
        } else {
            cursor_display_point
                .row()
                .0
                .saturating_add(row_delta.unsigned_abs())
                .min(max_row)
        };

        if target_row == cursor_display_point.row().0 {
            return cursor_offset;
        }

        let current_display_column = if self.masked {
            let current_line =
                buffer_line_text(display_snapshot.buffer_snapshot(), cursor_point.row);
            let column_bytes = (cursor_point.column as usize).min(current_line.len());
            current_line
                .get(..column_bytes)
                .unwrap_or("")
                .chars()
                .count() as u32
        } else {
            cursor_display_point.column()
        };

        let goal_column = *self
            .goal_display_column
            .get_or_insert(current_display_column);

        if self.masked {
            let target_line = buffer_line_text(display_snapshot.buffer_snapshot(), target_row);
            let target_column =
                byte_offset_from_char_count(&target_line, goal_column as usize) as u32;

            return buffer_snapshot
                .point_to_offset(text::Point {
                    row: target_row,
                    column: target_column,
                })
                .0;
        }

        let target_display_point = display_snapshot.clip_point(
            display_map::DisplayPoint::new(display_map::DisplayRow(target_row), goal_column),
            Bias::Left,
        );
        let target_buffer_point =
            display_snapshot.display_point_to_point(target_display_point, Bias::Left);
        buffer_snapshot.point_to_offset(target_buffer_point).0
    }

    fn offset_for_horizontal_move(&self, direction: i32, cx: &mut Context<Self>) -> usize {
        let display_snapshot = self.display_snapshot(cx);
        let buffer_snapshot = display_snapshot.buffer_snapshot();
        let cursor_offset = self.cursor_offset().min(buffer_snapshot.len().0);
        let cursor_point = buffer_snapshot.offset_to_point(MultiBufferOffset(cursor_offset));
        let display_point = display_snapshot.point_to_display_point(cursor_point, Bias::Left);

        if direction < 0 {
            let moved = movement::left(&display_snapshot, display_point);
            moved.to_offset(&display_snapshot, Bias::Left)
        } else {
            let moved = movement::right(&display_snapshot, display_point);
            moved.to_offset(&display_snapshot, Bias::Right)
        }
    }

    fn previous_word_start(&self, offset: usize, cx: &mut Context<Self>) -> usize {
        let display_snapshot = self.display_snapshot(cx);
        let buffer_snapshot = display_snapshot.buffer_snapshot();
        let offset = buffer_snapshot.clip_offset(
            MultiBufferOffset(offset.min(buffer_snapshot.len().0)),
            Bias::Left,
        );
        let point = buffer_snapshot.offset_to_point(offset);
        let display_point = display_snapshot.point_to_display_point(point, Bias::Left);
        let classifier = self.char_classifier();
        let target = movement::previous_word_start(&display_snapshot, display_point, &classifier);
        target.to_offset(&display_snapshot, Bias::Left)
    }

    fn next_word_end(&self, offset: usize, cx: &mut Context<Self>) -> usize {
        let display_snapshot = self.display_snapshot(cx);
        let buffer_snapshot = display_snapshot.buffer_snapshot();
        let offset = buffer_snapshot.clip_offset(
            MultiBufferOffset(offset.min(buffer_snapshot.len().0)),
            Bias::Right,
        );
        let point = buffer_snapshot.offset_to_point(offset);
        let display_point = display_snapshot.point_to_display_point(point, Bias::Right);
        let classifier = self.char_classifier();
        let target = movement::next_word_end(&display_snapshot, display_point, &classifier);
        target.to_offset(&display_snapshot, Bias::Right)
    }

    fn previous_subword_start(&self, offset: usize, cx: &mut Context<Self>) -> usize {
        let display_snapshot = self.display_snapshot(cx);
        let buffer_snapshot = display_snapshot.buffer_snapshot();
        let offset = buffer_snapshot.clip_offset(
            MultiBufferOffset(offset.min(buffer_snapshot.len().0)),
            Bias::Left,
        );
        let point = buffer_snapshot.offset_to_point(offset);
        let display_point = display_snapshot.point_to_display_point(point, Bias::Left);
        let classifier = self.char_classifier();
        let target =
            movement::previous_subword_start(&display_snapshot, display_point, &classifier);
        target.to_offset(&display_snapshot, Bias::Left)
    }

    fn next_subword_end(&self, offset: usize, cx: &mut Context<Self>) -> usize {
        let display_snapshot = self.display_snapshot(cx);
        let buffer_snapshot = display_snapshot.buffer_snapshot();
        let offset = buffer_snapshot.clip_offset(
            MultiBufferOffset(offset.min(buffer_snapshot.len().0)),
            Bias::Right,
        );
        let point = buffer_snapshot.offset_to_point(offset);
        let display_point = display_snapshot.point_to_display_point(point, Bias::Right);
        let classifier = self.char_classifier();
        let target = movement::next_subword_end(&display_snapshot, display_point, &classifier);
        target.to_offset(&display_snapshot, Bias::Right)
    }

    fn line_indent_offset(&self, cx: &mut Context<Self>) -> usize {
        let display_snapshot = self.display_snapshot(cx);
        let buffer_snapshot = display_snapshot.buffer_snapshot();
        let cursor = buffer_snapshot.clip_offset(
            MultiBufferOffset(self.cursor_offset().min(buffer_snapshot.len().0)),
            Bias::Left,
        );
        let cursor_point = buffer_snapshot.offset_to_point(cursor);
        let row = display_map::DisplayRow(cursor_point.row);
        let line_start = buffer_snapshot.point_to_offset(text::Point::new(cursor_point.row, 0));
        let line = buffer_line_text(display_snapshot.buffer_snapshot(), row.0);

        for (index, character) in line.char_indices() {
            if !character.is_whitespace() {
                return (line_start + index).0;
            }
        }
        (line_start + line.len()).0
    }

    fn line_beginning_offset(&self, stop_at_indent: bool, cx: &mut Context<Self>) -> usize {
        let display_snapshot = self.display_snapshot(cx);
        let buffer_snapshot = display_snapshot.buffer_snapshot();
        let cursor = buffer_snapshot.clip_offset(
            MultiBufferOffset(self.cursor_offset().min(buffer_snapshot.len().0)),
            Bias::Left,
        );
        let cursor_point = buffer_snapshot.offset_to_point(cursor);
        let line_start = buffer_snapshot.point_to_offset(text::Point::new(cursor_point.row, 0));

        if !stop_at_indent {
            return line_start.0;
        }

        let indent = self.line_indent_offset(cx);
        if cursor.0 > indent || cursor == line_start {
            indent
        } else {
            line_start.0
        }
    }

    fn line_end_offset(&self, cx: &mut Context<Self>) -> usize {
        let display_snapshot = self.display_snapshot(cx);
        let buffer_snapshot = display_snapshot.buffer_snapshot();
        let cursor = buffer_snapshot.clip_offset(
            MultiBufferOffset(self.cursor_offset().min(buffer_snapshot.len().0)),
            Bias::Left,
        );
        let cursor_point = buffer_snapshot.offset_to_point(cursor);
        let line_start = buffer_snapshot.point_to_offset(text::Point::new(cursor_point.row, 0));
        let line_len = buffer_snapshot.line_len(MultiBufferRow(cursor_point.row)) as usize;
        (line_start + line_len).0
    }

    fn replace_range(&mut self, range: Range<usize>, new_text: &str, cx: &mut Context<Self>) {
        let snapshot = self.snapshot(cx);
        let start = snapshot.clip_offset(
            MultiBufferOffset(range.start.min(snapshot.len().0)),
            Bias::Left,
        );
        let end = snapshot.clip_offset(
            MultiBufferOffset(range.end.min(snapshot.len().0)),
            Bias::Right,
        );
        let range = if end < start { end..start } else { start..end };
        if range.is_empty() && new_text.is_empty() {
            return;
        }

        let now = Instant::now();
        let before = self.selection_state();
        let transaction_id = self
            .buffer
            .update(cx, |buffer, cx| buffer.start_transaction_at(now, cx));
        if let Some(transaction_id) = transaction_id {
            self.selection_history
                .selections_by_transaction
                .entry(transaction_id)
                .or_insert(SelectionTransactionEntry {
                    before: before.clone(),
                    after: before.clone(),
                });
        }

        let edit_range = range.start..range.end;
        self.buffer.update(cx, |buffer, cx| {
            buffer.edit([(edit_range.clone(), new_text)], cx);
        });
        let cursor = (range.start + new_text.len()).0;
        self.selected_range = cursor..cursor;
        self.selection_reversed = false;
        self.marked_range = None;
        self.goal_display_column = None;
        self.record_selection_history();
        self.request_autoscroll(scroll::Autoscroll::newest(), cx);
        let after = self.selection_state();
        let transaction_id = self
            .buffer
            .update(cx, |buffer, cx| buffer.end_transaction_at(now, cx));
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
        let snapshot = self.snapshot(cx);
        let start = snapshot.clip_offset(
            MultiBufferOffset(range.start.min(snapshot.len().0)),
            Bias::Left,
        );
        let end = snapshot.clip_offset(
            MultiBufferOffset(range.end.min(snapshot.len().0)),
            Bias::Right,
        );
        let range = if end < start { end..start } else { start..end };

        if range.is_empty() && new_text.is_empty() {
            self.selected_range = selection;
            self.selection_reversed = false;
            self.marked_range = marked_range;
            self.goal_display_column = None;
            self.record_selection_history();
            self.request_autoscroll(scroll::Autoscroll::newest(), cx);
            cx.notify();
            return;
        }

        let now = Instant::now();
        let before = self.selection_state();
        let transaction_id = self
            .buffer
            .update(cx, |buffer, cx| buffer.start_transaction_at(now, cx));
        if let Some(transaction_id) = transaction_id {
            self.selection_history
                .selections_by_transaction
                .entry(transaction_id)
                .or_insert(SelectionTransactionEntry {
                    before: before.clone(),
                    after: before.clone(),
                });
        }

        let edit_range = range.start..range.end;
        self.buffer.update(cx, |buffer, cx| {
            buffer.edit([(edit_range.clone(), new_text)], cx);
        });
        self.selected_range = selection;
        self.selection_reversed = false;
        self.marked_range = marked_range;
        self.goal_display_column = None;
        self.record_selection_history();
        self.request_autoscroll(scroll::Autoscroll::newest(), cx);
        let after = self.selection_state();
        let transaction_id = self
            .buffer
            .update(cx, |buffer, cx| buffer.end_transaction_at(now, cx));
        self.record_selection_transaction(transaction_id, before, after);
        cx.emit(EditorEvent::BufferEdited);
        cx.notify();
    }

    fn replace_selection(&mut self, new_text: &str, cx: &mut Context<Self>) {
        let range = self.selected_range.clone();
        self.replace_range(range, new_text, cx);
    }

    fn text_offset_from_utf16(&self, utf16_offset: usize, cx: &App) -> usize {
        let snapshot = self.snapshot(cx);
        snapshot
            .offset_utf16_to_offset(MultiBufferOffsetUtf16(OffsetUtf16(utf16_offset)))
            .0
            .min(snapshot.len().0)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>, cx: &App) -> Range<usize> {
        let start = self.text_offset_from_utf16(range_utf16.start, cx);
        let end = self.text_offset_from_utf16(range_utf16.end, cx);
        start..end
    }

    fn range_to_utf16(&self, range_utf8: &Range<usize>, cx: &App) -> Range<usize> {
        let snapshot = self.snapshot(cx);
        let start = snapshot.clip_offset(
            MultiBufferOffset(range_utf8.start.min(snapshot.len().0)),
            Bias::Left,
        );
        let end = snapshot.clip_offset(
            MultiBufferOffset(range_utf8.end.min(snapshot.len().0)),
            Bias::Right,
        );
        let range = if end < start { end..start } else { start..end };
        let start = snapshot.offset_to_offset_utf16(range.start).0.0;
        let end = snapshot.offset_to_offset_utf16(range.end).0.0;
        start..end
    }

    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.buffer.update(cx, |buffer, cx| {
            buffer
                .as_singleton()
                .expect("set_text requires a singleton buffer");
            buffer.set_text(text, cx);
        });
        self.selected_range = text.len()..text.len();
        self.selection_reversed = false;
        self.marked_range = None;
        self.goal_display_column = None;
        self.record_selection_history();
        if matches!(
            self.mode,
            EditorMode::SingleLine | EditorMode::AutoHeight { .. }
        ) {
            self.request_autoscroll(scroll::Autoscroll::newest(), cx);
        }
        cx.emit(EditorEvent::BufferEdited);
        cx.notify();
    }

    fn clear(&mut self, cx: &mut Context<Self>) {
        let snapshot = self.snapshot(cx);
        if snapshot.is_empty() {
            return;
        }

        let now = Instant::now();
        let before = self.selection_state();
        let transaction_id = self
            .buffer
            .update(cx, |buffer, cx| buffer.start_transaction_at(now, cx));
        if let Some(transaction_id) = transaction_id {
            self.selection_history
                .selections_by_transaction
                .entry(transaction_id)
                .or_insert(SelectionTransactionEntry {
                    before: before.clone(),
                    after: before.clone(),
                });
        }

        self.buffer.update(cx, |buffer, cx| {
            buffer.edit([(MultiBufferOffset::ZERO..snapshot.len(), "")], cx);
        });
        self.selected_range = 0..0;
        self.selection_reversed = false;
        self.marked_range = None;
        self.goal_display_column = None;
        self.record_selection_history();
        self.request_autoscroll(scroll::Autoscroll::newest(), cx);
        let after = self.selection_state();
        let transaction_id = self
            .buffer
            .update(cx, |buffer, cx| buffer.end_transaction_at(now, cx));
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

    pub fn move_selection_to_end(&mut self, cx: &mut Context<Self>) {
        let offset = self.snapshot(cx).len().0;
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        self.goal_display_column = None;
        self.record_selection_history();
        self.request_autoscroll(scroll::Autoscroll::newest(), cx);
        cx.notify();
    }

    fn move_left(&mut self, _: &MoveLeft, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let offset = self.offset_for_horizontal_move(-1, cx);
            self.move_to(offset, cx);
        } else {
            self.move_to(self.selected_range.start, cx)
        }
    }

    fn move_right(&mut self, _: &MoveRight, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let offset = self.offset_for_horizontal_move(1, cx);
            self.move_to(offset, cx);
        } else {
            self.move_to(self.selected_range.end, cx)
        }
    }

    fn move_up(&mut self, _: &MoveUp, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            self.move_to(self.selected_range.start, cx);
            return;
        }

        let offset = self.offset_for_vertical_move(-1, cx);
        self.move_to_vertical(offset, cx);
    }

    fn move_down(&mut self, _: &MoveDown, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            self.move_to(self.selected_range.end, cx);
            return;
        }

        let offset = self.offset_for_vertical_move(1, cx);
        self.move_to_vertical(offset, cx);
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        let offset = self.offset_for_horizontal_move(-1, cx);
        self.select_to(offset, cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        let offset = self.offset_for_horizontal_move(1, cx);
        self.select_to(offset, cx);
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        let offset = self.offset_for_vertical_move(-1, cx);
        self.select_to_vertical(offset, cx);
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        let offset = self.offset_for_vertical_move(1, cx);
        self.select_to_vertical(offset, cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.selected_range = 0..self.snapshot(cx).len().0;
        self.selection_reversed = false;
        self.goal_display_column = None;
        self.record_selection_history();
        self.request_autoscroll(scroll::Autoscroll::newest(), cx);
        cx.notify();
    }

    fn move_to_beginning_of_line(
        &mut self,
        action: &MoveToBeginningOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.line_beginning_offset(action.stop_at_indent, cx);
        self.move_to(offset, cx);
    }

    fn select_to_beginning_of_line(
        &mut self,
        action: &SelectToBeginningOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.line_beginning_offset(action.stop_at_indent, cx);
        self.select_to(offset, cx);
    }

    fn delete_to_beginning_of_line(
        &mut self,
        action: &DeleteToBeginningOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.input_enabled {
            return;
        }

        if self.selected_range.is_empty() {
            let offset = self.line_beginning_offset(action.stop_at_indent, cx);
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
        let offset = self.line_end_offset(cx);
        self.move_to(offset, cx);
    }

    fn select_to_end_of_line(
        &mut self,
        _: &SelectToEndOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.line_end_offset(cx);
        self.select_to(offset, cx);
    }

    fn delete_to_end_of_line(
        &mut self,
        _: &DeleteToEndOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.input_enabled {
            return;
        }

        if self.selected_range.is_empty() {
            let cursor = self.cursor_offset();
            let end = self.line_end_offset(cx);
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
        let offset = self.snapshot(cx).len().0;
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
        let offset = self.snapshot(cx).len().0;
        self.select_to(offset, cx);
    }

    fn move_to_previous_word_start(
        &mut self,
        _: &MoveToPreviousWordStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.previous_word_start(self.cursor_offset(), cx);
        self.move_to(offset, cx);
    }

    fn move_to_previous_subword_start(
        &mut self,
        _: &MoveToPreviousSubwordStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.previous_subword_start(self.cursor_offset(), cx);
        self.move_to(offset, cx);
    }

    fn move_to_next_word_end(
        &mut self,
        _: &MoveToNextWordEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.next_word_end(self.cursor_offset(), cx);
        self.move_to(offset, cx);
    }

    fn move_to_next_subword_end(
        &mut self,
        _: &MoveToNextSubwordEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.next_subword_end(self.cursor_offset(), cx);
        self.move_to(offset, cx);
    }

    fn select_to_previous_word_start(
        &mut self,
        _: &SelectToPreviousWordStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.previous_word_start(self.cursor_offset(), cx);
        self.select_to(offset, cx);
    }

    fn select_to_next_word_end(
        &mut self,
        _: &SelectToNextWordEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.next_word_end(self.cursor_offset(), cx);
        self.select_to(offset, cx);
    }

    fn select_to_previous_subword_start(
        &mut self,
        _: &SelectToPreviousSubwordStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.previous_subword_start(self.cursor_offset(), cx);
        self.select_to(offset, cx);
    }

    fn select_to_next_subword_end(
        &mut self,
        _: &SelectToNextSubwordEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.next_subword_end(self.cursor_offset(), cx);
        self.select_to(offset, cx);
    }

    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        if !self.input_enabled {
            return;
        }

        if self.selected_range.is_empty() {
            let start = self.offset_for_horizontal_move(-1, cx);
            self.selected_range = start..self.cursor_offset();
        }
        self.replace_selection("", cx);
    }

    fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        if !self.input_enabled {
            return;
        }

        if self.selected_range.is_empty() {
            let end = self.offset_for_horizontal_move(1, cx);
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
        if !self.input_enabled {
            return;
        }

        if self.selected_range.is_empty() {
            let start = self.previous_word_start(self.cursor_offset(), cx);
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
        if !self.input_enabled {
            return;
        }

        if self.selected_range.is_empty() {
            let start = self.previous_subword_start(self.cursor_offset(), cx);
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
        if !self.input_enabled {
            return;
        }

        if self.selected_range.is_empty() {
            let end = self.next_word_end(self.cursor_offset(), cx);
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
        if !self.input_enabled {
            return;
        }

        if self.selected_range.is_empty() {
            let end = self.next_subword_end(self.cursor_offset(), cx);
            self.selected_range = self.cursor_offset()..end;
        }
        self.replace_selection("", cx);
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            return;
        }

        let current_text = self.snapshot(cx).text();
        let text = current_text
            .get(self.selected_range.clone())
            .unwrap_or("")
            .to_string();
        if text.is_empty() {
            return;
        }

        let snapshot = self.snapshot(cx);
        let start_offset = snapshot.clip_offset(
            MultiBufferOffset(self.selected_range.start.min(snapshot.len().0)),
            Bias::Left,
        );
        let end_offset = snapshot.clip_offset(
            MultiBufferOffset(self.selected_range.end.min(snapshot.len().0)),
            Bias::Right,
        );
        let start_row = snapshot.offset_to_point(start_offset).row;
        let end_row = snapshot.offset_to_point(end_offset).row;
        let selection_metadata = ClipboardSelection {
            len: text.len(),
            is_entire_line: false,
            first_line_indent: 0,
            file_path: None,
            line_range: Some(start_row..=end_row),
        };
        cx.write_to_clipboard(ClipboardItem::new_string_with_json_metadata(
            text,
            vec![selection_metadata],
        ));
    }

    fn cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        if !self.input_enabled {
            return;
        }

        if self.selected_range.is_empty() {
            return;
        }

        let current_text = self.snapshot(cx).text();
        let text = current_text
            .get(self.selected_range.clone())
            .unwrap_or("")
            .to_string();
        if text.is_empty() {
            return;
        }

        let snapshot = self.snapshot(cx);
        let start_offset = snapshot.clip_offset(
            MultiBufferOffset(self.selected_range.start.min(snapshot.len().0)),
            Bias::Left,
        );
        let end_offset = snapshot.clip_offset(
            MultiBufferOffset(self.selected_range.end.min(snapshot.len().0)),
            Bias::Right,
        );
        let start_row = snapshot.offset_to_point(start_offset).row;
        let end_row = snapshot.offset_to_point(end_offset).row;
        let selection_metadata = ClipboardSelection {
            len: text.len(),
            is_entire_line: false,
            first_line_indent: 0,
            file_path: None,
            line_range: Some(start_row..=end_row),
        };
        cx.write_to_clipboard(ClipboardItem::new_string_with_json_metadata(
            text,
            vec![selection_metadata],
        ));
        self.replace_selection("", cx);
    }

    pub fn do_paste(
        &mut self,
        text: &String,
        clipboard_selections: Option<Vec<ClipboardSelection>>,
        handle_entire_lines: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.input_enabled {
            return;
        }

        let mut text_to_insert = text.as_str();
        let mut is_entire_line = false;

        if let Some(selection) = clipboard_selections
            .as_ref()
            .and_then(|selections| selections.first())
        {
            is_entire_line = selection.is_entire_line;

            if let Some(slice) = text.get(..selection.len.min(text.len())) {
                text_to_insert = slice;
            }
        }

        if self.selected_range.is_empty() && handle_entire_lines && is_entire_line {
            let snapshot = self.snapshot(cx);
            let cursor = snapshot.clip_offset(
                MultiBufferOffset(self.cursor_offset().min(snapshot.len().0)),
                Bias::Left,
            );
            let cursor_point = snapshot.offset_to_point(cursor);
            let line_start = snapshot.point_to_offset(text::Point::new(cursor_point.row, 0));
            self.selected_range = line_start.0..line_start.0;
            self.selection_reversed = false;
        }

        let _ = window;
        self.replace_selection(text_to_insert, cx);
    }

    pub fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if !self.input_enabled {
            return;
        }

        if let Some(item) = cx.read_from_clipboard() {
            let entries = item.entries();
            match entries.first() {
                // For now, we only support applying metadata if there's one string.
                Some(ClipboardEntry::String(clipboard_string)) if entries.len() == 1 => {
                    self.do_paste(
                        clipboard_string.text(),
                        clipboard_string.metadata_json::<Vec<ClipboardSelection>>(),
                        true,
                        window,
                        cx,
                    );
                }
                _ => {
                    self.do_paste(&item.text().unwrap_or_default(), None, true, window, cx);
                }
            }
        }
    }

    fn newline(&mut self, _: &Newline, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_input("\n", window, cx);
    }

    fn handle_input(&mut self, text: &str, _: &mut Window, cx: &mut Context<Self>) {
        if !self.input_enabled {
            return;
        }

        self.replace_selection(text, cx);
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        if !self.input_enabled {
            return;
        }

        let transaction_id = self.buffer.update(cx, |buffer, cx| buffer.undo(cx));
        let Some(transaction_id) = transaction_id else {
            return;
        };

        let selection_state = self
            .selection_history
            .selections_by_transaction
            .get(&transaction_id)
            .map(|entry| entry.before.clone());
        if let Some(selection_state) = selection_state.as_ref() {
            self.restore_selection_state(selection_state, cx);
        } else {
            let text_len = self.snapshot(cx).len().0;
            self.clamp_selection(text_len);
        }
        self.marked_range = None;
        self.goal_display_column = None;
        self.request_autoscroll(scroll::Autoscroll::newest(), cx);
        cx.emit(EditorEvent::BufferEdited);
        cx.notify();
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        if !self.input_enabled {
            return;
        }

        let transaction_id = self.buffer.update(cx, |buffer, cx| buffer.redo(cx));
        let Some(transaction_id) = transaction_id else {
            return;
        };

        let selection_state = self
            .selection_history
            .selections_by_transaction
            .get(&transaction_id)
            .map(|entry| entry.after.clone());
        if let Some(selection_state) = selection_state.as_ref() {
            self.restore_selection_state(selection_state, cx);
        } else {
            let text_len = self.snapshot(cx).len().0;
            self.clamp_selection(text_len);
        }
        self.marked_range = None;
        self.goal_display_column = None;
        self.request_autoscroll(scroll::Autoscroll::newest(), cx);
        cx.emit(EditorEvent::BufferEdited);
        cx.notify();
    }

    fn undo_selection(&mut self, _: &UndoSelection, _: &mut Window, cx: &mut Context<Self>) {
        self.selection_history.mode = SelectionHistoryMode::Undoing;
        let state = self.selection_history.undo();
        self.selection_history.mode = SelectionHistoryMode::Normal;
        if let Some(state) = state {
            self.restore_selection_state(&state, cx);
            self.goal_display_column = None;
            self.request_autoscroll(scroll::Autoscroll::newest(), cx);
            cx.notify();
        }
    }

    fn redo_selection(&mut self, _: &RedoSelection, _: &mut Window, cx: &mut Context<Self>) {
        self.selection_history.mode = SelectionHistoryMode::Redoing;
        let state = self.selection_history.redo();
        self.selection_history.mode = SelectionHistoryMode::Normal;
        if let Some(state) = state {
            self.restore_selection_state(&state, cx);
            self.goal_display_column = None;
            self.request_autoscroll(scroll::Autoscroll::newest(), cx);
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

        let text_len = self.snapshot(cx).len().0;
        let offset = self
            .index_for_mouse_position(event.position, cx)
            .min(text_len);

        if event.modifiers.shift {
            self.goal_display_column = None;

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
        } else {
            self.goal_display_column = None;
            self.selected_range = offset..offset;
            self.selection_reversed = false;
            self.record_selection_history();
            cx.notify();
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.selecting {
            let text_len = self.snapshot(cx).len().0;
            let offset = self
                .index_for_mouse_position(event.position, cx)
                .min(text_len);

            self.goal_display_column = None;

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
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>, _cx: &mut Context<Self>) -> usize {
        let Some(position_map) = self.last_position_map.as_ref() else {
            return 0;
        };
        let snapshot = position_map.snapshot.buffer_snapshot();
        if snapshot.is_empty() {
            return 0;
        }

        if position.y < position_map.bounds.top() {
            return 0;
        }
        if position.y > position_map.bounds.bottom() {
            return snapshot.len().0;
        }

        let point_for_position = position_map.point_for_position(position);
        let offset = if position_map.masked {
            let row = point_for_position.previous_valid.row().0;
            let line_index = row.saturating_sub(position_map.scroll_position.y as u32) as usize;
            let Some(line) = position_map.line_layouts.get(line_index) else {
                return snapshot.len().0;
            };
            if line.row.0 != row {
                return snapshot.len().0;
            }

            let unclipped_column = point_for_position
                .previous_valid
                .column()
                .saturating_add(point_for_position.column_overshoot_after_line_end);
            let local_display_column =
                unclipped_column.saturating_sub(line.line_display_column_start as u32) as usize;
            let local_display_column = local_display_column.min(line.len);
            let buffer_column = byte_offset_from_char_count(&line.line_text, local_display_column);
            line.line_start_offset + buffer_column
        } else {
            let display_point = point_for_position
                .as_valid()
                .unwrap_or(point_for_position.previous_valid);
            let point = position_map
                .snapshot
                .display_point_to_point(display_point, Bias::Left);
            snapshot.point_to_offset(point).0
        };
        offset.min(snapshot.len().0)
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
            EditorMode::Full { .. } => "full",
        };
        key_context.set("mode", mode);

        if self.selection_mark_mode {
            key_context.add("selection_mode");
        }

        if self.mode == EditorMode::SingleLine
            && self.selected_range.is_empty()
            && self.selected_range.end == self.snapshot(cx).len().0
        {
            key_context.add("end_of_input");
        }

        for addon in self.addons.values() {
            addon.extend_key_context(&mut key_context, cx);
        }

        key_context
    }

    pub(crate) fn create_style(&self, cx: &App) -> EditorStyle {
        let theme_colors = cx.theme().colors();
        let theme_settings = ThemeSettings::get_global(cx);

        let font_size = match self.mode {
            EditorMode::SingleLine | EditorMode::AutoHeight { .. } => gpui::rems(0.875).into(),
            EditorMode::Full { .. } => theme_settings.buffer_font_size(cx).into(),
        };

        let text_style = TextStyle {
            color: theme_colors.editor_foreground,
            font_family: theme_settings.buffer_font.family.clone(),
            font_features: theme_settings.buffer_font.features.clone(),
            font_fallbacks: theme_settings.buffer_font.fallbacks.clone(),
            font_size,
            font_weight: theme_settings.buffer_font.weight,
            font_style: theme_settings.buffer_font.style,
            line_height: gpui::relative(theme_settings.line_height()),
            ..Default::default()
        };

        EditorStyle {
            background: theme_colors.editor_background,
            text: text_style,
        }
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
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16, cx);
        actual_range.replace(self.range_to_utf16(&range, cx));
        let snapshot = self.snapshot(cx);
        let start = range.start.min(snapshot.len().0);
        let end = range.end.min(snapshot.len().0);
        let (start, end) = if end < start {
            (end, start)
        } else {
            (start, end)
        };
        Some(
            snapshot
                .text_for_range(MultiBufferOffset(start)..MultiBufferOffset(end))
                .collect(),
        )
    }

    fn selected_text_range(
        &mut self,
        ignore_disabled_input: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        if !ignore_disabled_input && !self.input_enabled {
            return None;
        }

        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range, cx),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range, cx))
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
            .map(|range_utf16| self.range_from_utf16(range_utf16, cx))
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
            .map(|range_utf16| self.range_from_utf16(range_utf16, cx))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        let selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| range_from_utf16_in_text(new_text, range_utf16))
            .map(|new_range| range.start + new_range.start..range.start + new_range.end)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());

        let marked_range = if new_text.is_empty() {
            None
        } else {
            Some(range.start..range.start + new_text.len())
        };

        self.replace_range_with_selection(range, new_text, selected_range, marked_range, cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _bounds: Bounds<Pixels>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let position_map = self.last_position_map.as_ref()?;
        let display_snapshot = self.display_snapshot(cx);
        let range = self.range_from_utf16(&range_utf16, cx);
        let start_point = display_snapshot
            .buffer_snapshot()
            .offset_to_point(MultiBufferOffset(
                range.start.min(display_snapshot.buffer_snapshot().len().0),
            ));
        let end_point = display_snapshot
            .buffer_snapshot()
            .offset_to_point(MultiBufferOffset(
                range.end.min(display_snapshot.buffer_snapshot().len().0),
            ));

        let row = start_point.row;
        let line_index = row.saturating_sub(position_map.scroll_position.y as u32) as usize;
        let Some(line) = position_map.line_layouts.get(line_index) else {
            return None;
        };
        if line.row.0 != row {
            return None;
        }

        let start_display_column = display_snapshot
            .point_to_display_point(start_point, Bias::Left)
            .column() as usize;
        let end_display_column = if start_point.row == end_point.row {
            display_snapshot
                .point_to_display_point(end_point, Bias::Right)
                .column() as usize
        } else {
            let row = display_map::DisplayRow(row);
            display_snapshot.line_len(row) as usize
        };
        let line_display_column_start = line.line_display_column_start;
        let start_display_column = start_display_column
            .saturating_sub(line_display_column_start)
            .min(line.len);
        let end_display_column = end_display_column
            .saturating_sub(line_display_column_start)
            .min(line.len);

        let top_left = gpui::point(
            line.origin.x + line.x_for_index(start_display_column),
            line.origin.y,
        );
        let bottom_right = gpui::point(
            line.origin.x + line.x_for_index(end_display_column),
            line.origin.y + position_map.line_height,
        );

        Some(Bounds::from_corners(top_left, bottom_right))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let position_map = self.last_position_map.as_ref()?;
        position_map.bounds.localize(&point)?;
        let snapshot = position_map.snapshot.buffer_snapshot();
        let point_for_position = position_map.point_for_position(point);
        let offset = if position_map.masked {
            let row = point_for_position.previous_valid.row().0;
            let line_index = row.saturating_sub(position_map.scroll_position.y as u32) as usize;
            let line = position_map.line_layouts.get(line_index)?;
            if line.row.0 != row {
                return None;
            }
            let unclipped_column = point_for_position
                .previous_valid
                .column()
                .saturating_add(point_for_position.column_overshoot_after_line_end);
            let local_display_column =
                unclipped_column.saturating_sub(line.line_display_column_start as u32) as usize;
            let local_display_column = local_display_column.min(line.len);
            let buffer_column = byte_offset_from_char_count(&line.line_text, local_display_column);
            line.line_start_offset + buffer_column
        } else {
            let display_point = point_for_position
                .as_valid()
                .unwrap_or(point_for_position.previous_valid);
            let point = position_map
                .snapshot
                .display_point_to_point(display_point, Bias::Left);
            snapshot.point_to_offset(point).0
        }
        .min(snapshot.len().0);

        Some(
            snapshot
                .offset_to_offset_utf16(MultiBufferOffset(offset))
                .0
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
        self.0.read(cx).snapshot(cx).text()
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

    fn render(&self, _window: &mut Window, cx: &App) -> gpui::AnyElement {
        let theme_colors = cx.theme().colors();
        let theme_settings = ThemeSettings::get_global(cx);

        let text_style = TextStyle {
            font_family: theme_settings.ui_font.family.clone(),
            font_features: theme_settings.ui_font.features.clone(),
            font_size: gpui::rems(0.875).into(),
            font_weight: theme_settings.buffer_font.weight,
            font_style: gpui::FontStyle::Normal,
            line_height: gpui::relative(1.2),
            color: theme_colors.text,
            ..Default::default()
        };

        let editor_style = EditorStyle {
            background: theme_colors.ghost_element_background,
            text: text_style,
        };

        EditorElement::new(&self.0, editor_style).into_any()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        &self.0
    }
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

fn buffer_line_text(snapshot: &MultiBufferSnapshot, row: u32) -> String {
    let line_start = snapshot.point_to_offset(text::Point::new(row, 0));
    let line_end = line_start + snapshot.line_len(MultiBufferRow(row)) as usize;
    snapshot.text_for_range(line_start..line_end).collect()
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
