use gpui::{
    Action, AnyWindowHandle, Context, Entity, TestAppContext, VisualTestContext, Window, prelude::*,
};
use std::{
    collections::BTreeMap,
    ops::{Deref, DerefMut},
    sync::{
        Arc, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use language::Buffer;
use multi_buffer::{MultiBuffer, MultiBufferOffset, ToOffset};
use text::SelectionGoal;
use util::test::{generate_marked_text, marked_text_ranges};

use crate::{
    DEFAULT_TAB_SIZE, Editor, EditorMode, SelectionEffects, SelectionHistory,
    display_map::{DisplayMap, HighlightKey},
};

pub struct EditorTestContext {
    pub cx: VisualTestContext,
    pub window: AnyWindowHandle,
    pub editor: Entity<Editor>,
    pub assertion_cx: AssertionContextManager,
}

impl EditorTestContext {
    pub fn new(cx: &mut TestAppContext) -> Self {
        Self::new_with_mode(cx, EditorMode::full())
    }

    pub fn new_single_line(cx: &mut TestAppContext) -> Self {
        Self::new_with_mode(cx, EditorMode::SingleLine)
    }

    fn new_with_mode(cx: &mut TestAppContext, mode: EditorMode) -> Self {
        let window_handle = cx
            .add_window(move |window, cx| Editor::new(mode, Editor::empty_buffer(cx), window, cx));
        let window: AnyWindowHandle = window_handle.into();
        let editor_handle = window
            .downcast::<Editor>()
            .expect("window should contain editor");
        let mut visual_cx = VisualTestContext::from_window(window, cx);
        let editor = editor_handle
            .root(&mut visual_cx)
            .expect("editor root should be available");
        let window = visual_cx
            .windows()
            .first()
            .copied()
            .expect("visual context should have a window");

        let focus_handle = editor.read_with(&visual_cx, |editor, _| editor.focus_handle.clone());
        visual_cx.update(|window, cx| focus_handle.focus(window, cx));

        Self {
            cx: visual_cx,
            window,
            editor,
            assertion_cx: AssertionContextManager::new(),
        }
    }

    pub fn for_editor_in(editor: Entity<Editor>, cx: &mut VisualTestContext) -> Self {
        cx.focus(&editor);
        Self {
            window: cx
                .windows()
                .first()
                .copied()
                .expect("visual context should have a window"),
            cx: cx.clone(),
            editor,
            assertion_cx: AssertionContextManager::new(),
        }
    }

    pub fn add_assertion_context(&self, context: String) -> ContextHandle {
        self.assertion_cx.add_context(context)
    }

    pub fn update_editor<F, T>(&mut self, update: F) -> T
    where
        F: FnOnce(&mut Editor, &mut Window, &mut Context<Editor>) -> T,
    {
        self.editor.update_in(&mut self.cx, update)
    }

    pub fn dispatch_action<A>(&mut self, action: A)
    where
        A: Action,
    {
        self.cx.dispatch_action(action);
    }

    #[track_caller]
    pub fn set_state(&mut self, marked_text: &str) -> ContextHandle {
        let assertion_context = self.add_assertion_context(format!(
            "Initial Editor State: \"{}\"",
            marked_text.escape_debug()
        ));
        let (text, mut ranges) = marked_text_ranges(marked_text, true);
        let selection = ranges.pop().unwrap_or(0..0);
        assert!(ranges.is_empty(), "expected a single selection range");

        self.update_editor(|editor, _, cx| {
            let text_buffer = cx.new(|cx| Buffer::local(text.as_str(), cx));
            let buffer = cx.new(|cx| MultiBuffer::singleton(text_buffer.clone(), cx));
            editor.buffer = buffer.clone();
            editor.display_map = cx.new(|cx| DisplayMap::new(buffer, DEFAULT_TAB_SIZE, cx));
            editor.change_selections(SelectionEffects::no_scroll(), cx, |selections| {
                selections.select_ranges([
                    MultiBufferOffset(selection.start)..MultiBufferOffset(selection.end)
                ]);
            });
            editor.clear_highlights(HighlightKey::InputComposition, cx);
            editor.ime_transaction = None;
            editor.selection_goal = SelectionGoal::None;
            editor.selection_history =
                SelectionHistory::new(editor.selections.disjoint_anchors_arc());
            editor.clear_last_position_map();
        });

        assertion_context
    }

    #[track_caller]
    pub fn assert_state(&mut self, marked_text: &str) {
        let (expected_text, mut ranges) = marked_text_ranges(marked_text, true);
        let expected_selection = ranges.pop().unwrap_or(0..0);
        assert!(ranges.is_empty(), "expected a single selection range");

        let (actual_text, actual_selection) = self.editor.read_with(&self.cx, |editor, cx| {
            (editor.buffer_snapshot(cx).text(), editor.selection(cx))
        });

        let assertion_context = self.assertion_cx.context();
        pretty_assertions::assert_eq!(
            actual_text,
            expected_text,
            "{assertion_context}text does not match"
        );

        let actual_range = if actual_selection.reversed {
            actual_selection.end.0..actual_selection.start.0
        } else {
            actual_selection.start.0..actual_selection.end.0
        };

        let actual_marked =
            generate_marked_text(&actual_text, &[actual_range], marked_text.contains('«'));

        let expected_marked = generate_marked_text(
            &expected_text,
            &[expected_selection],
            marked_text.contains('«'),
        );

        pretty_assertions::assert_eq!(
            actual_marked,
            expected_marked,
            "{assertion_context}selection does not match"
        );
    }

    #[track_caller]
    pub fn assert_variable_highlights(
        &mut self,
        marked_text: &str,
        keys: impl IntoIterator<Item = HighlightKey>,
    ) {
        let (unmarked_text, marked_ranges) = marked_text_ranges(marked_text, false);
        let keys = keys.into_iter().collect::<Vec<_>>();
        pretty_assertions::assert_eq!(
            keys.len(),
            marked_ranges.len(),
            "each marked range needs a highlight key"
        );
        self.editor.read_with(&self.cx, |editor, cx| {
            let snapshot = editor.buffer_snapshot(cx);
            pretty_assertions::assert_eq!(snapshot.text(), unmarked_text, "text doesn't match");
            for key in [
                HighlightKey::VariableDefined,
                HighlightKey::VariableUndefined,
            ] {
                let expected_ranges = marked_ranges
                    .iter()
                    .zip(&keys)
                    .filter(|(_, marked_key)| **marked_key == key)
                    .map(|(range, _)| range.clone())
                    .collect::<Vec<_>>();
                let ranges = editor
                    .text_highlights(key, cx)
                    .map(|(_, ranges)| {
                        ranges
                            .iter()
                            .map(|range| {
                                range.start.to_offset(&snapshot).0..range.end.to_offset(&snapshot).0
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                pretty_assertions::assert_eq!(
                    ranges,
                    expected_ranges,
                    "{key:?} highlights don't match"
                );
            }
        });
    }
}

impl Deref for EditorTestContext {
    type Target = VisualTestContext;

    fn deref(&self) -> &Self::Target {
        &self.cx
    }
}

impl DerefMut for EditorTestContext {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.cx
    }
}

#[derive(Clone)]
pub struct AssertionContextManager {
    id: Arc<AtomicUsize>,
    contexts: Arc<RwLock<BTreeMap<usize, String>>>,
}

impl AssertionContextManager {
    fn new() -> Self {
        Self {
            id: Arc::new(AtomicUsize::new(0)),
            contexts: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    fn add_context(&self, context: String) -> ContextHandle {
        let id = self.id.fetch_add(1, Ordering::Relaxed);
        let mut contexts = self
            .contexts
            .write()
            .expect("assertion contexts should not be poisoned");
        contexts.insert(id, context);
        ContextHandle {
            id,
            manager: self.clone(),
        }
    }

    fn context(&self) -> String {
        let contexts = self
            .contexts
            .read()
            .expect("assertion contexts should not be poisoned");
        let joined = contexts.values().cloned().collect::<Vec<_>>().join("\n");
        format!("\n{joined}\n")
    }
}

pub struct ContextHandle {
    id: usize,
    manager: AssertionContextManager,
}

impl Drop for ContextHandle {
    fn drop(&mut self) {
        let mut contexts = self
            .manager
            .contexts
            .write()
            .expect("assertion contexts should not be poisoned");
        contexts.remove(&self.id);
    }
}
