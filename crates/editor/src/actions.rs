use gpui::{Action, actions};
use schemars::JsonSchema;
use serde::Deserialize;

fn default_true() -> bool {
    true
}

/// Moves the cursor to the beginning of the current line.
#[derive(PartialEq, Clone, Deserialize, Default, JsonSchema, Action)]
#[action(namespace = editor)]
#[serde(deny_unknown_fields)]
pub struct MoveToBeginningOfLine {
    #[serde(default = "default_true")]
    pub stop_at_soft_wraps: bool,
    #[serde(default)]
    pub stop_at_indent: bool,
}

/// Selects from the cursor to the beginning of the current line.
#[derive(PartialEq, Clone, Deserialize, Default, JsonSchema, Action)]
#[action(namespace = editor)]
#[serde(deny_unknown_fields)]
pub struct SelectToBeginningOfLine {
    #[serde(default)]
    pub(super) stop_at_soft_wraps: bool,
    #[serde(default)]
    pub stop_at_indent: bool,
}

/// Deletes from the cursor to the beginning of the current line.
#[derive(PartialEq, Clone, Deserialize, Default, JsonSchema, Action)]
#[action(namespace = editor)]
#[serde(deny_unknown_fields)]
pub struct DeleteToBeginningOfLine {
    #[serde(default)]
    pub(super) stop_at_indent: bool,
}

/// Moves the cursor to the end of the current line.
#[derive(PartialEq, Clone, Deserialize, Default, JsonSchema, Action)]
#[action(namespace = editor)]
#[serde(deny_unknown_fields)]
pub struct MoveToEndOfLine {
    #[serde(default = "default_true")]
    pub stop_at_soft_wraps: bool,
}

/// Selects from the cursor to the end of the current line.
#[derive(PartialEq, Clone, Deserialize, Default, JsonSchema, Action)]
#[action(namespace = editor)]
#[serde(deny_unknown_fields)]
pub struct SelectToEndOfLine {
    #[serde(default)]
    pub(super) stop_at_soft_wraps: bool,
}

/// Handles text input in the editor.
#[derive(PartialEq, Clone, Deserialize, Default, JsonSchema, Action)]
#[action(namespace = editor)]
pub struct HandleInput(pub String);

/// Deletes from the cursor to the end of the next word.
/// Stops before the end of the next word, if whitespace sequences of length >= 2 are encountered.
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

/// Deletes from the cursor to the start of the previous word.
/// Stops before the start of the previous word, if whitespace sequences of length >= 2 are encountered.
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

/// Deletes from the cursor to the end of the next subword.
/// Stops before the end of the next subword, if whitespace sequences of length >= 2 are encountered.
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

/// Deletes from the cursor to the start of the previous subword.
/// Stops before the start of the previous subword, if whitespace sequences of length >= 2 are encountered.
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

actions!(
    editor,
    [
        /// Deletes the character before the cursor.
        Backspace,
        /// Copies selected text to the clipboard.
        Copy,
        /// Cuts selected text to the clipboard.
        Cut,
        /// Deletes the character after the cursor.
        Delete,
        /// Deletes from cursor to end of line.
        DeleteToEndOfLine,
        /// Inserts a new line and moves cursor to it.
        Newline,
        /// Moves cursor left.
        MoveLeft,
        /// Moves cursor right.
        MoveRight,
        /// Moves cursor up.
        MoveUp,
        /// Moves cursor down.
        MoveDown,
        /// Moves cursor to the beginning of the document.
        MoveToBeginning,
        /// Moves cursor to the end of the document.
        MoveToEnd,
        /// Moves cursor to the end of the next subword.
        MoveToNextSubwordEnd,
        /// Moves cursor to the end of the next word.
        MoveToNextWordEnd,
        /// Moves cursor to the start of the previous subword.
        MoveToPreviousSubwordStart,
        /// Moves cursor to the start of the previous word.
        MoveToPreviousWordStart,
        /// Pastes from clipboard.
        Paste,
        /// Redoes the last undone edit.
        Redo,
        /// Redoes the last selection change.
        RedoSelection,
        /// Selects all text.
        SelectAll,
        /// Selects to the left.
        SelectLeft,
        /// Selects to the right.
        SelectRight,
        /// Selects up.
        SelectUp,
        /// Selects down.
        SelectDown,
        /// Selects from cursor to the beginning of the document.
        SelectToBeginning,
        /// Selects from cursor to the end of the document.
        SelectToEnd,
        /// Selects to the end of the next subword.
        SelectToNextSubwordEnd,
        /// Selects to the end of the next word.
        SelectToNextWordEnd,
        /// Selects to the start of the previous subword.
        SelectToPreviousSubwordStart,
        /// Selects to the start of the previous word.
        SelectToPreviousWordStart,
        /// Undoes the last edit.
        Undo,
        /// Undoes the last selection change.
        UndoSelection,
    ]
);
