gpui::actions!(
    project_panel,
    [
        /// Expand the selected entry in the project tree.
        ExpandSelectedEntry,
        /// Collapse the selected entry in the project tree.
        CollapseSelectedEntry,
        /// Collapse the selected entry and its children in the project tree.
        CollapseSelectedEntryAndChildren,
        /// Collapse all entries in the project tree.
        CollapseAllEntries,
        /// Create a new directory.
        NewDirectory,
        /// Create a new file.
        NewFile,
        /// Open the selected entry.
        Open,
        /// Reveal the selected item in the system file manager.
        RevealInFileManager,
        /// Toggle focus on the project panel.
        ToggleFocus
    ]
);
