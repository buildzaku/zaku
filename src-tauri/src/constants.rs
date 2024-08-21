use std::fmt;

pub enum ZakuStoreKey {
    ActiveSpace,
    SavedSpaces,
}

impl fmt::Display for ZakuStoreKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            ZakuStoreKey::ActiveSpace => "active_space",
            ZakuStoreKey::SavedSpaces => "saved_spaces",
        };
        write!(f, "{}", value)
    }
}
