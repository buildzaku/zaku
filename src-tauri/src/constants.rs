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
