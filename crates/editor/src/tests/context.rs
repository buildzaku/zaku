use gpui::{
    Action, AnyWindowHandle, AppContext, Context, Entity, TestAppContext, VisualTestContext, Window,
};
use multi_buffer::MultiBuffer;
use pretty_assertions::assert_eq;
use std::{
    collections::BTreeMap,
    sync::{
        Arc, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
};
use text::{Buffer as TextBuffer, ReplicaId};

use util::test::{generate_marked_text, marked_text_ranges};

use crate::{DEFAULT_TAB_SIZE, Editor, SelectionHistory, SelectionState, next_buffer_id};

pub struct EditorTestContext {
    pub cx: VisualTestContext,
    pub editor: Entity<Editor>,
    pub assertion_cx: AssertionContextManager,
}

impl EditorTestContext {
    pub fn new(cx: &mut TestAppContext) -> Self {
        let window_handle = cx.add_window(Editor::single_line);
        let window: AnyWindowHandle = window_handle.into();
        let editor_handle = window.downcast::<Editor>().expect("window to host editor");
        let mut visual_cx = VisualTestContext::from_window(window, cx);
        let editor = editor_handle.root(&mut visual_cx).expect("editor root");

        let focus_handle = editor.read_with(&visual_cx, |editor, _| editor.focus_handle.clone());
        visual_cx.update(|window, cx| focus_handle.focus(window, cx));

        Self {
            cx: visual_cx,
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
        if !ranges.is_empty() {
            panic!("expected a single selection range");
        }

        let (selected_range, selection_reversed) = if selection.start <= selection.end {
            (selection, false)
        } else {
            (selection.end..selection.start, true)
        };

        self.update_editor(|editor, _, cx| {
            let text_buffer =
                cx.new(|_| TextBuffer::new(ReplicaId::LOCAL, next_buffer_id(), text.as_str()));
            let buffer = cx.new(|cx| MultiBuffer::singleton(text_buffer.clone(), cx));
            editor.buffer = buffer.clone();
            editor.display_map =
                cx.new(|cx| crate::display_map::DisplayMap::new(buffer, DEFAULT_TAB_SIZE, cx));
            editor.selected_range = selected_range.clone();
            editor.selection_reversed = selection_reversed;
            editor.marked_range = None;
            editor.selection_history = SelectionHistory::new(SelectionState {
                range: selected_range,
                reversed: selection_reversed,
            });
            editor.last_position_map = None;
        });

        assertion_context
    }

    #[track_caller]
    pub fn assert_state(&mut self, marked_text: &str) {
        let (expected_text, mut ranges) = marked_text_ranges(marked_text, true);
        let expected_selection = ranges.pop().unwrap_or(0..0);
        if !ranges.is_empty() {
            panic!("expected a single selection range");
        }

        let (actual_text, actual_selection, actual_reversed) =
            self.editor.read_with(&self.cx, |editor, cx| {
                (
                    editor.snapshot(cx).text(),
                    editor.selected_range.clone(),
                    editor.selection_reversed,
                )
            });

        let assertion_context = self.assertion_cx.context();
        assert_eq!(
            actual_text, expected_text,
            "{}text does not match",
            assertion_context
        );

        let actual_range = if actual_reversed {
            actual_selection.end..actual_selection.start
        } else {
            actual_selection
        };

        let actual_marked =
            generate_marked_text(&actual_text, &[actual_range], marked_text.contains('«'));

        let expected_marked = generate_marked_text(
            &expected_text,
            &[expected_selection],
            marked_text.contains('«'),
        );

        assert_eq!(
            actual_marked, expected_marked,
            "{}selection does not match",
            assertion_context
        );
    }
}

#[derive(Clone)]
pub struct AssertionContextManager {
    id: Arc<AtomicUsize>,
    contexts: Arc<RwLock<BTreeMap<usize, String>>>,
}

impl AssertionContextManager {
    pub fn new() -> Self {
        Self {
            id: Arc::new(AtomicUsize::new(0)),
            contexts: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    pub fn add_context(&self, context: String) -> ContextHandle {
        let id = self.id.fetch_add(1, Ordering::Relaxed);
        let mut contexts = self
            .contexts
            .write()
            .expect("assertion context lock poisoned");
        contexts.insert(id, context);
        ContextHandle {
            id,
            manager: self.clone(),
        }
    }

    pub fn context(&self) -> String {
        let contexts = self
            .contexts
            .read()
            .expect("assertion context lock poisoned");
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
            .expect("assertion context lock poisoned");
        contexts.remove(&self.id);
    }
}
