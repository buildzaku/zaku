use std::fmt;

pub enum ZakuStoreKey {
    ActiveWorkspacePath,
}

impl fmt::Display for ZakuStoreKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            ZakuStoreKey::ActiveWorkspacePath => "active_workspace_path",
        };
        write!(f, "{}", value)
    }
}

pub enum ZakuEvent {
    SynchronizeActiveWorkspace,
}

impl fmt::Display for ZakuEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            ZakuEvent::SynchronizeActiveWorkspace => "synchronize_active_workspace",
        };
        write!(f, "{}", value)
    }
}
