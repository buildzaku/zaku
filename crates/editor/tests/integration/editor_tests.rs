use gpui::{
    Action, AnyWindowHandle, AppContext, ClipboardItem, Context, Entity, EntityInputHandler, Point,
    TestAppContext, VisualTestContext, Window,
};
use indoc::indoc;
use pretty_assertions::assert_eq;
use std::{
    collections::BTreeMap,
    ops::{Deref, DerefMut, Range},
    sync::{
        Arc, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use language::{Buffer, Capability};
use multi_buffer::{MultiBuffer, MultiBufferOffset};
use settings::SettingsStore;
use text::SelectionGoal;
use util::test::{generate_marked_text, marked_text_ranges};

use editor::{
    DEFAULT_TAB_SIZE, Editor, EditorMode, SelectionEffects, SelectionHistory,
    display_map::{DisplayMap, DisplayPoint, DisplayRow, HighlightKey},
};

fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test_new(cx);
        cx.set_global(settings_store);
        theme::init(theme::LoadThemes::JustBase, cx);
        editor::init(cx);
    });
}

fn display_ranges(editor: &Editor, cx: &mut Context<'_, Editor>) -> Vec<Range<DisplayPoint>> {
    let snapshot = editor.display_snapshot(cx);
    editor
        .selections
        .all_display(&snapshot)
        .into_iter()
        .map(|selection| {
            if selection.reversed {
                selection.end..selection.start
            } else {
                selection.start..selection.end
            }
        })
        .collect()
}

struct EditorTestContext {
    cx: VisualTestContext,
    window: AnyWindowHandle,
    editor: Entity<Editor>,
    assertion_cx: AssertionContextManager,
}

impl EditorTestContext {
    fn new(cx: &mut TestAppContext) -> Self {
        Self::new_with_mode(cx, EditorMode::full())
    }

    fn new_single_line(cx: &mut TestAppContext) -> Self {
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

    fn add_assertion_context(&self, context: String) -> ContextHandle {
        self.assertion_cx.add_context(context)
    }

    fn update_editor<F, T>(&mut self, update: F) -> T
    where
        F: FnOnce(&mut Editor, &mut Window, &mut Context<Editor>) -> T,
    {
        self.editor.update_in(&mut self.cx, update)
    }

    fn dispatch_action<A>(&mut self, action: A)
    where
        A: Action,
    {
        self.cx.dispatch_action(action);
    }

    #[track_caller]
    fn set_state(&mut self, marked_text: &str) -> ContextHandle {
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
    fn assert_state(&mut self, marked_text: &str) {
        let (expected_text, mut ranges) = marked_text_ranges(marked_text, true);
        let expected_selection = ranges.pop().unwrap_or(0..0);
        assert!(ranges.is_empty(), "expected a single selection range");

        let (actual_text, actual_selection) = self.editor.read_with(&self.cx, |editor, cx| {
            (editor.buffer_snapshot(cx).text(), editor.selection(cx))
        });

        let assertion_context = self.assertion_cx.context();
        assert_eq!(
            actual_text, expected_text,
            "{}text does not match",
            assertion_context
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

        assert_eq!(
            actual_marked, expected_marked,
            "{}selection does not match",
            assertion_context
        );
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
struct AssertionContextManager {
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

struct ContextHandle {
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

#[gpui::test]
fn test_handle_input(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state(indoc! {"
        one
        twoˇ
        three
        four
        five
    "});

    cx.dispatch_action(actions::editor::HandleInput(String::new()));
    cx.assert_state(indoc! {"
        one
        twoˇ
        three
        four
        five
    "});

    cx.dispatch_action(actions::editor::HandleInput("X".to_string()));
    cx.assert_state(indoc! {"
        one
        twoXˇ
        three
        four
        five
    "});
}

#[gpui::test]
fn test_read_only_capability(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state("The quick brownˇ");

    cx.editor.read_with(&cx.cx, |editor, cx| {
        assert_eq!(editor.capability(cx), Capability::ReadWrite);
        assert!(!editor.read_only(cx));
    });

    cx.update_editor(|editor, _, _| editor.set_read_only(true));

    cx.editor.read_with(&cx.cx, |editor, cx| {
        assert_eq!(editor.capability(cx), Capability::ReadOnly);
        assert!(editor.read_only(cx));
    });

    cx.dispatch_action(actions::editor::HandleInput(" fox".to_string()));
    cx.assert_state("The quick brownˇ");

    cx.dispatch_action(actions::editor::Backspace);
    cx.assert_state("The quick brownˇ");

    cx.dispatch_action(actions::editor::Delete);
    cx.assert_state("The quick brownˇ");

    cx.dispatch_action(actions::editor::MoveLeft);
    cx.assert_state("The quick browˇn");

    cx.dispatch_action(actions::editor::MoveRight);
    cx.assert_state("The quick brownˇ");

    cx.update_editor(|editor, _, _| editor.set_read_only(false));
    cx.editor.read_with(&cx.cx, |editor, cx| {
        assert_eq!(editor.capability(cx), Capability::ReadWrite);
        assert!(!editor.read_only(cx));
    });

    cx.dispatch_action(actions::editor::HandleInput(" fox".to_string()));
    cx.assert_state("The quick brown foxˇ");
}

#[gpui::test]
fn test_handle_input_replaces_selection(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state("Hello, «worldˇ»!");
    cx.dispatch_action(actions::editor::HandleInput("from Zaku".to_string()));
    cx.assert_state("Hello, from Zakuˇ!");

    cx.set_state(indoc! {"
        Lorem «ipsumˇ» dolor sit amet
    "});
    cx.dispatch_action(actions::editor::HandleInput("ips\num".to_string()));
    cx.assert_state(indoc! {"
        Lorem ips
        umˇ dolor sit amet
    "});
}

#[gpui::test]
fn test_backspace(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state(indoc! {"
        The quick brown fˇox
        jumps over the lazy dog\
    "});
    cx.dispatch_action(actions::editor::Backspace);
    cx.assert_state(indoc! {"
        The quick brown ˇox
        jumps over the lazy dog\
    "});

    cx.dispatch_action(actions::editor::MoveToBeginningOfLine {
        stop_at_soft_wraps: true,
        stop_at_indent: true,
    });
    cx.assert_state(indoc! {"
        ˇThe quick brown ox
        jumps over the lazy dog\
    "});

    cx.dispatch_action(actions::editor::Backspace);
    cx.assert_state(indoc! {"
        ˇThe quick brown ox
        jumps over the lazy dog\
    "});

    cx.dispatch_action(actions::editor::MoveDown);
    cx.assert_state(indoc! {"
        The quick brown ox
        ˇjumps over the lazy dog\
    "});

    cx.dispatch_action(actions::editor::Backspace);
    cx.assert_state(indoc! {"
        The quick brown oxˇjumps over the lazy dog\
    "});

    cx.dispatch_action(actions::editor::MoveToEndOfLine {
        stop_at_soft_wraps: true,
    });
    cx.assert_state(indoc! {"
        The quick brown oxjumps over the lazy dogˇ\
    "});

    cx.dispatch_action(actions::editor::Backspace);
    cx.assert_state(indoc! {"
        The quick brown oxjumps over the lazy doˇ\
    "});
}

#[gpui::test]
fn test_delete(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state(indoc! {"
        The quick brown fˇox
        jumps over the lazy dog\
    "});
    cx.dispatch_action(actions::editor::Delete);
    cx.assert_state(indoc! {"
        The quick brown fˇx
        jumps over the lazy dog\
    "});

    cx.dispatch_action(actions::editor::MoveToBeginningOfLine {
        stop_at_soft_wraps: true,
        stop_at_indent: true,
    });
    cx.assert_state(indoc! {"
        ˇThe quick brown fx
        jumps over the lazy dog\
    "});

    cx.dispatch_action(actions::editor::Delete);
    cx.assert_state(indoc! {"
        ˇhe quick brown fx
        jumps over the lazy dog\
    "});

    cx.dispatch_action(actions::editor::MoveToEndOfLine {
        stop_at_soft_wraps: true,
    });
    cx.assert_state(indoc! {"
        he quick brown fxˇ
        jumps over the lazy dog\
    "});

    cx.dispatch_action(actions::editor::Delete);
    cx.assert_state(indoc! {"
        he quick brown fxˇjumps over the lazy dog\
    "});

    cx.dispatch_action(actions::editor::MoveToEndOfLine {
        stop_at_soft_wraps: true,
    });
    cx.assert_state(indoc! {"
        he quick brown fxjumps over the lazy dogˇ\
    "});

    cx.dispatch_action(actions::editor::Delete);
    cx.assert_state(indoc! {"
        he quick brown fxjumps over the lazy dogˇ\
    "});

    cx.dispatch_action(actions::editor::MoveLeft);
    cx.assert_state(indoc! {"
        he quick brown fxjumps over the lazy doˇg\
    "});

    cx.dispatch_action(actions::editor::Delete);
    cx.assert_state(indoc! {"
        he quick brown fxjumps over the lazy doˇ\
    "});
}

#[gpui::test]
fn test_newline(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state(indoc! {"
        The quick brown foxˇjumps over the lazy dog\
    "});
    cx.dispatch_action(actions::editor::Newline);
    cx.assert_state(indoc! {"
        The quick brown fox
        ˇjumps over the lazy dog\
    "});

    cx.dispatch_action(actions::editor::MoveUp);
    cx.assert_state(indoc! {"
        ˇThe quick brown fox
        jumps over the lazy dog\
    "});

    cx.dispatch_action(actions::editor::Newline);
    cx.assert_state(indoc! {"

        ˇThe quick brown fox
        jumps over the lazy dog\
    "});

    cx.dispatch_action(actions::editor::MoveToEnd);
    cx.assert_state(indoc! {"

        The quick brown fox
        jumps over the lazy dogˇ\
    "});

    cx.dispatch_action(actions::editor::Newline);
    cx.assert_state(indoc! {"

        The quick brown fox
        jumps over the lazy dog
        ˇ\
    "});

    cx.set_state(indoc! {"
        The« quick ˇ»brown fox
        jumps over the lazy dog\
    "});
    cx.dispatch_action(actions::editor::Newline);
    cx.assert_state(indoc! {"
        The
        ˇbrown fox
        jumps over the lazy dog\
    "});
}

#[gpui::test]
fn test_select_all(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state(indoc! {"
        abc
        deˇ
        fgh\
    "});
    cx.dispatch_action(actions::editor::SelectAll);
    cx.assert_state(indoc! {"
        «abc
        de
        fghˇ»\
    "});
}

#[gpui::test]
fn test_select_all_does_not_autoscroll(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    let line_height = cx.update_editor(|editor, window, cx| {
        editor.set_vertical_scroll_margin(2, cx);
        editor
            .create_style(cx)
            .text
            .line_height_in_pixels(window.rem_size())
    });
    let window = cx.window;
    cx.simulate_window_resize(window, gpui::size(gpui::px(1000.0), 6.0 * line_height));

    cx.set_state(indoc! {"
        ˇone
        two
        three
        four
        five
        six
        seven
        eight
        nine
        ten
    "});

    for _ in 0..6 {
        cx.dispatch_action(actions::editor::MoveDown);
    }
    cx.run_until_parked();

    cx.assert_state(indoc! {"
        one
        two
        three
        four
        five
        six
        ˇseven
        eight
        nine
        ten
    "});

    let initial_scroll_position = cx.update_editor(|editor, window, cx| {
        let scroll_position = editor.snapshot(window, cx).scroll_position();
        assert_eq!(scroll_position, Point::new(0.0, 3.0));

        scroll_position
    });

    cx.dispatch_action(actions::editor::SelectAll);

    let scroll_position_after_select_all =
        cx.update_editor(|editor, window, cx| editor.snapshot(window, cx).scroll_position());
    assert_eq!(
        initial_scroll_position, scroll_position_after_select_all,
        "scroll position should not change after select all",
    );
}

#[gpui::test]
fn test_move_beginning_of_line_stops_at_indent(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state("•••The quick brown fox jumps over the lazy dogˇ");
    let move_to_beginning = actions::editor::MoveToBeginningOfLine {
        stop_at_soft_wraps: true,
        stop_at_indent: true,
    };

    cx.dispatch_action(move_to_beginning.clone());
    cx.assert_state("•••ˇThe quick brown fox jumps over the lazy dog");

    cx.dispatch_action(move_to_beginning);
    cx.assert_state("ˇ•••The quick brown fox jumps over the lazy dog");
}

#[gpui::test]
fn test_delete_beginning_of_line_stops_at_indent(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state("•••The quick brown fox jumps over the lazy dogˇ");
    cx.dispatch_action(actions::editor::DeleteToBeginningOfLine {
        stop_at_indent: true,
    });
    cx.assert_state("•••ˇ");
}

#[gpui::test]
fn test_beginning_of_line(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    let move_to_beginning_of_line = actions::editor::MoveToBeginningOfLine {
        stop_at_soft_wraps: true,
        stop_at_indent: true,
    };

    cx.set_state(indoc! {"
        The quick brown fox
        ••jumps over the lazy dˇog
    "});

    cx.dispatch_action(move_to_beginning_of_line.clone());
    cx.assert_state(indoc! {"
        The quick brown fox
        ••ˇjumps over the lazy dog
    "});

    cx.dispatch_action(move_to_beginning_of_line.clone());
    cx.assert_state(indoc! {"
        The quick brown fox
        ˇ••jumps over the lazy dog
    "});

    cx.dispatch_action(move_to_beginning_of_line.clone());
    cx.assert_state(indoc! {"
        The quick brown fox
        ••ˇjumps over the lazy dog
    "});

    cx.set_state(indoc! {"
        The quick brown fox
        ••jumps over the lazy dˇog
    "});
    cx.dispatch_action(actions::editor::SelectToBeginningOfLine {
        stop_at_soft_wraps: true,
        stop_at_indent: true,
    });
    cx.assert_state(indoc! {"
        The quick brown fox
        ••«ˇjumps over the lazy d»og
    "});

    cx.dispatch_action(actions::editor::SelectToBeginningOfLine {
        stop_at_soft_wraps: true,
        stop_at_indent: true,
    });
    cx.assert_state(indoc! {"
        The quick brown fox
        «ˇ••jumps over the lazy d»og
    "});

    cx.set_state(indoc! {"
        The quick brown fox
        ••jumps over the lazy dˇog
    "});
    cx.dispatch_action(actions::editor::DeleteToBeginningOfLine {
        stop_at_indent: false,
    });
    cx.assert_state(indoc! {"
        The quick brown fox
        ˇog
    "});
}

#[gpui::test]
fn test_end_of_line(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state(indoc! {"
        The quick brown fox
        ••jumps over the lazy dˇog
    "});

    cx.dispatch_action(actions::editor::MoveToEndOfLine {
        stop_at_soft_wraps: true,
    });
    cx.assert_state(indoc! {"
        The quick brown fox
        ••jumps over the lazy dogˇ
    "});

    cx.dispatch_action(actions::editor::MoveToEndOfLine {
        stop_at_soft_wraps: true,
    });
    cx.assert_state(indoc! {"
        The quick brown fox
        ••jumps over the lazy dogˇ
    "});

    cx.set_state(indoc! {"
        The quick brown fox
        ••jumps over the lazy dˇog
    "});
    cx.dispatch_action(actions::editor::SelectToEndOfLine {
        stop_at_soft_wraps: true,
    });
    cx.assert_state(indoc! {"
        The quick brown fox
        ••jumps over the lazy d«ogˇ»
    "});

    cx.set_state(indoc! {"
        The quick brown fox
        ••jumps over the lazy dˇog
    "});
    cx.dispatch_action(actions::editor::DeleteToEndOfLine);
    cx.assert_state(indoc! {"
        The quick brown fox
        ••jumps over the lazy dˇ
    "});
}

#[gpui::test]
fn test_beginning_of_line_with_cursor_between_line_start_and_indent(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    let move_to_beginning_of_line = actions::editor::MoveToBeginningOfLine {
        stop_at_soft_wraps: true,
        stop_at_indent: true,
    };

    cx.set_state(indoc! {"
        •••ˇ•hello
        world
    "});

    cx.dispatch_action(move_to_beginning_of_line.clone());
    cx.assert_state(indoc! {"
        ˇ••••hello
        world
    "});

    cx.dispatch_action(move_to_beginning_of_line.clone());
    cx.assert_state(indoc! {"
        ••••ˇhello
        world
    "});

    cx.dispatch_action(move_to_beginning_of_line);
    cx.assert_state(indoc! {"
        ˇ••••hello
        world
    "});
}

#[gpui::test]
fn test_prev_next_word_boundary(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state("one two.thˇree");

    cx.dispatch_action(actions::editor::MoveToPreviousWordStart);
    cx.assert_state("one two.ˇthree");

    cx.dispatch_action(actions::editor::MoveToPreviousWordStart);
    cx.assert_state("one ˇtwo.three");

    cx.dispatch_action(actions::editor::MoveToPreviousWordStart);
    cx.assert_state("ˇone two.three");

    cx.dispatch_action(actions::editor::MoveToPreviousWordStart);
    cx.assert_state("ˇone two.three");

    cx.dispatch_action(actions::editor::MoveToNextWordEnd);
    cx.assert_state("oneˇ two.three");

    cx.dispatch_action(actions::editor::MoveToNextWordEnd);
    cx.assert_state("one twoˇ.three");

    cx.dispatch_action(actions::editor::MoveToNextWordEnd);
    cx.assert_state("one two.threeˇ");

    cx.dispatch_action(actions::editor::MoveToNextWordEnd);
    cx.assert_state("one two.threeˇ");

    cx.dispatch_action(actions::editor::SelectToPreviousWordStart);
    cx.assert_state("one two.«ˇthree»");

    cx.dispatch_action(actions::editor::MoveLeft);
    cx.set_state("one two.ˇthree");

    cx.dispatch_action(actions::editor::SelectToNextWordEnd);
    cx.assert_state("one two.«threeˇ»");
}

#[gpui::test]
fn test_delete_to_word_boundary(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state("one two t«hreˇ»e four");
    cx.dispatch_action(actions::editor::DeleteToPreviousWordStart {
        ignore_newlines: false,
        ignore_brackets: false,
    });
    cx.assert_state("one two tˇe four");

    cx.set_state("one two te «fˇ»our");
    cx.dispatch_action(actions::editor::DeleteToNextWordEnd {
        ignore_newlines: false,
        ignore_brackets: false,
    });
    cx.assert_state("one two te ˇour");
}

#[gpui::test]
fn test_delete_to_previous_word_start_or_newline(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    let delete_to_previous_word_start = actions::editor::DeleteToPreviousWordStart {
        ignore_newlines: false,
        ignore_brackets: false,
    };
    let delete_to_previous_word_start_ignore_newlines =
        actions::editor::DeleteToPreviousWordStart {
            ignore_newlines: true,
            ignore_brackets: false,
        };

    cx.set_state(indoc! {"
        snake_case

        kebab-case

        camelCaseˇ
    "});

    cx.dispatch_action(delete_to_previous_word_start.clone());
    cx.assert_state(indoc! {"
        snake_case

        kebab-case

        ˇ
    "});

    cx.dispatch_action(delete_to_previous_word_start.clone());
    cx.assert_state(indoc! {"
        snake_case

        kebab-case
        ˇ
    "});

    cx.dispatch_action(delete_to_previous_word_start.clone());
    cx.assert_state(indoc! {"
        snake_case

        kebab-caseˇ
    "});

    cx.dispatch_action(delete_to_previous_word_start.clone());
    cx.assert_state(indoc! {"
        snake_case

        kebab-ˇ
    "});

    cx.dispatch_action(delete_to_previous_word_start.clone());
    cx.assert_state(indoc! {"
        snake_case

        kebabˇ
    "});

    cx.dispatch_action(delete_to_previous_word_start);
    cx.assert_state(indoc! {"
        snake_case

        ˇ
    "});

    cx.dispatch_action(delete_to_previous_word_start_ignore_newlines.clone());
    cx.assert_state(indoc! {"
        snake_case
        ˇ
    "});

    cx.dispatch_action(delete_to_previous_word_start_ignore_newlines);
    cx.assert_state(indoc! {"
        ˇ
    "});
}

#[gpui::test]
fn test_delete_to_previous_subword_start_or_newline(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    let delete_to_previous_subword_start = actions::editor::DeleteToPreviousSubwordStart {
        ignore_newlines: false,
        ignore_brackets: false,
    };
    let delete_to_previous_subword_start_ignore_newlines =
        actions::editor::DeleteToPreviousSubwordStart {
            ignore_newlines: true,
            ignore_brackets: false,
        };

    cx.set_state(indoc! {"
        snake_case

        kebab-case

        camelCaseˇ
    "});

    cx.dispatch_action(delete_to_previous_subword_start.clone());
    cx.assert_state(indoc! {"
        snake_case

        kebab-case

        camelˇ
    "});

    cx.dispatch_action(delete_to_previous_subword_start.clone());
    cx.assert_state(indoc! {"
        snake_case

        kebab-case

        ˇ
    "});

    cx.dispatch_action(delete_to_previous_subword_start.clone());
    cx.assert_state(indoc! {"
        snake_case

        kebab-case
        ˇ
    "});

    cx.dispatch_action(delete_to_previous_subword_start.clone());
    cx.assert_state(indoc! {"
        snake_case

        kebab-caseˇ
    "});

    cx.dispatch_action(delete_to_previous_subword_start.clone());
    cx.assert_state(indoc! {"
        snake_case

        kebab-ˇ
    "});

    cx.dispatch_action(delete_to_previous_subword_start.clone());
    cx.assert_state(indoc! {"
        snake_case

        kebabˇ
    "});

    cx.dispatch_action(delete_to_previous_subword_start);
    cx.assert_state(indoc! {"
        snake_case

        ˇ
    "});

    cx.dispatch_action(delete_to_previous_subword_start_ignore_newlines.clone());
    cx.assert_state(indoc! {"
        snake_case
        ˇ
    "});

    cx.dispatch_action(delete_to_previous_subword_start_ignore_newlines.clone());
    cx.assert_state(indoc! {"
        snake_ˇ
    "});

    cx.dispatch_action(delete_to_previous_subword_start_ignore_newlines);
    cx.assert_state(indoc! {"
        ˇ
    "});
}

#[gpui::test]
fn test_delete_to_next_word_end_or_newline(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    let delete_to_next_word_end = actions::editor::DeleteToNextWordEnd {
        ignore_newlines: false,
        ignore_brackets: false,
    };
    let delete_to_next_word_end_ignore_newlines = actions::editor::DeleteToNextWordEnd {
        ignore_newlines: true,
        ignore_brackets: false,
    };

    cx.set_state(indoc! {"
        ˇsnake_case

        kebab-case

        camelCase
    "});

    cx.dispatch_action(delete_to_next_word_end.clone());
    cx.assert_state(indoc! {"
        ˇ

        kebab-case

        camelCase
    "});

    cx.dispatch_action(delete_to_next_word_end.clone());
    cx.assert_state(indoc! {"
        ˇ
        kebab-case

        camelCase
    "});

    cx.dispatch_action(delete_to_next_word_end.clone());
    cx.assert_state(indoc! {"
        ˇkebab-case

        camelCase
    "});

    cx.dispatch_action(delete_to_next_word_end.clone());
    cx.assert_state(indoc! {"
        ˇ-case

        camelCase
    "});

    cx.dispatch_action(delete_to_next_word_end.clone());
    cx.assert_state(indoc! {"
        ˇcase

        camelCase
    "});

    cx.dispatch_action(delete_to_next_word_end);
    cx.assert_state(indoc! {"
        ˇ

        camelCase
    "});

    cx.dispatch_action(delete_to_next_word_end_ignore_newlines.clone());
    cx.assert_state(indoc! {"
        ˇ
        camelCase
    "});

    cx.dispatch_action(delete_to_next_word_end_ignore_newlines);
    cx.assert_state(indoc! {"
        ˇ
    "});
}

#[gpui::test]
fn test_delete_to_next_subword_end_or_newline(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    let delete_to_next_subword_end = actions::editor::DeleteToNextSubwordEnd {
        ignore_newlines: false,
        ignore_brackets: false,
    };
    let delete_to_next_subword_end_ignore_newlines = actions::editor::DeleteToNextSubwordEnd {
        ignore_newlines: true,
        ignore_brackets: false,
    };

    cx.set_state(indoc! {"
        ˇsnake_case

        kebab-case

        camelCase
    "});

    cx.dispatch_action(delete_to_next_subword_end.clone());
    cx.assert_state(indoc! {"
        ˇ_case

        kebab-case

        camelCase
    "});

    cx.dispatch_action(delete_to_next_subword_end.clone());
    cx.assert_state(indoc! {"
        ˇ

        kebab-case

        camelCase
    "});

    cx.dispatch_action(delete_to_next_subword_end.clone());
    cx.assert_state(indoc! {"
        ˇ
        kebab-case

        camelCase
    "});

    cx.dispatch_action(delete_to_next_subword_end.clone());
    cx.assert_state(indoc! {"
        ˇkebab-case

        camelCase
    "});

    cx.dispatch_action(delete_to_next_subword_end.clone());
    cx.assert_state(indoc! {"
        ˇ-case

        camelCase
    "});

    cx.dispatch_action(delete_to_next_subword_end.clone());
    cx.assert_state(indoc! {"
        ˇcase

        camelCase
    "});

    cx.dispatch_action(delete_to_next_subword_end);
    cx.assert_state(indoc! {"
        ˇ

        camelCase
    "});

    cx.dispatch_action(delete_to_next_subword_end_ignore_newlines.clone());
    cx.assert_state(indoc! {"
        ˇ
        camelCase
    "});

    cx.dispatch_action(delete_to_next_subword_end_ignore_newlines.clone());
    cx.assert_state(indoc! {"
        ˇCase
    "});

    cx.dispatch_action(delete_to_next_subword_end_ignore_newlines);
    cx.assert_state(indoc! {"
        ˇ
    "});
}

#[gpui::test]
fn test_undo_redo_restores_cursor(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state("Hello, woˇrld!");
    cx.dispatch_action(actions::editor::HandleInput("d".to_string()));
    cx.assert_state("Hello, wodˇrld!");

    cx.dispatch_action(actions::editor::Undo);
    cx.assert_state("Hello, woˇrld!");

    cx.dispatch_action(actions::editor::Redo);
    cx.assert_state("Hello, wodˇrld!");
}

#[gpui::test]
fn test_undo_redo_restores_selection(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state("Hello, «worldˇ»!");
    cx.dispatch_action(actions::editor::HandleInput("from Zaku".to_string()));
    cx.assert_state("Hello, from Zakuˇ!");

    cx.dispatch_action(actions::editor::MoveToPreviousWordStart);
    cx.dispatch_action(actions::editor::SelectToNextWordEnd);
    cx.assert_state("Hello, from «Zakuˇ»!");

    cx.dispatch_action(actions::editor::HandleInput("another planet".to_string()));
    cx.assert_state("Hello, from another planetˇ!");

    cx.dispatch_action(actions::editor::Undo);
    cx.assert_state("Hello, from «Zakuˇ»!");

    cx.dispatch_action(actions::editor::Undo);
    cx.assert_state("Hello, «worldˇ»!");

    cx.dispatch_action(actions::editor::Redo);
    cx.assert_state("Hello, from Zakuˇ!");

    cx.dispatch_action(actions::editor::Redo);
    cx.assert_state("Hello, from another planetˇ!");
}

#[gpui::test]
fn test_undo_redo_selection(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state("Hello, woˇrld!");
    cx.dispatch_action(actions::editor::MoveRight);
    cx.assert_state("Hello, worˇld!");

    cx.dispatch_action(actions::editor::MoveLeft);
    cx.assert_state("Hello, woˇrld!");

    cx.dispatch_action(actions::editor::UndoSelection);
    cx.assert_state("Hello, worˇld!");

    cx.dispatch_action(actions::editor::RedoSelection);
    cx.assert_state("Hello, woˇrld!");
}

#[gpui::test]
fn test_selection_with_mouse(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state(indoc! {"
        The
        quick
        brˇown
        fox
    "});

    cx.update_editor(|editor, _, cx| {
        editor.begin_selection(DisplayPoint::new(DisplayRow(2), 2), 1, cx);
    });
    cx.update_editor(|editor, _, cx| {
        assert_eq!(
            display_ranges(editor, cx),
            [DisplayPoint::new(DisplayRow(2), 2)..DisplayPoint::new(DisplayRow(2), 2)]
        );
    });

    cx.update_editor(|editor, _, cx| {
        editor.update_selection(DisplayPoint::new(DisplayRow(3), 3), cx);
    });
    cx.update_editor(|editor, _, cx| {
        assert_eq!(
            display_ranges(editor, cx),
            [DisplayPoint::new(DisplayRow(2), 2)..DisplayPoint::new(DisplayRow(3), 3)]
        );
    });

    cx.update_editor(|editor, _, cx| {
        editor.update_selection(DisplayPoint::new(DisplayRow(1), 1), cx);
    });
    cx.update_editor(|editor, _, cx| {
        assert_eq!(
            display_ranges(editor, cx),
            [DisplayPoint::new(DisplayRow(2), 2)..DisplayPoint::new(DisplayRow(1), 1)]
        );
    });

    cx.update_editor(|editor, _, cx| {
        editor.end_selection(cx);
        editor.update_selection(DisplayPoint::new(DisplayRow(3), 3), cx);
    });
    cx.update_editor(|editor, _, cx| {
        assert_eq!(
            display_ranges(editor, cx),
            [DisplayPoint::new(DisplayRow(2), 2)..DisplayPoint::new(DisplayRow(1), 1)]
        );
    });

    cx.update_editor(|editor, _, cx| {
        editor.begin_selection(DisplayPoint::new(DisplayRow(3), 3), 1, cx);
        editor.update_selection(DisplayPoint::new(DisplayRow(0), 0), cx);
    });
    cx.update_editor(|editor, _, cx| {
        assert_eq!(
            display_ranges(editor, cx),
            [DisplayPoint::new(DisplayRow(3), 3)..DisplayPoint::new(DisplayRow(0), 0)]
        );
    });

    cx.update_editor(|editor, _, cx| {
        editor.end_selection(cx);
    });
    cx.update_editor(|editor, _, cx| {
        assert_eq!(
            display_ranges(editor, cx),
            [DisplayPoint::new(DisplayRow(3), 3)..DisplayPoint::new(DisplayRow(0), 0)]
        );
    });
}

#[gpui::test]
fn test_ime_composition(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state("abcdeˇ");
    cx.update_editor(|editor, window, cx| {
        editor.replace_and_mark_text_in_range(Some(0..1), "à", None, window, cx);
        editor.replace_and_mark_text_in_range(Some(0..1), "á", None, window, cx);
        editor.replace_and_mark_text_in_range(Some(0..1), "ä", None, window, cx);
        assert_eq!(editor.buffer_snapshot(cx).text(), "äbcde");
        assert_eq!(editor.marked_text_range(window, cx), Some(0..1));

        editor.replace_text_in_range(None, "ā", window, cx);
        assert_eq!(editor.buffer_snapshot(cx).text(), "ābcde");
        assert_eq!(editor.marked_text_range(window, cx), None);

        editor.undo(&actions::editor::Undo, window, cx);
        assert_eq!(editor.buffer_snapshot(cx).text(), "abcde");
        assert_eq!(editor.marked_text_range(window, cx), None);

        editor.redo(&actions::editor::Redo, window, cx);
        assert_eq!(editor.buffer_snapshot(cx).text(), "ābcde");
        assert_eq!(editor.marked_text_range(window, cx), None);

        editor.replace_and_mark_text_in_range(Some(0..1), "à", None, window, cx);
        assert_eq!(editor.buffer_snapshot(cx).text(), "àbcde");
        assert_eq!(editor.marked_text_range(window, cx), Some(0..1));

        editor.undo(&actions::editor::Undo, window, cx);
        assert_eq!(editor.buffer_snapshot(cx).text(), "ābcde");
        assert_eq!(editor.marked_text_range(window, cx), None);

        editor.replace_and_mark_text_in_range(Some(4..999), "è", None, window, cx);
        assert_eq!(editor.buffer_snapshot(cx).text(), "ābcdè");
        assert_eq!(editor.marked_text_range(window, cx), Some(4..5));

        editor.replace_text_in_range(Some(4..999), "ę", window, cx);
        assert_eq!(editor.buffer_snapshot(cx).text(), "ābcdę");
        assert_eq!(editor.marked_text_range(window, cx), None);

        editor.replace_and_mark_text_in_range(Some(0..1), "XYZ", None, window, cx);
        assert_eq!(editor.buffer_snapshot(cx).text(), "XYZbcdę");
        assert_eq!(editor.marked_text_range(window, cx), Some(0..3));

        editor.replace_and_mark_text_in_range(Some(1..2), "1", None, window, cx);
        assert_eq!(editor.buffer_snapshot(cx).text(), "X1Zbcdę");
        assert_eq!(editor.marked_text_range(window, cx), Some(1..2));

        editor.replace_text_in_range(Some(1..2), "2", window, cx);
        assert_eq!(editor.buffer_snapshot(cx).text(), "X2Zbcdę");
        assert_eq!(editor.marked_text_range(window, cx), None);
    });
}

#[gpui::test]
fn test_insert_with_old_selections(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state("a( «Xˇ» ), b( Y ), c( Z )");
    cx.update_editor(|editor, _, cx| {
        editor.buffer.update(cx, |buffer, cx| {
            buffer.edit(
                [
                    (MultiBufferOffset(2)..MultiBufferOffset(5), ""),
                    (MultiBufferOffset(10)..MultiBufferOffset(13), ""),
                    (MultiBufferOffset(18)..MultiBufferOffset(21), ""),
                ],
                cx,
            );
        });
        assert_eq!(editor.buffer_snapshot(cx).text(), "a(), b(), c()");
        assert_eq!(editor.selected_range(cx), 2..2);
    });

    cx.assert_state("a(ˇ), b(), c()");
    cx.dispatch_action(actions::editor::HandleInput("Z".to_string()));
    cx.assert_state("a(Zˇ), b(), c()");
}

#[gpui::test]
fn test_vertical_autoscroll(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    let line_height = cx.update_editor(|editor, window, cx| {
        editor.set_vertical_scroll_margin(2, cx);
        editor
            .create_style(cx)
            .text
            .line_height_in_pixels(window.rem_size())
    });
    let window = cx.window;
    cx.simulate_window_resize(window, gpui::size(gpui::px(1000.0), 6.0 * line_height));

    cx.set_state(indoc! {"
        ˇone
        two
        three
        four
        five
        six
        seven
        eight
        nine
        ten
    "});

    cx.update_editor(|editor, window, cx| {
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            Point::new(0.0, 0.0)
        );
    });

    for _ in 0..6 {
        cx.dispatch_action(actions::editor::MoveDown);
    }
    cx.run_until_parked();

    cx.assert_state(indoc! {"
        one
        two
        three
        four
        five
        six
        ˇseven
        eight
        nine
        ten
    "});
    cx.update_editor(|editor, window, cx| {
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            Point::new(0.0, 3.0)
        );
    });

    for _ in 0..3 {
        cx.dispatch_action(actions::editor::MoveDown);
    }
    cx.run_until_parked();

    cx.assert_state(indoc! {"
        one
        two
        three
        four
        five
        six
        seven
        eight
        nine
        ˇten
    "});
    cx.update_editor(|editor, window, cx| {
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            Point::new(0.0, 6.0)
        );
    });

    for _ in 0..6 {
        cx.dispatch_action(actions::editor::MoveUp);
    }
    cx.run_until_parked();

    cx.assert_state(indoc! {"
        one
        two
        three
        ˇfour
        five
        six
        seven
        eight
        nine
        ten
    "});
    cx.update_editor(|editor, window, cx| {
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            Point::new(0.0, 1.0)
        );
    });
}

#[gpui::test]
fn test_vertical_autoscroll_on_undo_redo(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    let line_height = cx.update_editor(|editor, window, cx| {
        editor.set_vertical_scroll_margin(2, cx);
        editor
            .create_style(cx)
            .text
            .line_height_in_pixels(window.rem_size())
    });
    let window = cx.window;
    cx.simulate_window_resize(window, gpui::size(gpui::px(1000.0), 6.0 * line_height));

    cx.set_state(indoc! {"
        one
        two
        three
        fourˇ
        five
        six
        seven
        eight
        nine
        ten
    "});

    cx.dispatch_action(actions::editor::HandleInput("5".to_string()));
    cx.assert_state(indoc! {"
        one
        two
        three
        four5ˇ
        five
        six
        seven
        eight
        nine
        ten
    "});

    for _ in 0..6 {
        cx.dispatch_action(actions::editor::MoveDown);
    }
    cx.run_until_parked();
    cx.assert_state(indoc! {"
        one
        two
        three
        four5
        five
        six
        seven
        eight
        nine
        tenˇ
    "});
    cx.update_editor(|editor, window, cx| {
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            Point::new(0.0, 6.0)
        );
    });

    cx.dispatch_action(actions::editor::Undo);
    cx.run_until_parked();
    cx.assert_state(indoc! {"
        one
        two
        three
        fourˇ
        five
        six
        seven
        eight
        nine
        ten
    "});
    cx.update_editor(|editor, window, cx| {
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            Point::new(0.0, 1.0)
        );
    });

    for _ in 0..4 {
        cx.dispatch_action(actions::editor::MoveDown);
    }
    cx.run_until_parked();
    cx.assert_state(indoc! {"
        one
        two
        three
        four
        five
        six
        seven
        eighˇt
        nine
        ten
    "});
    cx.update_editor(|editor, window, cx| {
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            Point::new(0.0, 4.0)
        );
    });

    cx.dispatch_action(actions::editor::Redo);
    cx.run_until_parked();
    cx.assert_state(indoc! {"
        one
        two
        three
        four5ˇ
        five
        six
        seven
        eight
        nine
        ten
    "});
    cx.update_editor(|editor, window, cx| {
        assert_eq!(
            editor.snapshot(window, cx).scroll_position(),
            Point::new(0.0, 1.0)
        );
    });
}

#[gpui::test]
fn test_copy_cut_paste_actions(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state("Hello, «worldˇ»!");
    cx.dispatch_action(actions::editor::Copy);
    let clipboard_text = cx
        .cx
        .read_from_clipboard()
        .and_then(|item: ClipboardItem| item.text());
    assert_eq!(clipboard_text.as_deref(), Some("world"));

    cx.dispatch_action(actions::editor::Cut);
    cx.assert_state("Hello, ˇ!");
    let clipboard_text = cx
        .cx
        .read_from_clipboard()
        .and_then(|item: ClipboardItem| item.text());
    assert_eq!(clipboard_text.as_deref(), Some("world"));

    cx.cx
        .write_to_clipboard(ClipboardItem::new_string("hello world".to_string()));
    cx.dispatch_action(actions::editor::Paste);
    cx.assert_state("Hello, hello worldˇ!");
}

#[gpui::test]
fn test_single_line_editor_paste_strips_newlines(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new_single_line(cx);

    cx.set_state("ˇ");
    cx.cx.write_to_clipboard(ClipboardItem::new_string(
        "The quick\r\nbrown fox jumps over\nthe lazy dog\r".to_string(),
    ));
    cx.dispatch_action(actions::editor::Paste);
    cx.assert_state("The quickbrown fox jumps overthe lazy dogˇ");
}

#[gpui::test]
fn test_single_line_editor_replace_text_in_range_strips_newlines(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new_single_line(cx);

    cx.set_state("Lorem «ipsumˇ»");
    cx.dispatch_action(actions::editor::HandleInput(
        "ipsum\r\ndolor sit\namet".to_string(),
    ));
    cx.assert_state("Lorem ipsumdolor sitametˇ");
}

#[gpui::test]
fn test_move_cursor(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state(indoc! {"
        ˇaaaaaa
        \t\taaaaaa
        aaaaaa\
    "});

    cx.dispatch_action(actions::editor::MoveDown);
    cx.assert_state(indoc! {"
        aaaaaa
        ˇ\t\taaaaaa
        aaaaaa\
    "});

    cx.dispatch_action(actions::editor::MoveRight);
    cx.assert_state(indoc! {"
        aaaaaa
        \tˇ\taaaaaa
        aaaaaa\
    "});

    cx.dispatch_action(actions::editor::MoveLeft);
    cx.assert_state(indoc! {"
        aaaaaa
        ˇ\t\taaaaaa
        aaaaaa\
    "});

    cx.dispatch_action(actions::editor::MoveUp);
    cx.assert_state(indoc! {"
        ˇaaaaaa
        \t\taaaaaa
        aaaaaa\
    "});

    cx.dispatch_action(actions::editor::MoveToEnd);
    cx.assert_state(indoc! {"
        aaaaaa
        \t\taaaaaa
        aaaaaaˇ\
    "});

    cx.dispatch_action(actions::editor::MoveToBeginning);
    cx.assert_state(indoc! {"
        ˇaaaaaa
        \t\taaaaaa
        aaaaaa\
    "});

    cx.set_state("a«bˇ»cd");
    cx.dispatch_action(actions::editor::SelectToBeginning);
    cx.assert_state("«ˇa»bcd");

    cx.dispatch_action(actions::editor::SelectToEnd);
    cx.assert_state("a«bcdˇ»");
}

#[gpui::test]
fn test_move_cursor_multibyte(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state(indoc! {"
        ˇ🌑🌒🌓🌔🌕🌖
        abcde
        абвгд
    "});

    cx.dispatch_action(actions::editor::MoveRight);
    cx.assert_state(indoc! {"
        🌑ˇ🌒🌓🌔🌕🌖
        abcde
        абвгд
    "});

    cx.dispatch_action(actions::editor::MoveRight);
    cx.assert_state(indoc! {"
        🌑🌒ˇ🌓🌔🌕🌖
        abcde
        абвгд
    "});

    cx.dispatch_action(actions::editor::MoveRight);
    cx.assert_state(indoc! {"
        🌑🌒🌓ˇ🌔🌕🌖
        abcde
        абвгд
    "});

    cx.dispatch_action(actions::editor::MoveDown);
    cx.assert_state(indoc! {"
        🌑🌒🌓🌔🌕🌖
        abcdeˇ
        абвгд
    "});

    cx.dispatch_action(actions::editor::MoveDown);
    cx.assert_state(indoc! {"
        🌑🌒🌓🌔🌕🌖
        abcde
        абвгдˇ
    "});

    cx.dispatch_action(actions::editor::MoveLeft);
    cx.dispatch_action(actions::editor::MoveLeft);
    cx.assert_state(indoc! {"
        🌑🌒🌓🌔🌕🌖
        abcde
        абвˇгд
    "});

    cx.dispatch_action(actions::editor::MoveUp);
    cx.assert_state(indoc! {"
        🌑🌒🌓🌔🌕🌖
        abcˇde
        абвгд
    "});

    cx.dispatch_action(actions::editor::MoveUp);
    cx.assert_state(indoc! {"
        🌑ˇ🌒🌓🌔🌕🌖
        abcde
        абвгд
    "});
}

#[gpui::test]
fn test_move_cursor_different_line_lengths(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state(indoc! {"
        ⓐⓑⓒⓓⓔˇ
        abcd
        αβγ
        abcd
        ⓐⓑⓒⓓⓔ\
    "});

    cx.dispatch_action(actions::editor::MoveDown);
    cx.assert_state(indoc! {"
        ⓐⓑⓒⓓⓔ
        abcdˇ
        αβγ
        abcd
        ⓐⓑⓒⓓⓔ\
    "});

    cx.dispatch_action(actions::editor::MoveDown);
    cx.assert_state(indoc! {"
        ⓐⓑⓒⓓⓔ
        abcd
        αβγˇ
        abcd
        ⓐⓑⓒⓓⓔ\
    "});

    cx.dispatch_action(actions::editor::MoveDown);
    cx.assert_state(indoc! {"
        ⓐⓑⓒⓓⓔ
        abcd
        αβγ
        abcdˇ
        ⓐⓑⓒⓓⓔ\
    "});

    cx.dispatch_action(actions::editor::MoveDown);
    cx.assert_state(indoc! {"
        ⓐⓑⓒⓓⓔ
        abcd
        αβγ
        abcd
        ⓐⓑⓒⓓⓔˇ\
    "});

    cx.dispatch_action(actions::editor::MoveDown);
    cx.assert_state(indoc! {"
        ⓐⓑⓒⓓⓔ
        abcd
        αβγ
        abcd
        ⓐⓑⓒⓓⓔˇ\
    "});

    cx.dispatch_action(actions::editor::MoveUp);
    cx.assert_state(indoc! {"
        ⓐⓑⓒⓓⓔ
        abcd
        αβγ
        abcdˇ
        ⓐⓑⓒⓓⓔ\
    "});

    cx.dispatch_action(actions::editor::MoveUp);
    cx.assert_state(indoc! {"
        ⓐⓑⓒⓓⓔ
        abcd
        αβγˇ
        abcd
        ⓐⓑⓒⓓⓔ\
    "});
}
