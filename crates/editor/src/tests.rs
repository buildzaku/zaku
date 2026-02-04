mod context;

use gpui::{ClipboardItem, TestAppContext};
use pretty_assertions::assert_eq;

use context::EditorTestContext;

use crate::{
    Backspace, Copy, Cut, Delete, DeleteToBeginningOfLine, HandleInput, MoveLeft, MoveRight,
    MoveToBeginningOfLine, Paste, Redo, RedoSelection, Undo, UndoSelection,
};

fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::init(cx);
    });
}

#[gpui::test]
fn test_handle_input_replaces_selection(cx: &mut TestAppContext) {
    init_test(cx);
    let mut editor_test_context = EditorTestContext::new(cx);

    editor_test_context.set_state("Hello, «worldˇ»!");
    editor_test_context.dispatch_action(HandleInput("from Comet".to_string()));
    editor_test_context.assert_state("Hello, from Cometˇ!");

    editor_test_context.set_state("Lorem ˇipsum dolor sit amet");
    editor_test_context.dispatch_action(HandleInput("ips\num\r".to_string()));
    editor_test_context.assert_state("Lorem ipsumˇipsum dolor sit amet");
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
