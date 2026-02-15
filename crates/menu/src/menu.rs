use gpui::{App, KeyBinding, actions};

const KEY_CONTEXT: &str = "menu";

pub fn init(cx: &mut App) {
    cx.bind_keys([
        // Navigation
        KeyBinding::new("home", SelectFirst, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-pageup", SelectFirst, Some(KEY_CONTEXT)),
        KeyBinding::new("pageup", SelectFirst, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-up", SelectFirst, Some(KEY_CONTEXT)),
        KeyBinding::new("end", SelectLast, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-pagedown", SelectLast, Some(KEY_CONTEXT)),
        KeyBinding::new("pagedown", SelectLast, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-down", SelectLast, Some(KEY_CONTEXT)),
        KeyBinding::new("tab", SelectNext, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-n", SelectNext, Some(KEY_CONTEXT)),
        KeyBinding::new("down", SelectNext, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-tab", SelectPrevious, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-p", SelectPrevious, Some(KEY_CONTEXT)),
        KeyBinding::new("up", SelectPrevious, Some(KEY_CONTEXT)),
        // Confirm / Cancel
        KeyBinding::new("enter", Confirm, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-enter", SecondaryConfirm, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-enter", SecondaryConfirm, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-c", Cancel, Some(KEY_CONTEXT)),
        KeyBinding::new("escape", Cancel, Some(KEY_CONTEXT)),
        #[cfg(not(target_os = "windows"))]
        KeyBinding::new("ctrl-escape", Cancel, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-escape", Cancel, Some(KEY_CONTEXT)),
        #[cfg(not(target_os = "windows"))]
        KeyBinding::new("alt-shift-enter", Restart, Some(KEY_CONTEXT)),
        #[cfg(target_os = "windows")]
        KeyBinding::new("shift-alt-enter", Restart, Some(KEY_CONTEXT)),
    ]);
}

actions!(
    menu,
    [
        /// Cancels the current menu operation.
        Cancel,
        /// Confirms the selected menu item.
        Confirm,
        /// Performs secondary confirmation action.
        SecondaryConfirm,
        /// Selects the previous item in the menu.
        SelectPrevious,
        /// Selects the next item in the menu.
        SelectNext,
        /// Selects the first item in the menu.
        SelectFirst,
        /// Selects the last item in the menu.
        SelectLast,
        /// Restarts the menu from the beginning.
        Restart,
    ]
);
