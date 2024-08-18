use std::fmt;

pub enum ZakuStoreKey {
    ActiveSpacePath,
}

impl fmt::Display for ZakuStoreKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            ZakuStoreKey::ActiveSpacePath => "active_space_path",
        };
        write!(f, "{}", value)
    }
}

pub enum ZakuEvent {
    SynchronizeActiveSpace,
}

impl fmt::Display for ZakuEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            ZakuEvent::SynchronizeActiveSpace => "synchronize_active_space",
        };
        write!(f, "{}", value)
    }
}
