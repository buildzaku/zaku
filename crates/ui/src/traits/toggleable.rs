/// For elements with two visually distinct states, like checkboxes or switches.
pub trait Toggleable {
    fn toggle_state(self, selected: bool) -> Self;
}

#[derive(Debug, Default, PartialEq, Eq, Hash, Clone, Copy)]
pub enum ToggleState {
    #[default]
    Unselected,
    Indeterminate,
    Selected,
}

impl ToggleState {
    pub fn inverse(&self) -> Self {
        match self {
            Self::Unselected | Self::Indeterminate => Self::Selected,
            Self::Selected => Self::Unselected,
        }
    }

    /// Creates a `ToggleState` from the given `any_checked` and `all_checked` flags.
    pub fn from_any_and_all(any_checked: bool, all_checked: bool) -> Self {
        match (any_checked, all_checked) {
            (true, true) => Self::Selected,
            (false, false) => Self::Unselected,
            _ => Self::Indeterminate,
        }
    }

    pub fn selected(&self) -> bool {
        match self {
            ToggleState::Indeterminate | ToggleState::Unselected => false,
            ToggleState::Selected => true,
        }
    }
}

impl From<bool> for ToggleState {
    fn from(selected: bool) -> Self {
        if selected {
            Self::Selected
        } else {
            Self::Unselected
        }
    }
}

impl From<Option<bool>> for ToggleState {
    fn from(selected: Option<bool>) -> Self {
        match selected {
            Some(true) => Self::Selected,
            Some(false) => Self::Unselected,
            None => Self::Indeterminate,
        }
    }
}
