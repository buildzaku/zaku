use gpui::Action;
use schemars::JsonSchema;
use serde::Deserialize;

use util::serde::default_true;

/// Move the cursor to the beginning of the current line.
#[derive(PartialEq, Clone, Deserialize, Default, JsonSchema, Action)]
#[action(namespace = editor)]
#[serde(deny_unknown_fields)]
pub struct MoveToBeginningOfLine {
    #[serde(default = "default_true")]
    pub stop_at_soft_wraps: bool,
    #[serde(default)]
    pub stop_at_indent: bool,
}

/// Select from the cursor to the beginning of the current line.
#[derive(PartialEq, Clone, Deserialize, Default, JsonSchema, Action)]
#[action(namespace = editor)]
#[serde(deny_unknown_fields)]
pub struct SelectToBeginningOfLine {
    #[serde(default)]
    pub stop_at_soft_wraps: bool,
    #[serde(default)]
    pub stop_at_indent: bool,
}

/// Delete from the cursor to the beginning of the current line.
#[derive(PartialEq, Clone, Deserialize, Default, JsonSchema, Action)]
#[action(namespace = editor)]
#[serde(deny_unknown_fields)]
pub struct DeleteToBeginningOfLine {
    #[serde(default)]
    pub stop_at_indent: bool,
}

/// Move the cursor to the end of the current line.
#[derive(PartialEq, Clone, Deserialize, Default, JsonSchema, Action)]
#[action(namespace = editor)]
#[serde(deny_unknown_fields)]
pub struct MoveToEndOfLine {
    #[serde(default = "default_true")]
    pub stop_at_soft_wraps: bool,
}

/// Select from the cursor to the end of the current line.
#[derive(PartialEq, Clone, Deserialize, Default, JsonSchema, Action)]
#[action(namespace = editor)]
#[serde(deny_unknown_fields)]
pub struct SelectToEndOfLine {
    #[serde(default)]
    pub stop_at_soft_wraps: bool,
}

/// Handle text input in the editor.
#[derive(PartialEq, Clone, Deserialize, Default, JsonSchema, Action)]
#[action(namespace = editor)]
pub struct HandleInput(pub String);

/// Delete from the cursor to the end of the next word.
/// Stop before the end of the next word if whitespace sequences of length >= 2 are encountered.
#[derive(PartialEq, Clone, Deserialize, Default, JsonSchema, Action)]
#[action(namespace = editor)]
#[serde(deny_unknown_fields)]
pub struct DeleteToNextWordEnd {
    #[serde(default)]
    pub ignore_newlines: bool,
    // Whether to stop before the end of the next word, if language-defined bracket is encountered.
    #[serde(default)]
    pub ignore_brackets: bool,
}

/// Delete from the cursor to the start of the previous word.
/// Stop before the start of the previous word if whitespace sequences of length >= 2 are encountered.
#[derive(PartialEq, Clone, Deserialize, Default, JsonSchema, Action)]
#[action(namespace = editor)]
#[serde(deny_unknown_fields)]
pub struct DeleteToPreviousWordStart {
    #[serde(default)]
    pub ignore_newlines: bool,
    // Whether to stop before the start of the previous word, if language-defined bracket is encountered.
    #[serde(default)]
    pub ignore_brackets: bool,
}

/// Delete from the cursor to the end of the next subword.
/// Stop before the end of the next subword if whitespace sequences of length >= 2 are encountered.
#[derive(PartialEq, Clone, Deserialize, Default, JsonSchema, Action)]
#[action(namespace = editor)]
#[serde(deny_unknown_fields)]
pub struct DeleteToNextSubwordEnd {
    #[serde(default)]
    pub ignore_newlines: bool,
    // Whether to stop before the start of the previous word, if language-defined bracket is encountered.
    #[serde(default)]
    pub ignore_brackets: bool,
}

/// Delete from the cursor to the start of the previous subword.
/// Stop before the start of the previous subword if whitespace sequences of length >= 2 are encountered.
#[derive(PartialEq, Clone, Deserialize, Default, JsonSchema, Action)]
#[action(namespace = editor)]
#[serde(deny_unknown_fields)]
pub struct DeleteToPreviousSubwordStart {
    #[serde(default)]
    pub ignore_newlines: bool,
    // Whether to stop before the start of the previous word, if language-defined bracket is encountered.
    #[serde(default)]
    pub ignore_brackets: bool,
}

gpui::actions!(
    editor,
    [
        /// Delete the character before the cursor.
        Backspace,
        /// Copy selected text to the clipboard.
        Copy,
        /// Cut selected text to the clipboard.
        Cut,
        /// Delete the character after the cursor.
        Delete,
        /// Delete from the cursor to the end of the line.
        DeleteToEndOfLine,
        /// Insert a new line and move the cursor to it.
        Newline,
        /// Move the cursor left.
        MoveLeft,
        /// Move the cursor right.
        MoveRight,
        /// Move the cursor up.
        MoveUp,
        /// Move the cursor down.
        MoveDown,
        /// Move the cursor to the beginning of the document.
        MoveToBeginning,
        /// Move the cursor to the end of the document.
        MoveToEnd,
        /// Move the cursor to the end of the next subword.
        MoveToNextSubwordEnd,
        /// Move the cursor to the end of the next word.
        MoveToNextWordEnd,
        /// Move the cursor to the start of the previous subword.
        MoveToPreviousSubwordStart,
        /// Move the cursor to the start of the previous word.
        MoveToPreviousWordStart,
        /// Paste from the clipboard.
        Paste,
        /// Redo the last undone edit.
        Redo,
        /// Redo the last selection change.
        RedoSelection,
        /// Select all text.
        SelectAll,
        /// Extend selection left.
        SelectLeft,
        /// Extend selection right.
        SelectRight,
        /// Extend selection up.
        SelectUp,
        /// Extend selection down.
        SelectDown,
        /// Extend selection to the beginning of the document.
        SelectToBeginning,
        /// Extend selection to the end of the document.
        SelectToEnd,
        /// Extend selection to the end of the next subword.
        SelectToNextSubwordEnd,
        /// Extend selection to the end of the next word.
        SelectToNextWordEnd,
        /// Extend selection to the start of the previous subword.
        SelectToPreviousSubwordStart,
        /// Extend selection to the start of the previous word.
        SelectToPreviousWordStart,
        /// Undo the last edit.
        Undo,
        /// Undo the last selection change.
        UndoSelection,
    ]
);
