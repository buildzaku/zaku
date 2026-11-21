use gpui::{
    App, BorrowAppContext, ClipboardItem, Context, Entity, EntityInputHandler, Modifiers, Point,
    ScrollDelta, ScrollEvent, SharedString, TestAppContext, Window, prelude::*,
};
use indoc::indoc;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::{
    ops::Range,
    rc::Rc,
    sync::{
        Arc,
        atomic::{self, AtomicUsize},
    },
    time::Duration,
};

use language::Capability;
use multi_buffer::MultiBufferOffset;
use project::{Variable, project_config_store::VariableScope};
use settings::{Settings, SettingsStore};

use editor::{
    Editor, EditorSettings, VariableProvider, VariableProviderDelegate,
    code_context_menus::CompletionsMenu,
    display_map::{DisplayPoint, DisplayRow},
    test::editor_test_context::EditorTestContext,
};

fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test_new(cx);
        cx.set_global(settings_store);
        theme::init(theme::LoadThemes::JustBase, cx);
        editor::init(cx);
    });
}

fn get_hover_popover_delay(cx: &TestAppContext) -> u64 {
    cx.read(|cx: &App| -> u64 { EditorSettings::get_global(cx).hover_popover.delay.0 })
}

struct TestVariableDelegate {
    variables: Vec<(VariableScope, Variable)>,
    request_count: Arc<AtomicUsize>,
}

impl VariableProviderDelegate for TestVariableDelegate {
    fn variables(&self, _: &App) -> Option<Vec<(VariableScope, Variable)>> {
        self.request_count.fetch_add(1, atomic::Ordering::Release);
        Some(self.variables.clone())
    }
}

fn handle_hover_requests(cx: &mut EditorTestContext) -> Arc<AtomicUsize> {
    let request_count = Arc::new(AtomicUsize::new(0));
    cx.update_editor(|editor, _, _| {
        editor.set_semantics_provider(Some(Rc::new(VariableProvider::new(TestVariableDelegate {
            variables: vec![(
                VariableScope::Project,
                Variable {
                    name: "base_url".to_string(),
                    value: "https://api.zaku.dev".to_string(),
                    disabled: false,
                },
            )],
            request_count: request_count.clone(),
        }))));
    });
    request_count
}

fn handle_completion_requests(cx: &mut EditorTestContext) -> Arc<AtomicUsize> {
    let request_count = Arc::new(AtomicUsize::new(0));
    cx.update_editor(|editor, _, _| {
        editor.set_completion_provider(Some(Rc::new(VariableProvider::new(
            TestVariableDelegate {
                variables: vec![
                    (
                        VariableScope::Project,
                        Variable {
                            name: "base_url".to_string(),
                            value: "https://api.zaku.dev".to_string(),
                            disabled: false,
                        },
                    ),
                    (
                        VariableScope::Project,
                        Variable {
                            name: "token".to_string(),
                            value: "test-token".to_string(),
                            disabled: false,
                        },
                    ),
                ],
                request_count: request_count.clone(),
            },
        ))));
    });
    request_count
}

fn completion_menu_entries(menu: &CompletionsMenu) -> Vec<String> {
    let entries = menu.entries.borrow();
    entries
        .iter()
        .map(|string_match| string_match.string.clone())
        .collect()
}

struct TestScrollContainer {
    editor: Entity<Editor>,
}

impl Render for TestScrollContainer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui::div()
            .id("scroll-container")
            .size_full()
            .overflow_y_scroll()
            .child(self.editor.clone())
            .child(gpui::div().h(gpui::px(2000.0)))
    }
}

struct TestRequestView {
    url: Entity<Editor>,
    body: Entity<Editor>,
}

impl Render for TestRequestView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui::div()
            .size_full()
            .flex()
            .flex_col()
            .child(self.url.clone())
            .child(gpui::div().flex_1().min_h_0().child(self.body.clone()))
    }
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
    cx.simulate_keystroke("d");
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
fn test_mouse_selection(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state(indoc! {"
        The
        quick
        brˇown
        fox
    "});

    cx.update_editor(|editor, window, cx| {
        editor.begin_selection(DisplayPoint::new(DisplayRow(2), 2), 1, window, cx);
    });
    cx.update_editor(|editor, _, cx| {
        assert_eq!(
            display_ranges(editor, cx),
            [DisplayPoint::new(DisplayRow(2), 2)..DisplayPoint::new(DisplayRow(2), 2)]
        );
    });

    cx.update_editor(|editor, window, cx| {
        editor.update_selection(DisplayPoint::new(DisplayRow(3), 3), window, cx);
    });
    cx.update_editor(|editor, _, cx| {
        assert_eq!(
            display_ranges(editor, cx),
            [DisplayPoint::new(DisplayRow(2), 2)..DisplayPoint::new(DisplayRow(3), 3)]
        );
    });

    cx.update_editor(|editor, window, cx| {
        editor.update_selection(DisplayPoint::new(DisplayRow(1), 1), window, cx);
    });
    cx.update_editor(|editor, _, cx| {
        assert_eq!(
            display_ranges(editor, cx),
            [DisplayPoint::new(DisplayRow(2), 2)..DisplayPoint::new(DisplayRow(1), 1)]
        );
    });

    cx.update_editor(|editor, window, cx| {
        editor.end_selection(window, cx);
        editor.update_selection(DisplayPoint::new(DisplayRow(3), 3), window, cx);
    });
    cx.update_editor(|editor, _, cx| {
        assert_eq!(
            display_ranges(editor, cx),
            [DisplayPoint::new(DisplayRow(2), 2)..DisplayPoint::new(DisplayRow(1), 1)]
        );
    });

    cx.update_editor(|editor, window, cx| {
        editor.begin_selection(DisplayPoint::new(DisplayRow(3), 3), 1, window, cx);
        editor.update_selection(DisplayPoint::new(DisplayRow(0), 0), window, cx);
    });
    cx.update_editor(|editor, _, cx| {
        assert_eq!(
            display_ranges(editor, cx),
            [DisplayPoint::new(DisplayRow(3), 3)..DisplayPoint::new(DisplayRow(0), 0)]
        );
    });

    cx.update_editor(|editor, window, cx| {
        editor.end_selection(window, cx);
    });
    cx.update_editor(|editor, _, cx| {
        assert_eq!(
            display_ranges(editor, cx),
            [DisplayPoint::new(DisplayRow(3), 3)..DisplayPoint::new(DisplayRow(0), 0)]
        );
    });
}

#[gpui::test]
fn test_mouse_selection_beyond_end_of_buffer(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state(indoc! {"
        The
        quick
        brˇown
        fox\
    "});

    cx.update_editor(|editor, window, cx| {
        editor.begin_selection(DisplayPoint::new(DisplayRow(0), 1), 1, window, cx);
        editor.update_selection(DisplayPoint::new(DisplayRow(4), 0), window, cx);
    });
    cx.update_editor(|editor, _, cx| {
        assert_eq!(
            display_ranges(editor, cx),
            [DisplayPoint::new(DisplayRow(0), 1)..DisplayPoint::new(DisplayRow(3), 3)]
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
    cx.simulate_keystroke("Z");
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

    cx.simulate_keystroke("5");
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
fn test_paste_normalizes_line_endings(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state("foo\nˇqux");
    cx.cx
        .write_to_clipboard(ClipboardItem::new_string("bar\r\nbaz\r\n".to_string()));
    cx.dispatch_action(actions::editor::Paste);
    cx.assert_state("foo\nbar\nbaz\nˇqux");
}

#[gpui::test]
fn test_set_text_normalizes_line_endings(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.update_editor(|editor, window, cx| editor.set_text("foo\r\nbar\r\nbaz", window, cx));
    cx.assert_state("foo\nbar\nbazˇ");
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
        🌑🌒ˇ🌓🌔🌕🌖
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

#[gpui::test]
fn test_mouse_hover_info_popover(cx: &mut TestAppContext) {
    init_test(cx);

    let mut cx = EditorTestContext::new(cx);

    cx.set_state("ˇ{{base_url}}/users");
    let request_count = handle_hover_requests(&mut cx);
    let hover_delay = Duration::from_millis(get_hover_popover_delay(&cx) + 100);
    let hover_point = cx.pixel_position("{{baseˇ_url}}/users");
    cx.simulate_mouse_move(hover_point, None, Modifiers::default());
    cx.update_editor(|editor, _, _| {
        assert!(editor.hover_state.info_popovers.is_empty());
    });

    cx.background_executor.advance_clock(hover_delay);
    assert_eq!(request_count.load(atomic::Ordering::Acquire), 1);
    cx.update_editor(|editor, _, _| {
        assert_eq!(editor.hover_state.info_popovers.len(), 1);
        assert_eq!(
            editor.hover_state.info_popovers[0].hover.variable,
            Some((
                VariableScope::Project,
                SharedString::from("https://api.zaku.dev")
            ))
        );
    });

    let hover_point = cx.pixel_position("{{base_url}}/ˇusers");
    cx.simulate_mouse_move(hover_point, None, Modifiers::default());
    cx.update_editor(|editor, _, _| {
        assert!(editor.hover_state.info_popovers.is_empty());
    });
    cx.background_executor.advance_clock(hover_delay);
    assert_eq!(request_count.load(atomic::Ordering::Acquire), 1);
    cx.update_editor(|editor, _, _| {
        assert!(editor.hover_state.info_popovers.is_empty());
    });

    let hover_point = cx.pixel_position("ˇ{{base_url}}/users");
    cx.simulate_mouse_move(hover_point, None, Modifiers::default());
    cx.background_executor.advance_clock(hover_delay);
    assert_eq!(request_count.load(atomic::Ordering::Acquire), 2);
    cx.update_editor(|editor, _, _| {
        assert_eq!(editor.hover_state.info_popovers.len(), 1);
        assert_eq!(
            editor.hover_state.info_popovers[0].hover.variable,
            Some((
                VariableScope::Project,
                SharedString::from("https://api.zaku.dev")
            ))
        );
    });

    let hover_point = cx.pixel_position("{{base_url}}ˇ/users");
    cx.simulate_mouse_move(hover_point, None, Modifiers::default());
    cx.update_editor(|editor, _, _| {
        assert!(editor.hover_state.info_popovers.is_empty());
    });
    cx.background_executor.advance_clock(hover_delay);
    assert_eq!(request_count.load(atomic::Ordering::Acquire), 2);
    cx.update_editor(|editor, _, _| {
        assert!(editor.hover_state.info_popovers.is_empty());
    });

    let hover_point = cx.pixel_position("{{base_url}ˇ}/users");
    cx.simulate_mouse_move(hover_point, None, Modifiers::default());
    cx.background_executor.advance_clock(hover_delay);
    assert_eq!(request_count.load(atomic::Ordering::Acquire), 3);
    cx.update_editor(|editor, _, _| {
        assert_eq!(editor.hover_state.info_popovers.len(), 1);
        assert_eq!(
            editor.hover_state.info_popovers[0].hover.variable,
            Some((
                VariableScope::Project,
                SharedString::from("https://api.zaku.dev")
            ))
        );
    });
}

#[gpui::test]
fn test_mouse_hover_cancelled_before_delay(cx: &mut TestAppContext) {
    init_test(cx);

    let mut cx = EditorTestContext::new(cx);

    cx.set_state("ˇ{{base_url}}/users");
    let request_count = handle_hover_requests(&mut cx);
    let hover_delay = Duration::from_millis(get_hover_popover_delay(&cx) + 100);
    let hover_point = cx.pixel_position("{{baseˇ_url}}/users");
    cx.simulate_mouse_move(hover_point, None, Modifiers::default());
    let hover_point = cx.gutter_position();
    cx.simulate_mouse_move(hover_point, None, Modifiers::default());

    cx.background_executor.advance_clock(hover_delay);
    assert_eq!(request_count.load(atomic::Ordering::Acquire), 0);
    cx.update_editor(|editor, _, _| {
        assert!(editor.hover_state.info_popovers.is_empty());
    });

    let hover_point = cx.pixel_position("{{baseˇ_url}}/users");
    cx.simulate_mouse_move(hover_point, None, Modifiers::default());
    cx.background_executor.advance_clock(hover_delay);
    assert_eq!(request_count.load(atomic::Ordering::Acquire), 1);
    cx.update_editor(|editor, _, _| {
        assert_eq!(editor.hover_state.info_popovers.len(), 1);
    });
}

#[gpui::test]
fn test_keyboard_hover_info_popover(cx: &mut TestAppContext) {
    init_test(cx);

    let mut cx = EditorTestContext::new(cx);

    cx.set_state("{{baseˇ_url}}/users");
    let request_count = handle_hover_requests(&mut cx);
    cx.dispatch_action(actions::editor::Hover);
    assert_eq!(request_count.load(atomic::Ordering::Acquire), 1);
    cx.update_editor(|editor, _, _| {
        assert_eq!(editor.hover_state.info_popovers.len(), 1);
        assert_eq!(
            editor.hover_state.info_popovers[0].hover.variable,
            Some((
                VariableScope::Project,
                SharedString::from("https://api.zaku.dev")
            ))
        );
    });
}

#[gpui::test]
fn test_hover_popover_hiding_delay(cx: &mut TestAppContext) {
    init_test(cx);

    let custom_delay_ms = 500;
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store
                .set_user_settings(
                    &json!({ "editor": { "hover_popover": { "hiding_delay": custom_delay_ms } } })
                        .to_string(),
                    cx,
                )
                .result()
                .unwrap();
        });
    });

    let mut cx = EditorTestContext::new(cx);

    cx.set_state("ˇ{{base_url}}/users");
    let request_count = handle_hover_requests(&mut cx);
    let hover_point = cx.pixel_position("{{baseˇ_url}}/users");
    cx.simulate_mouse_move(hover_point, None, Modifiers::default());
    cx.background_executor
        .advance_clock(Duration::from_millis(get_hover_popover_delay(&cx) + 100));
    assert_eq!(request_count.load(atomic::Ordering::Acquire), 1);
    cx.update_editor(|editor, _, _| {
        assert_eq!(editor.hover_state.info_popovers.len(), 1);
    });

    let hover_point = cx.gutter_position();
    cx.simulate_mouse_move(hover_point, None, Modifiers::default());
    cx.background_executor
        .advance_clock(Duration::from_millis(custom_delay_ms - 100));
    cx.update_editor(|editor, _, _| {
        assert_eq!(editor.hover_state.info_popovers.len(), 1);
    });

    cx.background_executor
        .advance_clock(Duration::from_millis(200));
    cx.update_editor(|editor, _, _| {
        assert!(editor.hover_state.info_popovers.is_empty());
    });
}

#[gpui::test]
fn test_hover_popover_cancel_hide_on_rehover(cx: &mut TestAppContext) {
    init_test(cx);

    let custom_delay_ms = 500;
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store
                .set_user_settings(
                    &json!({ "editor": { "hover_popover": { "hiding_delay": custom_delay_ms } } })
                        .to_string(),
                    cx,
                )
                .result()
                .unwrap();
        });
    });

    let mut cx = EditorTestContext::new(cx);

    cx.set_state("ˇ{{base_url}}/users");
    let request_count = handle_hover_requests(&mut cx);
    let hover_point = cx.pixel_position("{{baseˇ_url}}/users");
    cx.simulate_mouse_move(hover_point, None, Modifiers::default());
    cx.background_executor
        .advance_clock(Duration::from_millis(get_hover_popover_delay(&cx) + 100));
    assert_eq!(request_count.load(atomic::Ordering::Acquire), 1);
    cx.update_editor(|editor, _, _| {
        assert_eq!(editor.hover_state.info_popovers.len(), 1);
    });

    let gutter_point = cx.gutter_position();
    cx.simulate_mouse_move(gutter_point, None, Modifiers::default());
    cx.background_executor
        .advance_clock(Duration::from_millis(300));
    cx.update_editor(|editor, _, _| {
        assert_eq!(editor.hover_state.info_popovers.len(), 1);
    });

    cx.simulate_mouse_move(hover_point, None, Modifiers::default());
    cx.background_executor
        .advance_clock(Duration::from_millis(300));
    cx.update_editor(|editor, _, _| {
        assert_eq!(editor.hover_state.info_popovers.len(), 1);
    });

    cx.simulate_mouse_move(gutter_point, None, Modifiers::default());
    cx.background_executor
        .advance_clock(Duration::from_millis(custom_delay_ms + 100));
    cx.update_editor(|editor, _, _| {
        assert!(editor.hover_state.info_popovers.is_empty());
    });
}

#[gpui::test]
fn test_hover_popover_hiding_delay_restarts_when_mouse_gets_closer(cx: &mut TestAppContext) {
    init_test(cx);

    let custom_delay_ms = 600;
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store
                .set_user_settings(
                    &json!({ "editor": { "hover_popover": { "hiding_delay": custom_delay_ms } } })
                        .to_string(),
                    cx,
                )
                .result()
                .unwrap();
        });
    });

    let mut cx = EditorTestContext::new(cx);

    cx.set_state("ˇ{{base_url}}/users");
    let request_count = handle_hover_requests(&mut cx);
    let hover_delay = Duration::from_millis(get_hover_popover_delay(&cx) + 100);
    let hover_point = cx.pixel_position("{{baseˇ_url}}/users");
    cx.simulate_mouse_move(hover_point, None, Modifiers::default());
    cx.background_executor.advance_clock(hover_delay);
    assert_eq!(request_count.load(atomic::Ordering::Acquire), 1);
    cx.update_editor(|editor, _, _| {
        assert_eq!(editor.hover_state.info_popovers.len(), 1);
    });

    let popover_bounds = cx.debug_bounds("POPOVER-info_popover").unwrap();
    let far_point = gpui::point(
        popover_bounds.right() + gpui::px(60.0),
        popover_bounds.center().y,
    );
    cx.simulate_mouse_move(far_point, None, Modifiers::default());
    cx.background_executor
        .advance_clock(Duration::from_millis(400));

    let closer_point = gpui::point(
        popover_bounds.right() + gpui::px(20.0),
        popover_bounds.center().y,
    );
    cx.simulate_mouse_move(closer_point, None, Modifiers::default());
    cx.background_executor
        .advance_clock(Duration::from_millis(250));
    cx.update_editor(|editor, _, _| {
        assert_eq!(editor.hover_state.info_popovers.len(), 1);
    });

    cx.background_executor
        .advance_clock(Duration::from_millis(350));
    cx.update_editor(|editor, _, _| {
        assert!(editor.hover_state.info_popovers.is_empty());
    });

    cx.simulate_mouse_move(hover_point, None, Modifiers::default());
    cx.background_executor.advance_clock(hover_delay);
    cx.update_editor(|editor, _, _| {
        assert_eq!(editor.hover_state.info_popovers.len(), 1);
    });

    let popover_bounds = cx.debug_bounds("POPOVER-info_popover").unwrap();
    let gutter_point = cx.gutter_position();
    cx.simulate_mouse_move(gutter_point, None, Modifiers::default());
    cx.simulate_mouse_move(popover_bounds.center(), None, Modifiers::default());
    cx.background_executor
        .advance_clock(Duration::from_millis(custom_delay_ms + 100));
    cx.update_editor(|editor, _, _| {
        assert_eq!(editor.hover_state.info_popovers.len(), 1);
    });
}

#[gpui::test]
fn test_hover_popover_enabled_false(cx: &mut TestAppContext) {
    init_test(cx);

    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store
                .set_user_settings(
                    &json!({ "editor": { "hover_popover": { "enabled": false } } }).to_string(),
                    cx,
                )
                .result()
                .unwrap();
        });
    });

    let mut cx = EditorTestContext::new(cx);

    cx.set_state("{{baseˇ_url}}/users");
    let request_count = handle_hover_requests(&mut cx);
    let hover_point = cx.pixel_position("{{baseˇ_url}}/users");
    cx.simulate_mouse_move(hover_point, None, Modifiers::default());
    cx.background_executor
        .advance_clock(Duration::from_millis(get_hover_popover_delay(&cx) + 100));
    assert_eq!(request_count.load(atomic::Ordering::Acquire), 0);
    cx.update_editor(|editor, _, _| {
        assert!(editor.hover_state.info_popovers.is_empty());
    });

    cx.dispatch_action(actions::editor::Hover);
    assert_eq!(request_count.load(atomic::Ordering::Acquire), 1);
    cx.update_editor(|editor, _, _| {
        assert_eq!(editor.hover_state.info_popovers.len(), 1);
    });
}

#[gpui::test]
fn test_mouse_hover_info_popover_with_autocomplete_popover(cx: &mut TestAppContext) {
    init_test(cx);

    let mut cx = EditorTestContext::new(cx);

    cx.set_state("{{base_url}}/users ˇ");
    let request_count = handle_hover_requests(&mut cx);
    handle_completion_requests(&mut cx);
    cx.simulate_keystroke("{");
    cx.simulate_keystroke("{");
    cx.run_until_parked();
    cx.update_editor(|editor, _, _| {
        assert!(editor.has_visible_completions_menu());
    });

    let hover_delay = Duration::from_millis(get_hover_popover_delay(&cx) + 100);
    let hover_point = cx.pixel_position("{{baseˇ_url}}/users {{");
    cx.simulate_mouse_move(hover_point, None, Modifiers::default());
    cx.background_executor.advance_clock(hover_delay);
    assert_eq!(request_count.load(atomic::Ordering::Acquire), 0);
    cx.update_editor(|editor, _, _| {
        assert!(editor.has_visible_completions_menu());
        assert!(editor.hover_state.info_popovers.is_empty());
    });

    cx.dispatch_action(actions::editor::Cancel);
    cx.update_editor(|editor, _, _| {
        assert!(!editor.has_visible_completions_menu());
    });
    cx.simulate_mouse_move(hover_point, None, Modifiers::default());
    cx.background_executor.advance_clock(hover_delay);
    assert_eq!(request_count.load(atomic::Ordering::Acquire), 1);
    cx.update_editor(|editor, _, _| {
        assert_eq!(editor.hover_state.info_popovers.len(), 1);
    });
}

#[gpui::test]
fn test_keyboard_hover_grace(cx: &mut TestAppContext) {
    init_test(cx);

    let mut cx = EditorTestContext::new(cx);

    cx.set_state("{{baseˇ_url}}/users");
    let request_count = handle_hover_requests(&mut cx);
    cx.dispatch_action(actions::editor::Hover);
    assert_eq!(request_count.load(atomic::Ordering::Acquire), 1);
    cx.update_editor(|editor, _, _| {
        assert_eq!(editor.hover_state.info_popovers.len(), 1);
    });

    let hover_delay = Duration::from_millis(get_hover_popover_delay(&cx) + 100);
    let hiding_delay = cx.read(|cx| {
        Duration::from_millis(EditorSettings::get_global(cx).hover_popover.hiding_delay.0 + 100)
    });
    let hover_point = cx.pixel_position("{{base_url}}/ˇusers");
    cx.simulate_mouse_move(hover_point, None, Modifiers::default());
    cx.background_executor.advance_clock(hover_delay);
    let gutter_point = cx.gutter_position();
    cx.simulate_mouse_move(gutter_point, None, Modifiers::default());
    cx.background_executor.advance_clock(hiding_delay);
    assert_eq!(request_count.load(atomic::Ordering::Acquire), 1);
    cx.update_editor(|editor, _, _| {
        assert_eq!(editor.hover_state.info_popovers.len(), 1);
    });

    let popover_bounds = cx.debug_bounds("POPOVER-info_popover").unwrap();
    cx.simulate_click(popover_bounds.center(), Modifiers::default());
    cx.simulate_mouse_move(gutter_point, None, Modifiers::default());
    cx.background_executor.advance_clock(hiding_delay);
    cx.update_editor(|editor, _, _| {
        assert!(editor.hover_state.info_popovers.is_empty());
    });
}

#[gpui::test]
fn test_context_menus_hide_hover_popover(cx: &mut TestAppContext) {
    init_test(cx);

    let mut cx = EditorTestContext::new(cx);

    cx.set_state("{{baseˇ_url}}/users");
    handle_hover_requests(&mut cx);
    handle_completion_requests(&mut cx);
    cx.dispatch_action(actions::editor::Hover);
    cx.update_editor(|editor, _, _| {
        assert_eq!(editor.hover_state.info_popovers.len(), 1);
    });

    cx.dispatch_action(actions::editor::ShowCompletions);
    cx.update_editor(|editor, _, _| {
        assert!(editor.has_visible_completions_menu());
        assert!(editor.hover_state.info_popovers.is_empty());
    });
}

#[gpui::test]
fn test_completions_use_selection_head(cx: &mut TestAppContext) {
    init_test(cx);

    let mut cx = EditorTestContext::new(cx);

    cx.set_state("{{«baˇ»}}");
    handle_completion_requests(&mut cx);
    cx.dispatch_action(actions::editor::ShowCompletions);
    cx.update_editor(|editor, _, _| {
        if let Some(menu) = editor.context_menu().borrow().as_ref() {
            assert_eq!(completion_menu_entries(menu), &["base_url"]);
        } else {
            panic!("expected completion menu to be open");
        }
    });
}

#[gpui::test]
fn test_show_completions(cx: &mut TestAppContext) {
    init_test(cx);

    let mut cx = EditorTestContext::new(cx);

    cx.set_state("{baˇ");
    let request_count = handle_completion_requests(&mut cx);
    cx.dispatch_action(actions::editor::ShowCompletions);
    assert_eq!(request_count.load(atomic::Ordering::Acquire), 0);
    cx.update_editor(|editor, _, _| {
        assert!(!editor.has_visible_completions_menu());
    });

    cx.set_state("{{baˇ");
    cx.dispatch_action(actions::editor::ShowCompletions);
    assert_eq!(request_count.load(atomic::Ordering::Acquire), 1);
    cx.update_editor(|editor, _, _| {
        if let Some(menu) = editor.context_menu().borrow().as_ref() {
            assert_eq!(completion_menu_entries(menu), &["base_url"]);
        } else {
            panic!("expected completion menu to be open");
        }
    });
}

#[gpui::test]
fn test_hover_popover_hides_on_parent_scroll(cx: &mut TestAppContext) {
    init_test(cx);

    let (container, cx) = cx.add_window_view(|window, cx| TestScrollContainer {
        editor: cx.new(|cx| Editor::auto_height(1, Some(4), window, cx)),
    });
    let editor = container.read_with(cx, |container, _| container.editor.clone());
    let mut cx = EditorTestContext::for_editor_in(editor, cx);

    cx.set_state("ˇ{{base_url}}/users");
    handle_hover_requests(&mut cx);
    let hover_point = cx.pixel_position("{{baseˇ_url}}/users");
    cx.simulate_mouse_move(hover_point, None, Modifiers::default());
    cx.background_executor
        .advance_clock(Duration::from_millis(get_hover_popover_delay(&cx) + 100));
    let popover_bounds = cx.debug_bounds("POPOVER-info_popover").unwrap();

    cx.simulate_event(ScrollEvent {
        position: popover_bounds.center(),
        delta: ScrollDelta::Pixels(gpui::point(gpui::px(0.0), gpui::px(-100.0))),
        ..Default::default()
    });
    assert!(cx.debug_bounds("POPOVER-info_popover").is_some());
    cx.update_editor(|editor, _, _| {
        assert_eq!(editor.hover_state.info_popovers.len(), 1);
    });

    cx.simulate_event(ScrollEvent {
        position: hover_point,
        delta: ScrollDelta::Pixels(gpui::point(gpui::px(0.0), gpui::px(-100.0))),
        ..Default::default()
    });
    assert!(cx.debug_bounds("POPOVER-info_popover").is_none());
    cx.update_editor(|editor, _, _| {
        assert!(editor.hover_state.info_popovers.is_empty());
    });

    cx.simulate_event(ScrollEvent {
        position: hover_point,
        delta: ScrollDelta::Pixels(gpui::point(gpui::px(0.0), gpui::px(100.0))),
        ..Default::default()
    });
    assert!(cx.debug_bounds("POPOVER-info_popover").is_none());
}

#[gpui::test]
fn test_completions_menu_hidden_when_clipped_by_parent(cx: &mut TestAppContext) {
    init_test(cx);

    let (container, cx) = cx.add_window_view(|window, cx| TestScrollContainer {
        editor: cx.new(|cx| Editor::auto_height(1, Some(4), window, cx)),
    });
    let editor = container.read_with(cx, |container, _| container.editor.clone());
    let mut cx = EditorTestContext::for_editor_in(editor, cx);

    cx.set_state("ˇ");
    handle_completion_requests(&mut cx);
    cx.simulate_input("{{");
    assert!(cx.debug_bounds("MENU-completions").is_some());

    let scroll_point = cx.pixel_position("{{ˇ");
    cx.simulate_event(ScrollEvent {
        position: scroll_point,
        delta: ScrollDelta::Pixels(gpui::point(gpui::px(0.0), gpui::px(-100.0))),
        ..Default::default()
    });
    assert!(cx.debug_bounds("MENU-completions").is_none());
    cx.update_editor(|editor, _, _| {
        assert!(editor.has_visible_completions_menu());
    });

    cx.simulate_event(ScrollEvent {
        position: scroll_point,
        delta: ScrollDelta::Pixels(gpui::point(gpui::px(0.0), gpui::px(100.0))),
        ..Default::default()
    });
    assert!(cx.debug_bounds("MENU-completions").is_some());
}

#[gpui::test]
fn test_completions_menu_partially_clipped_by_parent(cx: &mut TestAppContext) {
    init_test(cx);

    let (container, cx) = cx.add_window_view(|window, cx| TestScrollContainer {
        editor: cx.new(|cx| Editor::auto_height(1, Some(4), window, cx)),
    });
    let editor = container.read_with(cx, |container, _| container.editor.clone());
    let mut cx = EditorTestContext::for_editor_in(editor, cx);

    cx.set_state(indoc! {"
        ˇ
        the quick brown fox
        jumps over the lazy dog\
    "});
    handle_completion_requests(&mut cx);
    let line_height = cx.update_editor(|editor, window, cx| {
        editor
            .style(cx)
            .text
            .line_height_in_pixels(window.rem_size())
    });
    let viewport_size = cx.update(|window, _| window.viewport_size());
    let scroll_point = gpui::point(
        viewport_size.width - gpui::px(10.0),
        cx.pixel_position(indoc! {"

            the quick ˇbrown fox
            jumps over the lazy dog\
        "})
            .y,
    );

    cx.simulate_input("{{");
    assert!(cx.debug_bounds("MENU-completions").is_some());

    cx.simulate_event(ScrollEvent {
        position: scroll_point,
        delta: ScrollDelta::Pixels(gpui::point(gpui::px(0.0), -line_height * 1.5)),
        ..Default::default()
    });
    assert!(cx.debug_bounds("MENU-completions").is_none());
    cx.update_editor(|editor, _, _| {
        assert!(editor.has_visible_completions_menu());
    });

    cx.simulate_event(ScrollEvent {
        position: scroll_point,
        delta: ScrollDelta::Pixels(gpui::point(gpui::px(0.0), line_height * 1.5)),
        ..Default::default()
    });
    assert!(cx.debug_bounds("MENU-completions").is_some());

    cx.dispatch_action(actions::editor::Cancel);
    cx.set_selections_state(indoc! {"
        {{
        the quick brown fox
        jumps over the lazy dogˇ\
    "});
    cx.simulate_input("{{");
    assert!(cx.debug_bounds("MENU-completions").is_some());

    cx.simulate_event(ScrollEvent {
        position: scroll_point,
        delta: ScrollDelta::Pixels(gpui::point(gpui::px(0.0), -line_height * 1.5)),
        ..Default::default()
    });
    assert!(cx.debug_bounds("MENU-completions").is_some());
}

#[gpui::test]
fn test_hover_popover_hides_on_other_editor_scroll(cx: &mut TestAppContext) {
    init_test(cx);

    let (request_view, cx) = cx.add_window_view(|window, cx| TestRequestView {
        url: cx.new(|cx| Editor::single_line(window, cx)),
        body: cx.new(|cx| Editor::full(window, cx)),
    });
    let (url, body) = request_view.read_with(cx, |request_view, _| {
        (request_view.url.clone(), request_view.body.clone())
    });

    let mut body_cx = EditorTestContext::for_editor_in(body, cx);
    body_cx.set_state(indoc! {r#"
        ˇ{
          "user_id": "{{user_id}}"
        }"#});
    let viewport_size = body_cx.update(|window, _| window.viewport_size());
    let scroll_point = gpui::point(
        viewport_size.width - gpui::px(50.0),
        body_cx
            .pixel_position(indoc! {r#"
                {
                  "user_ˇid": "{{user_id}}"
                }"#})
            .y,
    );

    let mut url_cx = EditorTestContext::for_editor_in(url, cx);
    url_cx.set_state("{{baseˇ_url}}/users");
    handle_hover_requests(&mut url_cx);
    url_cx.dispatch_action(actions::editor::Hover);
    assert!(url_cx.debug_bounds("POPOVER-info_popover").is_some());

    body_cx.simulate_event(ScrollEvent {
        position: scroll_point,
        delta: ScrollDelta::Pixels(gpui::point(gpui::px(0.0), gpui::px(-20.0))),
        ..Default::default()
    });
    assert!(url_cx.debug_bounds("POPOVER-info_popover").is_none());
    url_cx.update_editor(|editor, _, _| {
        assert!(editor.hover_state.info_popovers.is_empty());
    });
}
