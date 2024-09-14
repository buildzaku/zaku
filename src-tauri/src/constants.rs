use std::fmt;

pub enum ZakuStoreKey {
    ActiveSpaceReference,
    SpaceReferences,
}

impl fmt::Display for ZakuStoreKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            ZakuStoreKey::ActiveSpaceReference => "active_space_reference",
            ZakuStoreKey::SpaceReferences => "space_references",
        };

        return write!(formatter, "{}", value);
    }
}
