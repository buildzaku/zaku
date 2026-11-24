gpui::actions!(
    branch_picker,
    [
        /// Delete the selected Git branch. For a remote-tracking branch, this only removes the local tracking ref.
        DeleteBranch,
        /// Force delete the selected Git branch. For a remote-tracking branch, this only removes the local tracking ref.
        ForceDeleteBranch,
        /// Show all branches.
        ShowAllBranches,
        /// Show only local branches.
        ShowLocalBranches,
        /// Show only remote branches.
        ShowRemoteBranches,
        /// Cycle through branch filters.
        CycleBranchFilter,
        /// Toggle the branch filter menu.
        ToggleFilterMenu,
    ]
);
