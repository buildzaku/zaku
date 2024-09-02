use std::fmt;

pub enum ZakuStoreKey {
    ActiveSpace,
    SpaceReferences,
}

impl fmt::Display for ZakuStoreKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            ZakuStoreKey::ActiveSpace => "active_space",
            ZakuStoreKey::SpaceReferences => "space_references",
        };

        return write!(formatter, "{}", value);
    }
}
