gpui::actions!(
    menu,
    [
        /// Cancel the current menu operation.
        Cancel,
        /// Confirm the selected menu item.
        Confirm,
        /// Perform the secondary confirmation action.
        SecondaryConfirm,
        /// Select the previous item in the menu.
        SelectPrevious,
        /// Select the next item in the menu.
        SelectNext,
        /// Select the first item in the menu.
        SelectFirst,
        /// Select the last item in the menu.
        SelectLast,
        /// Restart the menu from the beginning.
        Restart,
    ]
);
