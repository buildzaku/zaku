mod context;

use gpui::{ClipboardItem, TestAppContext};
use indoc::indoc;
use pretty_assertions::assert_eq;

use settings::SettingsStore;

use crate::{
    Backspace, Copy, Cut, Delete, DeleteToBeginningOfLine, DeleteToEndOfLine,
    DeleteToNextSubwordEnd, DeleteToNextWordEnd, DeleteToPreviousSubwordStart,
    DeleteToPreviousWordStart, HandleInput, MoveDown, MoveLeft, MoveRight, MoveToBeginning,
    MoveToBeginningOfLine, MoveToEnd, MoveToEndOfLine, MoveToNextWordEnd, MoveToPreviousWordStart,
    MoveUp, Paste, Redo, RedoSelection, SelectToBeginningOfLine, SelectToEndOfLine, Undo,
    UndoSelection, tests::context::EditorTestContext,
};

fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        theme::init(theme::LoadThemes::JustBase, cx);
        crate::init(cx);
    });
}

#[gpui::test]
fn test_handle_input_replaces_selection(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state("Hello, «worldˇ»!");
    cx.dispatch_action(HandleInput("from Zaku".to_string()));
    cx.assert_state("Hello, from Zakuˇ!");

    cx.set_state(indoc! {"
        Lorem «ipsumˇ» dolor sit amet
    "});
    cx.dispatch_action(HandleInput("ips\num".to_string()));
    cx.assert_state(indoc! {"
        Lorem ips
        umˇ dolor sit amet
    "});
}

#[gpui::test]
fn test_backspace_and_delete_actions(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state("Hello, woˇrld!");
    cx.dispatch_action(Backspace);
    cx.assert_state("Hello, wˇrld!");

    cx.dispatch_action(Delete);
    cx.assert_state("Hello, wˇld!");
}

#[gpui::test]
fn test_move_beginning_of_line_stops_at_indent(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state("•••The quick brown fox jumps over the lazy dogˇ");
    let move_to_beginning = MoveToBeginningOfLine {
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
    cx.dispatch_action(DeleteToBeginningOfLine {
        stop_at_indent: true,
    });
    cx.assert_state("•••ˇ");
}

#[gpui::test]
fn test_beginning_of_line(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    let move_to_beginning_of_line = MoveToBeginningOfLine {
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
    cx.dispatch_action(SelectToBeginningOfLine {
        stop_at_soft_wraps: true,
        stop_at_indent: true,
    });
    cx.assert_state(indoc! {"
        The quick brown fox
        ••«ˇjumps over the lazy d»og
    "});

    cx.dispatch_action(SelectToBeginningOfLine {
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
    cx.dispatch_action(DeleteToBeginningOfLine {
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

    cx.dispatch_action(MoveToEndOfLine {
        stop_at_soft_wraps: true,
    });
    cx.assert_state(indoc! {"
        The quick brown fox
        ••jumps over the lazy dogˇ
    "});

    cx.dispatch_action(MoveToEndOfLine {
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
    cx.dispatch_action(SelectToEndOfLine {
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
    cx.dispatch_action(DeleteToEndOfLine);
    cx.assert_state(indoc! {"
        The quick brown fox
        ••jumps over the lazy dˇ
    "});
}

#[gpui::test]
fn test_beginning_of_line_with_cursor_between_line_start_and_indent(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    let move_to_beginning_of_line = MoveToBeginningOfLine {
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

    cx.dispatch_action(MoveToPreviousWordStart);
    cx.assert_state("one two.ˇthree");

    cx.dispatch_action(MoveToPreviousWordStart);
    cx.assert_state("one ˇtwo.three");

    cx.dispatch_action(MoveToPreviousWordStart);
    cx.assert_state("ˇone two.three");

    cx.dispatch_action(MoveToPreviousWordStart);
    cx.assert_state("ˇone two.three");

    cx.dispatch_action(MoveToNextWordEnd);
    cx.assert_state("oneˇ two.three");

    cx.dispatch_action(MoveToNextWordEnd);
    cx.assert_state("one twoˇ.three");

    cx.dispatch_action(MoveToNextWordEnd);
    cx.assert_state("one two.threeˇ");

    cx.dispatch_action(MoveToNextWordEnd);
    cx.assert_state("one two.threeˇ");
}

#[gpui::test]
fn test_delete_to_word_boundary(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state("one two t«hreˇ»e four");
    cx.dispatch_action(DeleteToPreviousWordStart {
        ignore_newlines: false,
        ignore_brackets: false,
    });
    cx.assert_state("one two tˇe four");

    cx.set_state("one two te «fˇ»our");
    cx.dispatch_action(DeleteToNextWordEnd {
        ignore_newlines: false,
        ignore_brackets: false,
    });
    cx.assert_state("one two te ˇour");
}

#[gpui::test]
fn test_delete_to_previous_word_start_or_newline(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    let delete_to_previous_word_start = DeleteToPreviousWordStart {
        ignore_newlines: false,
        ignore_brackets: false,
    };
    let delete_to_previous_word_start_ignore_newlines = DeleteToPreviousWordStart {
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

    let delete_to_previous_subword_start = DeleteToPreviousSubwordStart {
        ignore_newlines: false,
        ignore_brackets: false,
    };
    let delete_to_previous_subword_start_ignore_newlines = DeleteToPreviousSubwordStart {
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

    let delete_to_next_word_end = DeleteToNextWordEnd {
        ignore_newlines: false,
        ignore_brackets: false,
    };
    let delete_to_next_word_end_ignore_newlines = DeleteToNextWordEnd {
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

    let delete_to_next_subword_end = DeleteToNextSubwordEnd {
        ignore_newlines: false,
        ignore_brackets: false,
    };
    let delete_to_next_subword_end_ignore_newlines = DeleteToNextSubwordEnd {
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
    cx.dispatch_action(HandleInput("d".to_string()));
    cx.assert_state("Hello, wodˇrld!");

    cx.dispatch_action(Undo);
    cx.assert_state("Hello, woˇrld!");

    cx.dispatch_action(Redo);
    cx.assert_state("Hello, wodˇrld!");
}

#[gpui::test]
fn test_undo_redo_restores_selection(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state("Hello, «worldˇ»!");
    cx.dispatch_action(HandleInput("from Zaku".to_string()));
    cx.assert_state("Hello, from Zakuˇ!");

    cx.dispatch_action(Undo);
    cx.assert_state("Hello, «worldˇ»!");

    cx.dispatch_action(Redo);
    cx.assert_state("Hello, from Zakuˇ!");
}

#[gpui::test]
fn test_undo_redo_selection(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state("Hello, woˇrld!");
    cx.dispatch_action(MoveRight);
    cx.assert_state("Hello, worˇld!");

    cx.dispatch_action(MoveLeft);
    cx.assert_state("Hello, woˇrld!");

    cx.dispatch_action(UndoSelection);
    cx.assert_state("Hello, worˇld!");

    cx.dispatch_action(RedoSelection);
    cx.assert_state("Hello, woˇrld!");
}

#[gpui::test]
fn test_copy_cut_paste_actions(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state("Hello, «worldˇ»!");
    cx.dispatch_action(Copy);
    let clipboard_text = cx
        .cx
        .read_from_clipboard()
        .and_then(|item: ClipboardItem| item.text());
    assert_eq!(clipboard_text.as_deref(), Some("world"));

    cx.dispatch_action(Cut);
    cx.assert_state("Hello, ˇ!");
    let clipboard_text = cx
        .cx
        .read_from_clipboard()
        .and_then(|item: ClipboardItem| item.text());
    assert_eq!(clipboard_text.as_deref(), Some("world"));

    cx.cx
        .write_to_clipboard(ClipboardItem::new_string("hello world".to_string()));
    cx.dispatch_action(Paste);
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
    cx.dispatch_action(Paste);
    cx.assert_state("The quickbrown fox jumps overthe lazy dogˇ");
}

#[gpui::test]
fn test_single_line_editor_replace_text_in_range_strips_newlines(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new_single_line(cx);

    cx.set_state("Lorem «ipsumˇ»");
    cx.dispatch_action(HandleInput("ipsum\r\ndolor sit\namet".to_string()));
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

    cx.dispatch_action(MoveDown);
    cx.assert_state(indoc! {"
        aaaaaa
        ˇ\t\taaaaaa
        aaaaaa\
    "});

    cx.dispatch_action(MoveRight);
    cx.assert_state(indoc! {"
        aaaaaa
        \tˇ\taaaaaa
        aaaaaa\
    "});

    cx.dispatch_action(MoveLeft);
    cx.assert_state(indoc! {"
        aaaaaa
        ˇ\t\taaaaaa
        aaaaaa\
    "});

    cx.dispatch_action(MoveUp);
    cx.assert_state(indoc! {"
        ˇaaaaaa
        \t\taaaaaa
        aaaaaa\
    "});

    cx.dispatch_action(MoveToEnd);
    cx.assert_state(indoc! {"
        aaaaaa
        \t\taaaaaa
        aaaaaaˇ\
    "});

    cx.dispatch_action(MoveToBeginning);
    cx.assert_state(indoc! {"
        ˇaaaaaa
        \t\taaaaaa
        aaaaaa\
    "});
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

    cx.dispatch_action(MoveRight);
    cx.assert_state(indoc! {"
        🌑ˇ🌒🌓🌔🌕🌖
        abcde
        абвгд
    "});

    cx.dispatch_action(MoveRight);
    cx.assert_state(indoc! {"
        🌑🌒ˇ🌓🌔🌕🌖
        abcde
        абвгд
    "});

    cx.dispatch_action(MoveRight);
    cx.assert_state(indoc! {"
        🌑🌒🌓ˇ🌔🌕🌖
        abcde
        абвгд
    "});

    cx.dispatch_action(MoveDown);
    cx.assert_state(indoc! {"
        🌑🌒🌓🌔🌕🌖
        abcdeˇ
        абвгд
    "});

    cx.dispatch_action(MoveDown);
    cx.assert_state(indoc! {"
        🌑🌒🌓🌔🌕🌖
        abcde
        абвгдˇ
    "});

    cx.dispatch_action(MoveLeft);
    cx.dispatch_action(MoveLeft);
    cx.assert_state(indoc! {"
        🌑🌒🌓🌔🌕🌖
        abcde
        абвˇгд
    "});

    cx.dispatch_action(MoveUp);
    cx.assert_state(indoc! {"
        🌑🌒🌓🌔🌕🌖
        abcˇde
        абвгд
    "});

    cx.dispatch_action(MoveUp);
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

    cx.dispatch_action(MoveDown);
    cx.assert_state(indoc! {"
        ⓐⓑⓒⓓⓔ
        abcdˇ
        αβγ
        abcd
        ⓐⓑⓒⓓⓔ\
    "});

    cx.dispatch_action(MoveDown);
    cx.assert_state(indoc! {"
        ⓐⓑⓒⓓⓔ
        abcd
        αβγˇ
        abcd
        ⓐⓑⓒⓓⓔ\
    "});

    cx.dispatch_action(MoveDown);
    cx.assert_state(indoc! {"
        ⓐⓑⓒⓓⓔ
        abcd
        αβγ
        abcdˇ
        ⓐⓑⓒⓓⓔ\
    "});

    cx.dispatch_action(MoveDown);
    cx.assert_state(indoc! {"
        ⓐⓑⓒⓓⓔ
        abcd
        αβγ
        abcd
        ⓐⓑⓒⓓⓔˇ\
    "});

    cx.dispatch_action(MoveDown);
    cx.assert_state(indoc! {"
        ⓐⓑⓒⓓⓔ
        abcd
        αβγ
        abcd
        ⓐⓑⓒⓓⓔˇ\
    "});

    cx.dispatch_action(MoveUp);
    cx.assert_state(indoc! {"
        ⓐⓑⓒⓓⓔ
        abcd
        αβγ
        abcdˇ
        ⓐⓑⓒⓓⓔ\
    "});

    cx.dispatch_action(MoveUp);
    cx.assert_state(indoc! {"
        ⓐⓑⓒⓓⓔ
        abcd
        αβγˇ
        abcd
        ⓐⓑⓒⓓⓔ\
    "});
}
