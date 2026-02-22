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
fn test_move_to_beginning_of_line_toggles_indent(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state("•••Lorem ipsum dolor sit ametˇ");
    let move_to_beginning = MoveToBeginningOfLine {
        stop_at_soft_wraps: true,
        stop_at_indent: true,
    };

    cx.dispatch_action(move_to_beginning.clone());
    cx.assert_state("•••ˇLorem ipsum dolor sit amet");

    cx.dispatch_action(move_to_beginning);
    cx.assert_state("ˇ•••Lorem ipsum dolor sit amet");
}

#[gpui::test]
fn test_delete_to_beginning_of_line_respects_indent(cx: &mut TestAppContext) {
    init_test(cx);
    let mut cx = EditorTestContext::new(cx);

    cx.set_state("•••Lorem ipsum dolor sit ametˇ");
    cx.dispatch_action(DeleteToBeginningOfLine {
        stop_at_indent: true,
    });
    cx.assert_state("•••ˇ");
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
