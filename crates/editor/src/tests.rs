mod context;

use gpui::{ClipboardItem, TestAppContext};
use indoc::indoc;
use pretty_assertions::assert_eq;

use settings::SettingsStore;

use crate::{
    Backspace, Copy, Cut, Delete, DeleteToBeginningOfLine, HandleInput, MoveDown, MoveLeft,
    MoveRight, MoveToBeginning, MoveToBeginningOfLine, MoveToEnd, MoveUp, Paste, Redo,
    RedoSelection, Undo, UndoSelection, tests::context::EditorTestContext,
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
    let mut editor_test_context = EditorTestContext::new(cx);

    editor_test_context.set_state("Hello, «worldˇ»!");
    editor_test_context.dispatch_action(HandleInput("from Zaku".to_string()));
    editor_test_context.assert_state("Hello, from Zakuˇ!");

    editor_test_context.set_state(indoc! {"
        Lorem «ipsumˇ» dolor sit amet
    "});
    editor_test_context.dispatch_action(HandleInput("ips\num".to_string()));
    editor_test_context.assert_state(indoc! {"
        Lorem ips
        umˇ dolor sit amet
    "});
}

#[gpui::test]
fn test_backspace_and_delete_actions(cx: &mut TestAppContext) {
    init_test(cx);
    let mut editor_test_context = EditorTestContext::new(cx);

    editor_test_context.set_state("Hello, woˇrld!");
    editor_test_context.dispatch_action(Backspace);
    editor_test_context.assert_state("Hello, wˇrld!");

    editor_test_context.dispatch_action(Delete);
    editor_test_context.assert_state("Hello, wˇld!");
}

#[gpui::test]
fn test_move_to_beginning_of_line_toggles_indent(cx: &mut TestAppContext) {
    init_test(cx);
    let mut editor_test_context = EditorTestContext::new(cx);

    editor_test_context.set_state("•••Lorem ipsum dolor sit ametˇ");
    let move_to_beginning = MoveToBeginningOfLine {
        stop_at_soft_wraps: true,
        stop_at_indent: true,
    };

    editor_test_context.dispatch_action(move_to_beginning.clone());
    editor_test_context.assert_state("•••ˇLorem ipsum dolor sit amet");

    editor_test_context.dispatch_action(move_to_beginning);
    editor_test_context.assert_state("ˇ•••Lorem ipsum dolor sit amet");
}

#[gpui::test]
fn test_delete_to_beginning_of_line_respects_indent(cx: &mut TestAppContext) {
    init_test(cx);
    let mut editor_test_context = EditorTestContext::new(cx);

    editor_test_context.set_state("•••Lorem ipsum dolor sit ametˇ");
    editor_test_context.dispatch_action(DeleteToBeginningOfLine {
        stop_at_indent: true,
    });
    editor_test_context.assert_state("•••ˇ");
}

#[gpui::test]
fn test_undo_redo_restores_cursor(cx: &mut TestAppContext) {
    init_test(cx);
    let mut editor_test_context = EditorTestContext::new(cx);

    editor_test_context.set_state("Hello, woˇrld!");
    editor_test_context.dispatch_action(HandleInput("d".to_string()));
    editor_test_context.assert_state("Hello, wodˇrld!");

    editor_test_context.dispatch_action(Undo);
    editor_test_context.assert_state("Hello, woˇrld!");

    editor_test_context.dispatch_action(Redo);
    editor_test_context.assert_state("Hello, wodˇrld!");
}

#[gpui::test]
fn test_undo_redo_restores_selection(cx: &mut TestAppContext) {
    init_test(cx);
    let mut editor_test_context = EditorTestContext::new(cx);

    editor_test_context.set_state("Hello, «worldˇ»!");
    editor_test_context.dispatch_action(HandleInput("from Zaku".to_string()));
    editor_test_context.assert_state("Hello, from Zakuˇ!");

    editor_test_context.dispatch_action(Undo);
    editor_test_context.assert_state("Hello, «worldˇ»!");

    editor_test_context.dispatch_action(Redo);
    editor_test_context.assert_state("Hello, from Zakuˇ!");
}

#[gpui::test]
fn test_undo_redo_selection(cx: &mut TestAppContext) {
    init_test(cx);
    let mut editor_test_context = EditorTestContext::new(cx);

    editor_test_context.set_state("Hello, woˇrld!");
    editor_test_context.dispatch_action(MoveRight);
    editor_test_context.assert_state("Hello, worˇld!");

    editor_test_context.dispatch_action(MoveLeft);
    editor_test_context.assert_state("Hello, woˇrld!");

    editor_test_context.dispatch_action(UndoSelection);
    editor_test_context.assert_state("Hello, worˇld!");

    editor_test_context.dispatch_action(RedoSelection);
    editor_test_context.assert_state("Hello, woˇrld!");
}

#[gpui::test]
fn test_copy_cut_paste_actions(cx: &mut TestAppContext) {
    init_test(cx);
    let mut editor_test_context = EditorTestContext::new(cx);

    editor_test_context.set_state("Hello, «worldˇ»!");
    editor_test_context.dispatch_action(Copy);
    let clipboard_text = editor_test_context
        .cx
        .read_from_clipboard()
        .and_then(|item: ClipboardItem| item.text());
    assert_eq!(clipboard_text.as_deref(), Some("world"));

    editor_test_context.dispatch_action(Cut);
    editor_test_context.assert_state("Hello, ˇ!");
    let clipboard_text = editor_test_context
        .cx
        .read_from_clipboard()
        .and_then(|item: ClipboardItem| item.text());
    assert_eq!(clipboard_text.as_deref(), Some("world"));

    editor_test_context
        .cx
        .write_to_clipboard(ClipboardItem::new_string("hello world".to_string()));
    editor_test_context.dispatch_action(Paste);
    editor_test_context.assert_state("Hello, hello worldˇ!");
}

#[gpui::test]
fn test_move_cursor(cx: &mut TestAppContext) {
    init_test(cx);
    let mut editor_test_context = EditorTestContext::new(cx);

    editor_test_context.set_state(indoc! {"
        ˇaaaaaa
        \t\taaaaaa
        aaaaaa\
    "});

    editor_test_context.dispatch_action(MoveDown);
    editor_test_context.assert_state(indoc! {"
        aaaaaa
        ˇ\t\taaaaaa
        aaaaaa\
    "});

    editor_test_context.dispatch_action(MoveRight);
    editor_test_context.assert_state(indoc! {"
        aaaaaa
        \tˇ\taaaaaa
        aaaaaa\
    "});

    editor_test_context.dispatch_action(MoveLeft);
    editor_test_context.assert_state(indoc! {"
        aaaaaa
        ˇ\t\taaaaaa
        aaaaaa\
    "});

    editor_test_context.dispatch_action(MoveUp);
    editor_test_context.assert_state(indoc! {"
        ˇaaaaaa
        \t\taaaaaa
        aaaaaa\
    "});

    editor_test_context.dispatch_action(MoveToEnd);
    editor_test_context.assert_state(indoc! {"
        aaaaaa
        \t\taaaaaa
        aaaaaaˇ\
    "});

    editor_test_context.dispatch_action(MoveToBeginning);
    editor_test_context.assert_state(indoc! {"
        ˇaaaaaa
        \t\taaaaaa
        aaaaaa\
    "});
}

#[gpui::test]
fn test_move_cursor_multibyte(cx: &mut TestAppContext) {
    init_test(cx);
    let mut editor_test_context = EditorTestContext::new(cx);

    editor_test_context.set_state(indoc! {"
        ˇ🌑🌒🌓🌔🌕🌖
        abcde
        абвгд
    "});

    editor_test_context.dispatch_action(MoveRight);
    editor_test_context.assert_state(indoc! {"
        🌑ˇ🌒🌓🌔🌕🌖
        abcde
        абвгд
    "});

    editor_test_context.dispatch_action(MoveRight);
    editor_test_context.assert_state(indoc! {"
        🌑🌒ˇ🌓🌔🌕🌖
        abcde
        абвгд
    "});

    editor_test_context.dispatch_action(MoveRight);
    editor_test_context.assert_state(indoc! {"
        🌑🌒🌓ˇ🌔🌕🌖
        abcde
        абвгд
    "});

    editor_test_context.dispatch_action(MoveDown);
    editor_test_context.assert_state(indoc! {"
        🌑🌒🌓🌔🌕🌖
        abcdeˇ
        абвгд
    "});

    editor_test_context.dispatch_action(MoveDown);
    editor_test_context.assert_state(indoc! {"
        🌑🌒🌓🌔🌕🌖
        abcde
        абвгдˇ
    "});

    editor_test_context.dispatch_action(MoveLeft);
    editor_test_context.dispatch_action(MoveLeft);
    editor_test_context.assert_state(indoc! {"
        🌑🌒🌓🌔🌕🌖
        abcde
        абвˇгд
    "});

    editor_test_context.dispatch_action(MoveUp);
    editor_test_context.assert_state(indoc! {"
        🌑🌒🌓🌔🌕🌖
        abcˇde
        абвгд
    "});

    editor_test_context.dispatch_action(MoveUp);
    editor_test_context.assert_state(indoc! {"
        🌑ˇ🌒🌓🌔🌕🌖
        abcde
        абвгд
    "});
}

#[gpui::test]
fn test_move_cursor_different_line_lengths(cx: &mut TestAppContext) {
    init_test(cx);
    let mut editor_test_context = EditorTestContext::new(cx);

    editor_test_context.set_state(indoc! {"
        ⓐⓑⓒⓓⓔˇ
        abcd
        αβγ
        abcd
        ⓐⓑⓒⓓⓔ\
    "});

    editor_test_context.dispatch_action(MoveDown);
    editor_test_context.assert_state(indoc! {"
        ⓐⓑⓒⓓⓔ
        abcdˇ
        αβγ
        abcd
        ⓐⓑⓒⓓⓔ\
    "});

    editor_test_context.dispatch_action(MoveDown);
    editor_test_context.assert_state(indoc! {"
        ⓐⓑⓒⓓⓔ
        abcd
        αβγˇ
        abcd
        ⓐⓑⓒⓓⓔ\
    "});

    editor_test_context.dispatch_action(MoveDown);
    editor_test_context.assert_state(indoc! {"
        ⓐⓑⓒⓓⓔ
        abcd
        αβγ
        abcdˇ
        ⓐⓑⓒⓓⓔ\
    "});

    editor_test_context.dispatch_action(MoveDown);
    editor_test_context.assert_state(indoc! {"
        ⓐⓑⓒⓓⓔ
        abcd
        αβγ
        abcd
        ⓐⓑⓒⓓⓔˇ\
    "});

    editor_test_context.dispatch_action(MoveDown);
    editor_test_context.assert_state(indoc! {"
        ⓐⓑⓒⓓⓔ
        abcd
        αβγ
        abcd
        ⓐⓑⓒⓓⓔˇ\
    "});

    editor_test_context.dispatch_action(MoveUp);
    editor_test_context.assert_state(indoc! {"
        ⓐⓑⓒⓓⓔ
        abcd
        αβγ
        abcdˇ
        ⓐⓑⓒⓓⓔ\
    "});

    editor_test_context.dispatch_action(MoveUp);
    editor_test_context.assert_state(indoc! {"
        ⓐⓑⓒⓓⓔ
        abcd
        αβγˇ
        abcd
        ⓐⓑⓒⓓⓔ\
    "});
}
