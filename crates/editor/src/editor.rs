mod actions;
mod display_map;
mod element;
mod movement;
mod scroll;
mod selections_collection;

pub use actions::*;
pub use element::EditorElement;

use gpui::{
    AnyElement, App, Axis, Bounds, ClipboardEntry, ClipboardItem, Context, Entity,
    EntityInputHandler, EventEmitter, FocusHandle, Focusable, FontStyle, Hsla, KeyBinding,
    KeyContext, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Render, SharedString,
    Subscription, TextStyle, UTF16Selection, Window, prelude::*,
};
use multi_buffer::{
    Anchor, MultiBuffer, MultiBufferOffset, MultiBufferOffsetUtf16, MultiBufferRow,
    MultiBufferSnapshot,
};
use serde::{Deserialize, Serialize};
use std::{
    any::TypeId,
    borrow::Cow,
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
use text::{
    Bias, Buffer as TextBuffer, BufferId, OffsetUtf16, ReplicaId, Selection, SelectionGoal,
    TransactionId,
};

use input::{ERASED_EDITOR_FACTORY, ErasedEditor, ErasedEditorEvent};
use theme::{ActiveTheme, ThemeSettings};

use crate::display_map::{DisplayPoint, HighlightKey};
use crate::element::PositionMap;
use crate::selections_collection::{MutableSelectionsCollection, SelectionsCollection};

pub(crate) const KEY_CONTEXT: &str = "Editor";
const KEY_CONTEXT_FULL: &str = "Editor && mode == full";
const KEY_CONTEXT_AUTO_HEIGHT: &str = "Editor && mode == auto_height";
const DEFAULT_TAB_SIZE: NonZeroU32 = NonZeroU32::new(4).unwrap();

static NEXT_BUFFER_ID: AtomicU64 = AtomicU64::new(1);

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
        KeyBinding::new("enter", Newline, Some(KEY_CONTEXT_FULL)),
        KeyBinding::new("shift-enter", Newline, Some(KEY_CONTEXT_FULL)),
        KeyBinding::new("ctrl-enter", Newline, Some(KEY_CONTEXT_AUTO_HEIGHT)),
        KeyBinding::new("shift-enter", Newline, Some(KEY_CONTEXT_AUTO_HEIGHT)),
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

#[derive(Clone, Debug)]
pub enum SelectMode {
    Character,
    Word(Range<Anchor>),
    Line(Range<Anchor>),
    All,
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

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActiveLineHighlight {
    None,
    #[default]
    Line,
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

#[derive(Clone, Debug, PartialEq)]
pub struct EditorSnapshot {
    scroll_position: Point<scroll::ScrollOffset>,
}

impl EditorSnapshot {
    pub fn scroll_position(&self) -> Point<scroll::ScrollOffset> {
        self.scroll_position
    }
}

#[derive(Clone, Debug)]
struct SelectionHistoryEntry {
    selections: Arc<[Selection<Anchor>]>,
}

#[derive(Copy, Clone, Default, Debug, PartialEq, Eq)]
enum SelectionHistoryMode {
    #[default]
    Normal,
    Undoing,
    Redoing,
    Skipping,
}

#[derive(Default)]
struct SelectionHistory {
    selections_by_transaction:
        HashMap<TransactionId, (Arc<[Selection<Anchor>]>, Option<Arc<[Selection<Anchor>]>>)>,
    mode: SelectionHistoryMode,
    undo_stack: VecDeque<SelectionHistoryEntry>,
    redo_stack: VecDeque<SelectionHistoryEntry>,
}

impl SelectionHistory {
    fn new(initial: Arc<[Selection<Anchor>]>) -> Self {
        let mut history = Self::default();
        if !initial.is_empty() {
            history.push(SelectionHistoryEntry {
                selections: initial,
            });
        }
        history
    }

    fn push(&mut self, entry: SelectionHistoryEntry) {
        if !entry.selections.is_empty() {
            match self.mode {
                SelectionHistoryMode::Normal => {
                    self.push_undo(entry);
                    self.redo_stack.clear();
                }
                SelectionHistoryMode::Undoing => self.push_redo(entry),
                SelectionHistoryMode::Redoing => self.push_undo(entry),
                SelectionHistoryMode::Skipping => {}
            }
        }
    }

    fn push_undo(&mut self, entry: SelectionHistoryEntry) {
        let should_push = self
            .undo_stack
            .back()
            .is_none_or(|last| last.selections != entry.selections);
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
            .is_none_or(|last| last.selections != entry.selections);
        if should_push {
            self.redo_stack.push_back(entry);
            if self.redo_stack.len() > MAX_SELECTION_HISTORY_LEN {
                self.redo_stack.pop_front();
            }
        }
    }

    fn insert_transaction(
        &mut self,
        transaction_id: TransactionId,
        selections: Arc<[Selection<Anchor>]>,
    ) {
        if selections.is_empty() {
            return;
        }
        self.selections_by_transaction
            .insert(transaction_id, (selections, None));
    }
}

#[derive(Clone)]
struct SelectionEffects {
    scroll: Option<scroll::Autoscroll>,
}

impl Default for SelectionEffects {
    fn default() -> Self {
        Self {
            scroll: Some(scroll::Autoscroll::newest()),
        }
    }
}

impl SelectionEffects {
    fn no_scroll() -> Self {
        Self { scroll: None }
    }

    fn scroll(scroll: scroll::Autoscroll) -> Self {
        Self {
            scroll: Some(scroll),
        }
    }
}

struct DeferredSelectionEffectsState {
    changed: bool,
    effects: SelectionEffects,
    history_entry: SelectionHistoryEntry,
}

#[derive(Clone, Copy, Debug)]
struct ScrollbarDrag {
    axis: Axis,
    pointer_offset: Pixels,
}

#[derive(Clone, Copy, Debug)]
struct ScrollbarAxes {
    horizontal: bool,
    vertical: bool,
}

pub struct Editor {
    focus_handle: FocusHandle,
    buffer: Entity<MultiBuffer>,
    display_map: Entity<display_map::DisplayMap>,
    selections: SelectionsCollection,
    scroll_manager: scroll::ScrollManager,
    mode: EditorMode,
    placeholder: SharedString,
    ime_transaction: Option<TransactionId>,
    selection_history: SelectionHistory,
    defer_selection_effects: bool,
    deferred_selection_effects_state: Option<DeferredSelectionEffectsState>,
    addons: HashMap<TypeId, Box<dyn Addon>>,
    last_position_map: Option<Rc<PositionMap>>,
    show_scrollbars: ScrollbarAxes,
    scrollbar_drag: Option<ScrollbarDrag>,
    selecting: bool,
    input_enabled: bool,
    selection_mark_mode: bool,
    masked: bool,
    active_line_highlight: Option<ActiveLineHighlight>,
    selection_goal: SelectionGoal,
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
        let show_scrollbars = matches!(mode, EditorMode::Full { .. });
        let selections = SelectionsCollection::new();
        let selection_history = SelectionHistory::new(selections.disjoint_anchors_arc());

        let subscriptions = vec![
            cx.on_focus(&focus_handle, window, Self::on_focus),
            cx.on_blur(&focus_handle, window, Self::on_blur),
        ];

        let mut editor = Self {
            focus_handle,
            buffer,
            display_map,
            selections,
            scroll_manager,
            mode,
            placeholder: SharedString::default(),
            ime_transaction: None,
            selection_history,
            defer_selection_effects: false,
            deferred_selection_effects_state: None,
            addons: HashMap::new(),
            last_position_map: None,
            show_scrollbars: ScrollbarAxes {
                horizontal: show_scrollbars,
                vertical: show_scrollbars,
            },
            scrollbar_drag: None,
            selecting: false,
            input_enabled: true,
            selection_mark_mode: false,
            masked: false,
            active_line_highlight: None,
            selection_goal: SelectionGoal::None,
            _subscriptions: subscriptions,
        };

        editor.selection_history.mode = SelectionHistoryMode::Skipping;
        editor.end_selection(cx);
        editor.selection_history.mode = SelectionHistoryMode::Normal;

        editor
    }

    fn on_focus(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.finalize_last_transaction(cx);
        cx.notify();
    }

    fn on_blur(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.selecting = false;
        self.clear_highlights(HighlightKey::InputComposition, cx);
        self.ime_transaction = None;
        cx.emit(EditorEvent::Blurred);
        cx.notify();
    }

    fn buffer_snapshot(&self, cx: &App) -> MultiBufferSnapshot {
        self.buffer.read(cx).snapshot(cx)
    }

    pub fn snapshot(&self, _window: &Window, cx: &mut App) -> EditorSnapshot {
        let display_snapshot = self
            .display_map
            .update(cx, |display_map, cx| display_map.snapshot(cx));
        EditorSnapshot {
            scroll_position: self.scroll_position(&display_snapshot),
        }
    }

    fn display_snapshot(&self, cx: &mut Context<Self>) -> display_map::DisplaySnapshot {
        self.display_map
            .update(cx, |display_map, cx| display_map.snapshot(cx))
    }

    fn selection(&self, cx: &App) -> Selection<MultiBufferOffset> {
        let snapshot = self.buffer_snapshot(cx);
        let selection = self.selections.newest_anchor();
        Selection {
            id: selection.id,
            start: snapshot.offset_for_anchor(selection.start),
            end: snapshot.offset_for_anchor(selection.end),
            reversed: selection.reversed,
            goal: selection.goal,
        }
    }

    fn selection_utf16(&self, cx: &App) -> Selection<MultiBufferOffsetUtf16> {
        let snapshot = self.buffer_snapshot(cx);
        let selection = self.selections.newest_anchor();
        Selection {
            id: selection.id,
            start: snapshot.offset_to_offset_utf16(snapshot.offset_for_anchor(selection.start)),
            end: snapshot.offset_to_offset_utf16(snapshot.offset_for_anchor(selection.end)),
            reversed: selection.reversed,
            goal: selection.goal,
        }
    }

    fn selected_range(&self, cx: &App) -> Range<usize> {
        let selection = self.selection(cx);
        selection.start.0..selection.end.0
    }

    fn change_selections<R>(
        &mut self,
        effects: SelectionEffects,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut MutableSelectionsCollection<'_, '_>) -> R,
    ) -> R {
        let snapshot = self.display_snapshot(cx);
        if let Some(state) = &mut self.deferred_selection_effects_state {
            state.effects.scroll = effects.scroll.or(state.effects.scroll);
            let (changed, result) = self.selections.change_with(&snapshot, change);
            state.changed |= changed;
            return result;
        }

        let mut state = DeferredSelectionEffectsState {
            changed: false,
            effects,
            history_entry: SelectionHistoryEntry {
                selections: self.selections.disjoint_anchors_arc(),
            },
        };
        let (changed, result) = self.selections.change_with(&snapshot, change);
        state.changed = state.changed || changed;
        if self.defer_selection_effects {
            self.deferred_selection_effects_state = Some(state);
        } else {
            self.apply_selection_effects(state, cx);
        }
        result
    }

    fn with_selection_effects_deferred<R>(
        &mut self,
        cx: &mut Context<Self>,
        update: impl FnOnce(&mut Self, &mut Context<Self>) -> R,
    ) -> R {
        let already_deferred = self.defer_selection_effects;
        self.defer_selection_effects = true;
        let result = update(self, cx);
        if !already_deferred {
            self.defer_selection_effects = false;
            if let Some(state) = self.deferred_selection_effects_state.take() {
                self.apply_selection_effects(state, cx);
            }
        }
        result
    }

    fn transact(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        update: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>),
    ) -> Option<TransactionId> {
        self.with_selection_effects_deferred(cx, |this, cx| {
            this.start_transaction_at(Instant::now(), cx);
            update(this, window, cx);
            this.end_transaction_at(Instant::now(), cx)
        })
    }

    fn apply_selection_effects(
        &mut self,
        state: DeferredSelectionEffectsState,
        cx: &mut Context<Self>,
    ) {
        if state.changed {
            self.selection_history.push(state.history_entry);
            if let Some(autoscroll) = state.effects.scroll {
                self.request_autoscroll(autoscroll, cx);
            }
            cx.notify();
        }
    }

    fn start_transaction_at(
        &mut self,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> Option<TransactionId> {
        self.end_selection(cx);
        if let Some(transaction_id) = self
            .buffer
            .update(cx, |buffer, cx| buffer.start_transaction_at(now, cx))
        {
            self.selection_history
                .insert_transaction(transaction_id, self.selections.disjoint_anchors_arc());
            Some(transaction_id)
        } else {
            None
        }
    }

    fn end_transaction_at(
        &mut self,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> Option<TransactionId> {
        if let Some(transaction_id) = self
            .buffer
            .update(cx, |buffer, cx| buffer.end_transaction_at(now, cx))
        {
            if let Some((_, end_selections)) = self
                .selection_history
                .selections_by_transaction
                .get_mut(&transaction_id)
            {
                *end_selections = Some(self.selections.disjoint_anchors_arc());
            }
            Some(transaction_id)
        } else {
            None
        }
    }

    fn group_until_transaction(&mut self, transaction_id: TransactionId, cx: &mut Context<Self>) {
        self.buffer.update(cx, |buffer, cx| {
            buffer.group_until_transaction(transaction_id, cx);
        });
    }

    fn finalize_last_transaction(&mut self, cx: &mut Context<Self>) {
        self.buffer.update(cx, |buffer, cx| {
            buffer.finalize_last_transaction(cx);
        });
    }

    fn cursor_offset(&self, cx: &App) -> usize {
        let selection = self.selection(cx);
        if selection.reversed {
            selection.start.0
        } else {
            selection.end.0
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selection_goal = SelectionGoal::None;

        let text_len = self.buffer_snapshot(cx).len().0;
        let offset = offset.min(text_len);
        self.change_selections(
            SelectionEffects::scroll(scroll::Autoscroll::newest()),
            cx,
            |s| {
                s.move_cursors_with(&mut |map, _, _| {
                    let snapshot = map.buffer_snapshot();
                    let offset = snapshot.clip_offset(MultiBufferOffset(offset), Bias::Left);
                    let point = snapshot.offset_to_point(offset);
                    (
                        map.point_to_display_point(point, Bias::Left),
                        SelectionGoal::None,
                    )
                });
            },
        );
    }

    fn move_to_vertical(&mut self, offset: usize, cx: &mut Context<Self>) {
        let text_len = self.buffer_snapshot(cx).len().0;
        let offset = offset.min(text_len);
        self.change_selections(
            SelectionEffects::scroll(scroll::Autoscroll::newest()),
            cx,
            |s| {
                s.move_offsets_with(&mut |snapshot, selection| {
                    let offset = snapshot.clip_offset(MultiBufferOffset(offset), Bias::Left);
                    selection.collapse_to(offset, SelectionGoal::None);
                });
            },
        );
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selection_goal = SelectionGoal::None;
        let text_len = self.buffer_snapshot(cx).len().0;
        let offset = offset.min(text_len);
        self.change_selections(
            SelectionEffects::scroll(scroll::Autoscroll::newest()),
            cx,
            |s| {
                s.move_heads_with(&mut |map, _, _| {
                    let snapshot = map.buffer_snapshot();
                    let offset = snapshot.clip_offset(MultiBufferOffset(offset), Bias::Left);
                    let point = snapshot.offset_to_point(offset);
                    (
                        map.point_to_display_point(point, Bias::Left),
                        SelectionGoal::None,
                    )
                });
            },
        );
    }

    fn select_to_vertical(&mut self, offset: usize, cx: &mut Context<Self>) {
        let text_len = self.buffer_snapshot(cx).len().0;
        let offset = offset.min(text_len);
        self.change_selections(
            SelectionEffects::scroll(scroll::Autoscroll::newest()),
            cx,
            |s| {
                s.move_heads_with(&mut |map, _, _| {
                    let snapshot = map.buffer_snapshot();
                    let offset = snapshot.clip_offset(MultiBufferOffset(offset), Bias::Left);
                    let point = snapshot.offset_to_point(offset);
                    (
                        map.point_to_display_point(point, Bias::Left),
                        SelectionGoal::None,
                    )
                });
            },
        );
    }

    fn offset_for_vertical_move(
        &mut self,
        row_delta: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> usize {
        let display_snapshot = self.display_snapshot(cx);
        let buffer_snapshot = display_snapshot.buffer_snapshot();
        let cursor_offset = self.cursor_offset(cx).min(buffer_snapshot.len().0);
        let cursor_point = buffer_snapshot.offset_to_point(MultiBufferOffset(cursor_offset));
        let cursor_display_point =
            display_snapshot.point_to_display_point(cursor_point, Bias::Left);

        if self.masked {
            let current_line =
                buffer_line_text(display_snapshot.buffer_snapshot(), cursor_point.row);
            let column_bytes = (cursor_point.column as usize).min(current_line.len());
            let current_display_column = current_line
                .get(..column_bytes)
                .unwrap_or("")
                .chars()
                .count() as f64;
            let goal_column = match self.selection_goal {
                SelectionGoal::HorizontalPosition(x) => x,
                SelectionGoal::HorizontalRange { end, .. } => end,
                _ => current_display_column,
            };
            if matches!(self.selection_goal, SelectionGoal::None) {
                self.selection_goal = SelectionGoal::HorizontalPosition(goal_column);
            }
            let row_count = row_delta.unsigned_abs();
            let target_row = if row_delta.is_negative() {
                cursor_point.row.saturating_sub(row_count)
            } else {
                cursor_point
                    .row
                    .saturating_add(row_count)
                    .min(buffer_snapshot.max_point().row)
            };
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

        let text_layout_details = self.text_layout_details(window, cx);
        let (target_display_point, goal) = if row_delta.is_negative() {
            movement::up(
                &display_snapshot,
                cursor_display_point,
                self.selection_goal,
                false,
                &text_layout_details,
            )
        } else {
            movement::down(
                &display_snapshot,
                cursor_display_point,
                self.selection_goal,
                false,
                &text_layout_details,
            )
        };
        self.selection_goal = goal;
        let bias = if row_delta.is_negative() {
            Bias::Left
        } else {
            Bias::Right
        };
        let target_buffer_point =
            display_snapshot.display_point_to_point(target_display_point, bias);
        buffer_snapshot.point_to_offset(target_buffer_point).0
    }

    fn offset_for_horizontal_move(&mut self, direction: i32, cx: &mut Context<Self>) -> usize {
        let display_snapshot = self.display_snapshot(cx);
        let buffer_snapshot = display_snapshot.buffer_snapshot();
        let cursor_offset = self.cursor_offset(cx).min(buffer_snapshot.len().0);
        let cursor_point = buffer_snapshot.offset_to_point(MultiBufferOffset(cursor_offset));
        let display_point = display_snapshot.point_to_display_point(cursor_point, Bias::Left);

        if direction < 0 {
            let moved = movement::left(&display_snapshot, display_point);
            moved.to_offset(&display_snapshot, Bias::Left).0
        } else {
            let moved = movement::right(&display_snapshot, display_point);
            moved.to_offset(&display_snapshot, Bias::Right).0
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
        let target = movement::previous_word_start(&display_snapshot, display_point);
        target.to_offset(&display_snapshot, Bias::Left).0
    }

    fn previous_word_start_or_newline(&self, offset: usize, cx: &mut Context<Self>) -> usize {
        let display_snapshot = self.display_snapshot(cx);
        let buffer_snapshot = display_snapshot.buffer_snapshot();
        let offset = buffer_snapshot.clip_offset(
            MultiBufferOffset(offset.min(buffer_snapshot.len().0)),
            Bias::Left,
        );
        let point = buffer_snapshot.offset_to_point(offset);
        let display_point = display_snapshot.point_to_display_point(point, Bias::Left);
        let target = movement::previous_word_start_or_newline(&display_snapshot, display_point);
        target.to_offset(&display_snapshot, Bias::Left).0
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
        let target = movement::next_word_end(&display_snapshot, display_point);
        target.to_offset(&display_snapshot, Bias::Right).0
    }

    fn next_word_end_or_newline(&self, offset: usize, cx: &mut Context<Self>) -> usize {
        let display_snapshot = self.display_snapshot(cx);
        let buffer_snapshot = display_snapshot.buffer_snapshot();
        let offset = buffer_snapshot.clip_offset(
            MultiBufferOffset(offset.min(buffer_snapshot.len().0)),
            Bias::Right,
        );
        let point = buffer_snapshot.offset_to_point(offset);
        let display_point = display_snapshot.point_to_display_point(point, Bias::Right);
        let target = movement::next_word_end_or_newline(&display_snapshot, display_point);
        target.to_offset(&display_snapshot, Bias::Right).0
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
        let target = movement::previous_subword_start(&display_snapshot, display_point);
        target.to_offset(&display_snapshot, Bias::Left).0
    }

    fn previous_subword_start_or_newline(&self, offset: usize, cx: &mut Context<Self>) -> usize {
        let display_snapshot = self.display_snapshot(cx);
        let buffer_snapshot = display_snapshot.buffer_snapshot();
        let offset = buffer_snapshot.clip_offset(
            MultiBufferOffset(offset.min(buffer_snapshot.len().0)),
            Bias::Left,
        );
        let point = buffer_snapshot.offset_to_point(offset);
        let display_point = display_snapshot.point_to_display_point(point, Bias::Left);
        let target = movement::previous_subword_start_or_newline(&display_snapshot, display_point);
        target.to_offset(&display_snapshot, Bias::Left).0
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
        let target = movement::next_subword_end(&display_snapshot, display_point);
        target.to_offset(&display_snapshot, Bias::Right).0
    }

    fn next_subword_end_or_newline(&self, offset: usize, cx: &mut Context<Self>) -> usize {
        let display_snapshot = self.display_snapshot(cx);
        let buffer_snapshot = display_snapshot.buffer_snapshot();
        let offset = buffer_snapshot.clip_offset(
            MultiBufferOffset(offset.min(buffer_snapshot.len().0)),
            Bias::Right,
        );
        let point = buffer_snapshot.offset_to_point(offset);
        let display_point = display_snapshot.point_to_display_point(point, Bias::Right);
        let target = movement::next_subword_end_or_newline(&display_snapshot, display_point);
        target.to_offset(&display_snapshot, Bias::Right).0
    }

    fn line_indent_offset(&mut self, cx: &mut Context<Self>) -> usize {
        let display_snapshot = self.display_snapshot(cx);
        let buffer_snapshot = display_snapshot.buffer_snapshot();
        let cursor = buffer_snapshot.clip_offset(
            MultiBufferOffset(self.cursor_offset(cx).min(buffer_snapshot.len().0)),
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

    fn line_beginning_offset(&mut self, stop_at_indent: bool, cx: &mut Context<Self>) -> usize {
        let display_snapshot = self.display_snapshot(cx);
        let buffer_snapshot = display_snapshot.buffer_snapshot();
        let cursor = buffer_snapshot.clip_offset(
            MultiBufferOffset(self.cursor_offset(cx).min(buffer_snapshot.len().0)),
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

    fn line_end_offset(&mut self, cx: &mut Context<Self>) -> usize {
        let display_snapshot = self.display_snapshot(cx);
        let buffer_snapshot = display_snapshot.buffer_snapshot();
        let cursor = buffer_snapshot.clip_offset(
            MultiBufferOffset(self.cursor_offset(cx).min(buffer_snapshot.len().0)),
            Bias::Left,
        );
        let cursor_point = buffer_snapshot.offset_to_point(cursor);
        let line_start = buffer_snapshot.point_to_offset(text::Point::new(cursor_point.row, 0));
        let line_len = buffer_snapshot.line_len(MultiBufferRow(cursor_point.row)) as usize;
        (line_start + line_len).0
    }

    fn replace_range(&mut self, range: Range<usize>, new_text: &str, cx: &mut Context<Self>) {
        let snapshot = self.buffer_snapshot(cx);
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
        self.start_transaction_at(now, cx);

        let edit_range = range.start..range.end;
        self.buffer.update(cx, |buffer, cx| {
            buffer.edit([(edit_range.clone(), new_text)], cx);
        });
        let cursor = (range.start + new_text.len()).0;
        self.change_selections(SelectionEffects::no_scroll(), cx, |selections| {
            selections.select_ranges([MultiBufferOffset(cursor)..MultiBufferOffset(cursor)]);
        });
        self.selection_goal = SelectionGoal::None;
        self.request_autoscroll(scroll::Autoscroll::newest(), cx);
        self.end_transaction_at(now, cx);
        cx.emit(EditorEvent::BufferEdited);
        cx.notify();
    }

    fn replace_selection(&mut self, new_text: &str, cx: &mut Context<Self>) {
        let range = self.selected_range(cx);
        self.replace_range(range, new_text, cx);
    }

    fn text_offset_from_utf16(&self, utf16_offset: usize, cx: &App) -> usize {
        let snapshot = self.buffer_snapshot(cx);
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
        let snapshot = self.buffer_snapshot(cx);
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

    fn marked_text_ranges(&self, cx: &App) -> Option<Vec<Range<MultiBufferOffsetUtf16>>> {
        let (_, ranges) = self.text_highlights(HighlightKey::InputComposition, cx)?;
        let snapshot = self.buffer_snapshot(cx);
        Some(
            ranges
                .iter()
                .map(|range| {
                    snapshot.offset_to_offset_utf16(snapshot.offset_for_anchor(range.start))
                        ..snapshot.offset_to_offset_utf16(snapshot.offset_for_anchor(range.end))
                })
                .collect(),
        )
    }

    fn selection_replacement_ranges(
        &self,
        range: Range<MultiBufferOffsetUtf16>,
        cx: &mut Context<Self>,
    ) -> Vec<Range<MultiBufferOffsetUtf16>> {
        let display_snapshot = self.display_snapshot(cx);
        let selections = self
            .selections
            .all::<MultiBufferOffsetUtf16>(&display_snapshot);
        let newest_selection = self
            .selections
            .newest::<MultiBufferOffsetUtf16>(&display_snapshot);
        let start_delta = range.start.0.0 as isize - newest_selection.start.0.0 as isize;
        let end_delta = range.end.0.0 as isize - newest_selection.end.0.0 as isize;
        let snapshot = self.buffer_snapshot(cx);
        selections
            .into_iter()
            .map(|mut selection| {
                selection.start.0.0 =
                    (selection.start.0.0 as isize).saturating_add(start_delta) as usize;
                selection.end.0.0 = (selection.end.0.0 as isize).saturating_add(end_delta) as usize;
                snapshot.clip_offset_utf16(selection.start, Bias::Left)
                    ..snapshot.clip_offset_utf16(selection.end, Bias::Right)
            })
            .collect()
    }

    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.buffer.update(cx, |buffer, cx| {
            buffer
                .as_singleton()
                .expect("set_text requires a singleton buffer");
            buffer.set_text(text, cx);
        });
        let cursor = MultiBufferOffset(text.len());
        let mode = self.selection_history.mode;
        self.selection_history.mode = SelectionHistoryMode::Skipping;
        self.change_selections(SelectionEffects::no_scroll(), cx, |selections| {
            selections.select_ranges([cursor..cursor]);
        });
        self.selection_history.mode = mode;
        self.clear_highlights(HighlightKey::InputComposition, cx);
        self.selection_goal = SelectionGoal::None;
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
        let snapshot = self.buffer_snapshot(cx);
        if snapshot.is_empty() {
            return;
        }

        let now = Instant::now();
        self.start_transaction_at(now, cx);

        self.buffer.update(cx, |buffer, cx| {
            buffer.edit([(MultiBufferOffset::ZERO..snapshot.len(), "")], cx);
        });
        self.change_selections(SelectionEffects::no_scroll(), cx, |selections| {
            selections.select_ranges([MultiBufferOffset::ZERO..MultiBufferOffset::ZERO]);
        });
        self.clear_highlights(HighlightKey::InputComposition, cx);
        self.selection_goal = SelectionGoal::None;
        self.request_autoscroll(scroll::Autoscroll::newest(), cx);
        self.end_transaction_at(now, cx);
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
        self.selection_goal = SelectionGoal::None;
        cx.notify();
    }

    pub fn set_input_enabled(&mut self, input_enabled: bool) {
        self.input_enabled = input_enabled;
    }

    pub fn set_active_line_highlight(
        &mut self,
        active_line_highlight: Option<ActiveLineHighlight>,
    ) {
        self.active_line_highlight = active_line_highlight;
    }

    pub fn move_selection_to_end(&mut self, cx: &mut Context<Self>) {
        let offset = self.buffer_snapshot(cx).len().0;
        self.selection_goal = SelectionGoal::None;
        self.change_selections(
            SelectionEffects::scroll(scroll::Autoscroll::newest()),
            cx,
            |s| s.select_ranges([MultiBufferOffset(offset)..MultiBufferOffset(offset)]),
        );
    }

    fn move_left(&mut self, _: &MoveLeft, _: &mut Window, cx: &mut Context<Self>) {
        let selected_range = self.selected_range(cx);
        if selected_range.is_empty() {
            let offset = self.offset_for_horizontal_move(-1, cx);
            self.move_to(offset, cx);
        } else {
            self.move_to(selected_range.start, cx)
        }
    }

    fn move_right(&mut self, _: &MoveRight, _: &mut Window, cx: &mut Context<Self>) {
        let selected_range = self.selected_range(cx);
        if selected_range.is_empty() {
            let offset = self.offset_for_horizontal_move(1, cx);
            self.move_to(offset, cx);
        } else {
            self.move_to(selected_range.end, cx)
        }
    }

    fn move_up(&mut self, _: &MoveUp, window: &mut Window, cx: &mut Context<Self>) {
        let selected_range = self.selected_range(cx);
        if !selected_range.is_empty() {
            self.move_to(selected_range.start, cx);
            return;
        }

        let offset = self.offset_for_vertical_move(-1, window, cx);
        self.move_to_vertical(offset, cx);
    }

    fn move_down(&mut self, _: &MoveDown, window: &mut Window, cx: &mut Context<Self>) {
        let selected_range = self.selected_range(cx);
        if !selected_range.is_empty() {
            self.move_to(selected_range.end, cx);
            return;
        }

        let offset = self.offset_for_vertical_move(1, window, cx);
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

    fn select_up(&mut self, _: &SelectUp, window: &mut Window, cx: &mut Context<Self>) {
        let offset = self.offset_for_vertical_move(-1, window, cx);
        self.select_to_vertical(offset, cx);
    }

    fn select_down(&mut self, _: &SelectDown, window: &mut Window, cx: &mut Context<Self>) {
        let offset = self.offset_for_vertical_move(1, window, cx);
        self.select_to_vertical(offset, cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        let end = self.buffer_snapshot(cx).len();
        self.selection_goal = SelectionGoal::None;
        self.change_selections(SelectionEffects::no_scroll(), cx, |s| {
            s.select_ranges([MultiBufferOffset::ZERO..end]);
        });
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

        if self.selected_range(cx).is_empty() {
            let offset = self.line_beginning_offset(action.stop_at_indent, cx);
            let cursor = self.cursor_offset(cx);
            if cursor == offset {
                return;
            }
            self.change_selections(SelectionEffects::no_scroll(), cx, |s| {
                s.select_ranges([MultiBufferOffset(offset)..MultiBufferOffset(cursor)]);
            });
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

        if self.selected_range(cx).is_empty() {
            let cursor = self.cursor_offset(cx);
            let end = self.line_end_offset(cx);
            if cursor == end {
                return;
            }
            self.change_selections(SelectionEffects::no_scroll(), cx, |s| {
                s.select_ranges([MultiBufferOffset(cursor)..MultiBufferOffset(end)]);
            });
        }

        self.replace_selection("", cx);
    }

    fn move_to_beginning(&mut self, _: &MoveToBeginning, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn move_to_end(&mut self, _: &MoveToEnd, _: &mut Window, cx: &mut Context<Self>) {
        let offset = self.buffer_snapshot(cx).len().0;
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
        let offset = self.buffer_snapshot(cx).len().0;
        self.select_to(offset, cx);
    }

    fn move_to_previous_word_start(
        &mut self,
        _: &MoveToPreviousWordStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.previous_word_start(self.cursor_offset(cx), cx);
        self.move_to(offset, cx);
    }

    fn move_to_previous_subword_start(
        &mut self,
        _: &MoveToPreviousSubwordStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.previous_subword_start(self.cursor_offset(cx), cx);
        self.move_to(offset, cx);
    }

    fn move_to_next_word_end(
        &mut self,
        _: &MoveToNextWordEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.next_word_end(self.cursor_offset(cx), cx);
        self.move_to(offset, cx);
    }

    fn move_to_next_subword_end(
        &mut self,
        _: &MoveToNextSubwordEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.next_subword_end(self.cursor_offset(cx), cx);
        self.move_to(offset, cx);
    }

    fn select_to_previous_word_start(
        &mut self,
        _: &SelectToPreviousWordStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.previous_word_start(self.cursor_offset(cx), cx);
        self.select_to(offset, cx);
    }

    fn select_to_next_word_end(
        &mut self,
        _: &SelectToNextWordEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.next_word_end(self.cursor_offset(cx), cx);
        self.select_to(offset, cx);
    }

    fn select_to_previous_subword_start(
        &mut self,
        _: &SelectToPreviousSubwordStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.previous_subword_start(self.cursor_offset(cx), cx);
        self.select_to(offset, cx);
    }

    fn select_to_next_subword_end(
        &mut self,
        _: &SelectToNextSubwordEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = self.next_subword_end(self.cursor_offset(cx), cx);
        self.select_to(offset, cx);
    }

    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        if !self.input_enabled {
            return;
        }

        if self.selected_range(cx).is_empty() {
            let start = self.offset_for_horizontal_move(-1, cx);
            let end = self.cursor_offset(cx);
            self.change_selections(SelectionEffects::no_scroll(), cx, |s| {
                s.select_ranges([MultiBufferOffset(start)..MultiBufferOffset(end)]);
            });
        }
        self.replace_selection("", cx);
    }

    fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        if !self.input_enabled {
            return;
        }

        if self.selected_range(cx).is_empty() {
            let end = self.offset_for_horizontal_move(1, cx);
            let start = self.cursor_offset(cx);
            self.change_selections(SelectionEffects::no_scroll(), cx, |s| {
                s.select_ranges([MultiBufferOffset(start)..MultiBufferOffset(end)]);
            });
        }
        self.replace_selection("", cx);
    }

    fn delete_to_previous_word_start(
        &mut self,
        action: &DeleteToPreviousWordStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.input_enabled {
            return;
        }

        if self.selected_range(cx).is_empty() {
            let cursor_offset = self.cursor_offset(cx);
            let start = if action.ignore_newlines {
                self.previous_word_start(cursor_offset, cx)
            } else {
                self.previous_word_start_or_newline(cursor_offset, cx)
            };

            let display_snapshot = self.display_snapshot(cx);
            let buffer_snapshot = display_snapshot.buffer_snapshot();
            let cursor = buffer_snapshot.clip_offset(
                MultiBufferOffset(cursor_offset.min(buffer_snapshot.len().0)),
                Bias::Left,
            );
            let start = buffer_snapshot.clip_offset(
                MultiBufferOffset(start.min(buffer_snapshot.len().0)),
                Bias::Left,
            );

            let cursor_display_point = display_snapshot
                .point_to_display_point(buffer_snapshot.offset_to_point(cursor), Bias::Left);
            let start_display_point = display_snapshot
                .point_to_display_point(buffer_snapshot.offset_to_point(start), Bias::Left);
            let start = movement::adjust_greedy_deletion(
                &display_snapshot,
                cursor_display_point,
                start_display_point,
                action.ignore_brackets,
            )
            .to_offset(&display_snapshot, Bias::Left)
            .0;
            self.change_selections(SelectionEffects::no_scroll(), cx, |s| {
                s.select_ranges([MultiBufferOffset(start)..MultiBufferOffset(cursor_offset)]);
            });
        }
        self.replace_selection("", cx);
    }

    fn delete_to_previous_subword_start(
        &mut self,
        action: &DeleteToPreviousSubwordStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.input_enabled {
            return;
        }

        if self.selected_range(cx).is_empty() {
            let cursor_offset = self.cursor_offset(cx);
            let start = if action.ignore_newlines {
                self.previous_subword_start(cursor_offset, cx)
            } else {
                self.previous_subword_start_or_newline(cursor_offset, cx)
            };

            let display_snapshot = self.display_snapshot(cx);
            let buffer_snapshot = display_snapshot.buffer_snapshot();
            let cursor = buffer_snapshot.clip_offset(
                MultiBufferOffset(cursor_offset.min(buffer_snapshot.len().0)),
                Bias::Left,
            );
            let start = buffer_snapshot.clip_offset(
                MultiBufferOffset(start.min(buffer_snapshot.len().0)),
                Bias::Left,
            );

            let cursor_display_point = display_snapshot
                .point_to_display_point(buffer_snapshot.offset_to_point(cursor), Bias::Left);
            let start_display_point = display_snapshot
                .point_to_display_point(buffer_snapshot.offset_to_point(start), Bias::Left);
            let start = movement::adjust_greedy_deletion(
                &display_snapshot,
                cursor_display_point,
                start_display_point,
                action.ignore_brackets,
            )
            .to_offset(&display_snapshot, Bias::Left)
            .0;
            self.change_selections(SelectionEffects::no_scroll(), cx, |s| {
                s.select_ranges([MultiBufferOffset(start)..MultiBufferOffset(cursor_offset)]);
            });
        }
        self.replace_selection("", cx);
    }

    fn delete_to_next_word_end(
        &mut self,
        action: &DeleteToNextWordEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.input_enabled {
            return;
        }

        if self.selected_range(cx).is_empty() {
            let cursor_offset = self.cursor_offset(cx);
            let end = if action.ignore_newlines {
                self.next_word_end(cursor_offset, cx)
            } else {
                self.next_word_end_or_newline(cursor_offset, cx)
            };

            let display_snapshot = self.display_snapshot(cx);
            let buffer_snapshot = display_snapshot.buffer_snapshot();
            let cursor = buffer_snapshot.clip_offset(
                MultiBufferOffset(cursor_offset.min(buffer_snapshot.len().0)),
                Bias::Right,
            );
            let end = buffer_snapshot.clip_offset(
                MultiBufferOffset(end.min(buffer_snapshot.len().0)),
                Bias::Right,
            );

            let cursor_display_point = display_snapshot
                .point_to_display_point(buffer_snapshot.offset_to_point(cursor), Bias::Right);
            let end_display_point = display_snapshot
                .point_to_display_point(buffer_snapshot.offset_to_point(end), Bias::Right);
            let end = movement::adjust_greedy_deletion(
                &display_snapshot,
                cursor_display_point,
                end_display_point,
                action.ignore_brackets,
            )
            .to_offset(&display_snapshot, Bias::Right)
            .0;
            self.change_selections(SelectionEffects::no_scroll(), cx, |s| {
                s.select_ranges([MultiBufferOffset(cursor_offset)..MultiBufferOffset(end)]);
            });
        }
        self.replace_selection("", cx);
    }

    fn delete_to_next_subword_end(
        &mut self,
        action: &DeleteToNextSubwordEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.input_enabled {
            return;
        }

        if self.selected_range(cx).is_empty() {
            let cursor_offset = self.cursor_offset(cx);
            let end = if action.ignore_newlines {
                self.next_subword_end(cursor_offset, cx)
            } else {
                self.next_subword_end_or_newline(cursor_offset, cx)
            };

            let display_snapshot = self.display_snapshot(cx);
            let buffer_snapshot = display_snapshot.buffer_snapshot();
            let cursor = buffer_snapshot.clip_offset(
                MultiBufferOffset(cursor_offset.min(buffer_snapshot.len().0)),
                Bias::Right,
            );
            let end = buffer_snapshot.clip_offset(
                MultiBufferOffset(end.min(buffer_snapshot.len().0)),
                Bias::Right,
            );

            let cursor_display_point = display_snapshot
                .point_to_display_point(buffer_snapshot.offset_to_point(cursor), Bias::Right);
            let end_display_point = display_snapshot
                .point_to_display_point(buffer_snapshot.offset_to_point(end), Bias::Right);
            let end = movement::adjust_greedy_deletion(
                &display_snapshot,
                cursor_display_point,
                end_display_point,
                action.ignore_brackets,
            )
            .to_offset(&display_snapshot, Bias::Right)
            .0;
            self.change_selections(SelectionEffects::no_scroll(), cx, |s| {
                s.select_ranges([MultiBufferOffset(cursor_offset)..MultiBufferOffset(end)]);
            });
        }
        self.replace_selection("", cx);
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        let selected_range = self.selected_range(cx);
        if selected_range.is_empty() {
            return;
        }

        let current_text = self.buffer_snapshot(cx).text();
        let text = current_text
            .get(selected_range.clone())
            .unwrap_or("")
            .to_string();
        if text.is_empty() {
            return;
        }

        let snapshot = self.buffer_snapshot(cx);
        let start_offset = snapshot.clip_offset(
            MultiBufferOffset(selected_range.start.min(snapshot.len().0)),
            Bias::Left,
        );
        let end_offset = snapshot.clip_offset(
            MultiBufferOffset(selected_range.end.min(snapshot.len().0)),
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

        let selected_range = self.selected_range(cx);
        if selected_range.is_empty() {
            return;
        }

        let current_text = self.buffer_snapshot(cx).text();
        let text = current_text
            .get(selected_range.clone())
            .unwrap_or("")
            .to_string();
        if text.is_empty() {
            return;
        }

        let snapshot = self.buffer_snapshot(cx);
        let start_offset = snapshot.clip_offset(
            MultiBufferOffset(selected_range.start.min(snapshot.len().0)),
            Bias::Left,
        );
        let end_offset = snapshot.clip_offset(
            MultiBufferOffset(selected_range.end.min(snapshot.len().0)),
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

        let selected_range = self.selected_range(cx);
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

        if selected_range.is_empty() && handle_entire_lines && is_entire_line {
            let snapshot = self.buffer_snapshot(cx);
            let cursor = snapshot.clip_offset(
                MultiBufferOffset(self.cursor_offset(cx).min(snapshot.len().0)),
                Bias::Left,
            );
            let cursor_point = snapshot.offset_to_point(cursor);
            let line_start = snapshot.point_to_offset(text::Point::new(cursor_point.row, 0));
            self.change_selections(SelectionEffects::no_scroll(), cx, |s| {
                s.select_ranges([line_start..line_start]);
            });
        }

        let _ = window;
        let sanitized_text_to_insert = self.sanitize_to_single_line(text_to_insert);
        self.replace_selection(sanitized_text_to_insert.as_ref(), cx);
    }

    fn sanitize_to_single_line<'a>(&self, text: &'a str) -> Cow<'a, str> {
        if !matches!(self.mode, EditorMode::SingleLine) || !text.contains(['\n', '\r']) {
            return Cow::Borrowed(text);
        }

        Cow::Owned(
            text.chars()
                .filter(|char| !matches!(char, '\n' | '\r'))
                .collect(),
        )
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

        let sanitized_text = self.sanitize_to_single_line(text);
        self.replace_selection(sanitized_text.as_ref(), cx);
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        if !self.input_enabled {
            return;
        }

        let transaction_id = self.buffer.update(cx, |buffer, cx| buffer.undo(cx));
        let Some(transaction_id) = transaction_id else {
            return;
        };

        if let Some((selections, _)) = self
            .selection_history
            .selections_by_transaction
            .get(&transaction_id)
            .cloned()
        {
            self.change_selections(SelectionEffects::no_scroll(), cx, |s| {
                s.select_anchors(selections.to_vec());
            });
        }
        self.clear_highlights(HighlightKey::InputComposition, cx);
        self.ime_transaction = None;
        self.selection_goal = SelectionGoal::None;
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

        if let Some((_, Some(selections))) = self
            .selection_history
            .selections_by_transaction
            .get(&transaction_id)
            .cloned()
        {
            self.change_selections(SelectionEffects::no_scroll(), cx, |s| {
                s.select_anchors(selections.to_vec());
            });
        }
        self.clear_highlights(HighlightKey::InputComposition, cx);
        self.ime_transaction = None;
        self.selection_goal = SelectionGoal::None;
        self.request_autoscroll(scroll::Autoscroll::newest(), cx);
        cx.emit(EditorEvent::BufferEdited);
        cx.notify();
    }

    fn undo_selection(&mut self, _: &UndoSelection, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(entry) = self.selection_history.undo_stack.pop_back() {
            self.selection_history.mode = SelectionHistoryMode::Undoing;
            self.with_selection_effects_deferred(cx, |this, cx| {
                this.end_selection(cx);
                this.change_selections(
                    SelectionEffects::scroll(scroll::Autoscroll::newest()),
                    cx,
                    |s| {
                        s.select_anchors(entry.selections.to_vec());
                    },
                );
            });
            self.selection_history.mode = SelectionHistoryMode::Normal;
            self.selection_goal = SelectionGoal::None;
        }
    }

    fn redo_selection(&mut self, _: &RedoSelection, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(entry) = self.selection_history.redo_stack.pop_back() {
            self.selection_history.mode = SelectionHistoryMode::Redoing;
            self.with_selection_effects_deferred(cx, |this, cx| {
                this.end_selection(cx);
                this.change_selections(
                    SelectionEffects::scroll(scroll::Autoscroll::newest()),
                    cx,
                    |s| {
                        s.select_anchors(entry.selections.to_vec());
                    },
                );
            });
            self.selection_history.mode = SelectionHistoryMode::Normal;
            self.selection_goal = SelectionGoal::None;
        }
    }

    fn display_point_for_mouse_position(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> DisplayPoint {
        let display_snapshot = self.display_snapshot(cx);
        let text_len = display_snapshot.buffer_snapshot().len().0;
        let offset = self.index_for_mouse_position(position, cx).min(text_len);
        let point = display_snapshot
            .buffer_snapshot()
            .offset_to_point(MultiBufferOffset(offset));
        display_snapshot.point_to_display_point(point, Bias::Left)
    }

    fn begin_selection(
        &mut self,
        position: DisplayPoint,
        click_count: usize,
        cx: &mut Context<Self>,
    ) {
        let display_snapshot = self.display_snapshot(cx);
        let buffer = display_snapshot.buffer_snapshot();
        let position = display_snapshot.clip_point(position, Bias::Left);

        let (start, end, mode) = match click_count {
            1 => {
                let start = buffer.anchor_before(position.to_point(&display_snapshot));
                (start, start, SelectMode::Character)
            }
            2 => {
                let position = position.to_offset(&display_snapshot, Bias::Left);
                let (word_range, _) = buffer.surrounding_word(position, None);
                let start = buffer.anchor_before(word_range.start);
                let end = buffer.anchor_before(word_range.end);
                (start, end, SelectMode::Word(start..end))
            }
            3 => {
                let position = position.to_point(&display_snapshot);
                let line_start = text::Point::new(position.row, 0);
                let next_line_start = buffer.clip_point(
                    text::Point::new(position.row.saturating_add(1), 0),
                    Bias::Left,
                );
                let start = buffer.anchor_before(line_start);
                let end = buffer.anchor_before(next_line_start);
                (start, end, SelectMode::Line(start..end))
            }
            _ => {
                let start = buffer.anchor_before(MultiBufferOffset::ZERO);
                let end = buffer.anchor_before(buffer.len());
                (start, end, SelectMode::All)
            }
        };

        self.change_selections(SelectionEffects::no_scroll(), cx, |selections| {
            selections.clear_disjoint();
            selections.set_pending_anchor_range(start..end, mode);
        });
    }

    fn extend_selection(
        &mut self,
        position: DisplayPoint,
        click_count: usize,
        cx: &mut Context<Self>,
    ) {
        let display_snapshot = self.display_snapshot(cx);
        let tail = self
            .selections
            .newest::<MultiBufferOffset>(&display_snapshot)
            .tail();
        let click_count = click_count.max(match self.selections.select_mode() {
            SelectMode::Character => 1,
            SelectMode::Word(_) => 2,
            SelectMode::Line(_) => 3,
            SelectMode::All => 4,
        });
        self.begin_selection(position, click_count, cx);

        let tail_anchor = display_snapshot.buffer_snapshot().anchor_before(tail);
        let current_selection = match self.selections.select_mode() {
            SelectMode::Character | SelectMode::All => tail_anchor..tail_anchor,
            SelectMode::Word(range) | SelectMode::Line(range) => range.clone(),
        };
        let Some(mut pending_selection) = self.selections.pending_anchor().cloned() else {
            return;
        };

        let snapshot = display_snapshot.buffer_snapshot();
        if snapshot.offset_for_anchor(pending_selection.start)
            > snapshot.offset_for_anchor(current_selection.start)
        {
            pending_selection.start = current_selection.start;
        }
        if snapshot.offset_for_anchor(pending_selection.end)
            < snapshot.offset_for_anchor(current_selection.end)
        {
            pending_selection.end = current_selection.end;
            pending_selection.reversed = true;
        }

        let mut pending_mode = self
            .selections
            .pending_mode()
            .unwrap_or(SelectMode::Character);
        match &mut pending_mode {
            SelectMode::Word(range) | SelectMode::Line(range) => *range = current_selection,
            SelectMode::Character | SelectMode::All => {}
        }

        self.change_selections(SelectionEffects::no_scroll(), cx, |selections| {
            selections.set_pending(pending_selection.clone(), pending_mode);
            selections.set_is_extending(true);
        });
    }

    fn update_selection(&mut self, position: DisplayPoint, cx: &mut Context<Self>) {
        let display_snapshot = self.display_snapshot(cx);
        let Some(mut pending) = self.selections.pending_anchor().cloned() else {
            return;
        };

        let buffer = display_snapshot.buffer_snapshot();
        let mode = self
            .selections
            .pending_mode()
            .unwrap_or(SelectMode::Character);

        let (head, tail) = match &mode {
            SelectMode::Character => (
                position.to_point(&display_snapshot),
                buffer.point_for_anchor(pending.tail()),
            ),
            SelectMode::Word(original_range) => {
                let offset = position.to_offset(&display_snapshot, Bias::Left);
                let original_start = buffer.offset_for_anchor(original_range.start);
                let original_end = buffer.offset_for_anchor(original_range.end);

                let head_offset = if buffer.is_inside_word(offset, None)
                    || (original_start..original_end).contains(&offset)
                {
                    let (word_range, _) = buffer.surrounding_word(offset, None);
                    if word_range.start < original_start {
                        word_range.start
                    } else {
                        word_range.end
                    }
                } else {
                    offset
                };

                let head = buffer.offset_to_point(head_offset);
                let tail = if head_offset <= original_start {
                    buffer.offset_to_point(original_end)
                } else {
                    buffer.offset_to_point(original_start)
                };
                (head, tail)
            }
            SelectMode::Line(original_range) => {
                let original_start = buffer.point_for_anchor(original_range.start);
                let original_end = buffer.point_for_anchor(original_range.end);

                let position = display_snapshot.clip_point(position, Bias::Left);
                let position = position.to_point(&display_snapshot);
                let line_start = text::Point::new(position.row, 0);
                let next_line_start = buffer.clip_point(
                    text::Point::new(position.row.saturating_add(1), 0),
                    Bias::Left,
                );

                let head = if line_start < original_start {
                    line_start
                } else {
                    next_line_start
                };
                let tail = if head <= original_start {
                    original_end
                } else {
                    original_start
                };
                (head, tail)
            }
            SelectMode::All => {
                return;
            }
        };

        if head < tail {
            pending.start = buffer.anchor_before(head);
            pending.end = buffer.anchor_before(tail);
            pending.reversed = true;
        } else {
            pending.start = buffer.anchor_before(tail);
            pending.end = buffer.anchor_before(head);
            pending.reversed = false;
        }

        self.change_selections(SelectionEffects::no_scroll(), cx, |selections| {
            selections.set_pending(pending.clone(), mode);
        });
    }

    fn end_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(pending_mode) = self.selections.pending_mode() {
            let selections = self
                .selections
                .all::<MultiBufferOffset>(&self.display_snapshot(cx));
            self.change_selections(SelectionEffects::no_scroll(), cx, |selections_collection| {
                selections_collection.select(selections);
                selections_collection.clear_pending();
                if selections_collection.is_extending() {
                    selections_collection.set_is_extending(false);
                } else {
                    selections_collection.set_select_mode(pending_mode);
                }
            });
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
        self.selection_goal = SelectionGoal::None;
        let position = self.display_point_for_mouse_position(event.position, cx);

        if event.modifiers.shift {
            self.extend_selection(position, event.click_count, cx);
        } else {
            self.begin_selection(position, event.click_count, cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.selecting = false;
        self.end_selection(cx);
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.selecting {
            self.selection_goal = SelectionGoal::None;
            let position = self.display_point_for_mouse_position(event.position, cx);
            self.update_selection(position, cx);
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

        let selected_range = self.selected_range(cx);
        if self.mode == EditorMode::SingleLine
            && selected_range.is_empty()
            && selected_range.end == self.buffer_snapshot(cx).len().0
        {
            key_context.add("end_of_input");
        }

        for addon in self.addons.values() {
            addon.extend_key_context(&mut key_context, cx);
        }

        key_context
    }

    pub fn text_layout_details(
        &self,
        window: &mut Window,
        cx: &mut App,
    ) -> movement::TextLayoutDetails {
        movement::TextLayoutDetails {
            text_system: window.text_system().clone(),
            editor_style: self.create_style(cx),
            rem_size: window.rem_size(),
        }
    }

    pub(crate) fn create_style(&self, cx: &App) -> EditorStyle {
        let theme_colors = cx.theme().colors();
        let theme_settings = ThemeSettings::get_global(cx);

        let font_size = match self.mode {
            EditorMode::SingleLine | EditorMode::AutoHeight { .. } => gpui::rems(0.875).into(),
            EditorMode::Full { .. } => (theme_settings.buffer_font_size(cx) * 0.875).into(),
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

    fn highlight_text(
        &mut self,
        key: HighlightKey,
        ranges: Vec<Range<Anchor>>,
        style: gpui::HighlightStyle,
        cx: &mut Context<Self>,
    ) {
        self.display_map.update(cx, |map, cx| {
            map.highlight_text(key, ranges, style, false, cx);
        });
        cx.notify();
    }

    fn text_highlights(
        &self,
        key: HighlightKey,
        cx: &App,
    ) -> Option<(gpui::HighlightStyle, Vec<Range<Anchor>>)> {
        let map = self.display_map.read(cx);
        map.text_highlights(key)
            .map(|(style, ranges)| (style, ranges.to_vec()))
    }

    fn clear_highlights(&mut self, key: HighlightKey, cx: &mut Context<Self>) {
        let cleared = self
            .display_map
            .update(cx, |map, _| map.clear_highlights(key));
        if cleared {
            cx.notify();
        }
    }

    fn marked_range(&self, cx: &App) -> Option<Range<usize>> {
        let snapshot = self.buffer_snapshot(cx);
        let (_, ranges) = self.text_highlights(HighlightKey::InputComposition, cx)?;
        let range = ranges.first()?;
        Some(snapshot.offset_for_anchor(range.start).0..snapshot.offset_for_anchor(range.end).0)
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
        let snapshot = self.buffer_snapshot(cx);
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

        let selection = self.selection_utf16(cx);
        let range = selection.range();
        Some(UTF16Selection {
            range: range.start.0.0..range.end.0.0,
            reversed: selection.reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        let snapshot = self.buffer_snapshot(cx);
        let (_, ranges) = self.text_highlights(HighlightKey::InputComposition, cx)?;
        let range = ranges.first()?;
        Some(
            snapshot
                .offset_to_offset_utf16(snapshot.offset_for_anchor(range.start))
                .0
                .0
                ..snapshot
                    .offset_to_offset_utf16(snapshot.offset_for_anchor(range.end))
                    .0
                    .0,
        )
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.clear_highlights(HighlightKey::InputComposition, cx);
        self.ime_transaction.take();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.input_enabled {
            return;
        }

        self.transact(window, cx, |this, window, cx| {
            let new_selected_ranges = if let Some(range_utf16) = range_utf16 {
                let range_utf16 = MultiBufferOffsetUtf16(OffsetUtf16(range_utf16.start))
                    ..MultiBufferOffsetUtf16(OffsetUtf16(range_utf16.end));
                Some(this.selection_replacement_ranges(range_utf16, cx))
            } else {
                this.marked_text_ranges(cx)
            };

            if let Some(new_selected_ranges) = new_selected_ranges {
                this.change_selections(SelectionEffects::no_scroll(), cx, |selections| {
                    selections.select_ranges(new_selected_ranges)
                });
                this.backspace(&Default::default(), window, cx);
            }

            this.handle_input(new_text, window, cx);
        });

        if let Some(transaction) = self.ime_transaction {
            self.group_until_transaction(transaction, cx);
        }

        self.unmark_text(window, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.input_enabled {
            return;
        }

        let transaction = self.transact(window, cx, |this, window, cx| {
            let ranges_to_replace = if let Some(mut marked_ranges) = this.marked_text_ranges(cx) {
                let snapshot = this.buffer_snapshot(cx);
                if let Some(relative_range_utf16) = range_utf16.as_ref() {
                    for marked_range in &mut marked_ranges {
                        marked_range.end = marked_range.start + relative_range_utf16.end;
                        marked_range.start += relative_range_utf16.start;
                        marked_range.start =
                            snapshot.clip_offset_utf16(marked_range.start, Bias::Left);
                        marked_range.end =
                            snapshot.clip_offset_utf16(marked_range.end, Bias::Right);
                    }
                }
                Some(marked_ranges)
            } else if let Some(range_utf16) = range_utf16 {
                let range_utf16 = MultiBufferOffsetUtf16(OffsetUtf16(range_utf16.start))
                    ..MultiBufferOffsetUtf16(OffsetUtf16(range_utf16.end));
                Some(this.selection_replacement_ranges(range_utf16, cx))
            } else {
                None
            };

            if let Some(ranges) = ranges_to_replace {
                this.change_selections(SelectionEffects::no_scroll(), cx, |selections| {
                    selections.select_ranges(ranges)
                });
            }

            let marked_ranges = {
                let snapshot = this.buffer_snapshot(cx);
                this.selections
                    .disjoint_anchors()
                    .iter()
                    .map(|selection| {
                        selection.start.bias_left(&snapshot)..selection.end.bias_right(&snapshot)
                    })
                    .collect::<Vec<_>>()
            };

            if new_text.is_empty() {
                this.unmark_text(window, cx);
            } else {
                this.highlight_text(
                    HighlightKey::InputComposition,
                    marked_ranges.clone(),
                    gpui::HighlightStyle {
                        underline: Some(gpui::UnderlineStyle {
                            thickness: gpui::px(1.0),
                            color: None,
                            wavy: false,
                        }),
                        ..Default::default()
                    },
                    cx,
                );
            }

            this.handle_input(new_text, window, cx);

            if let Some(new_selected_range) = new_selected_range_utf16 {
                let snapshot = this.buffer_snapshot(cx);
                let new_selected_ranges = marked_ranges
                    .into_iter()
                    .map(|marked_range| {
                        let insertion_start = snapshot
                            .offset_to_offset_utf16(snapshot.offset_for_anchor(marked_range.start))
                            .0;
                        let new_start = MultiBufferOffsetUtf16(OffsetUtf16(
                            insertion_start.0 + new_selected_range.start,
                        ));
                        let new_end = MultiBufferOffsetUtf16(OffsetUtf16(
                            insertion_start.0 + new_selected_range.end,
                        ));
                        snapshot.clip_offset_utf16(new_start, Bias::Left)
                            ..snapshot.clip_offset_utf16(new_end, Bias::Right)
                    })
                    .collect::<Vec<_>>();
                this.change_selections(SelectionEffects::no_scroll(), cx, |selections| {
                    selections.select_ranges(new_selected_ranges)
                });
            }
        });

        self.ime_transaction = self.ime_transaction.or(transaction);
        if let Some(transaction) = self.ime_transaction {
            self.group_until_transaction(transaction, cx);
        }

        if self
            .text_highlights(HighlightKey::InputComposition, cx)
            .is_none()
        {
            self.ime_transaction.take();
        }
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
        self.0.read(cx).buffer_snapshot(cx).text()
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

    fn render(&self, _window: &mut Window, cx: &App) -> AnyElement {
        let theme_colors = cx.theme().colors();
        let theme_settings = ThemeSettings::get_global(cx);

        let text_style = TextStyle {
            font_family: theme_settings.ui_font.family.clone(),
            font_features: theme_settings.ui_font.features.clone(),
            font_size: gpui::rems(0.875).into(),
            font_weight: theme_settings.buffer_font.weight,
            font_style: FontStyle::Normal,
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
