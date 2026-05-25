use anyhow::anyhow;
use std::{fmt, num::NonZeroU64};

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, PartialOrd, Ord, Eq)]
pub struct BufferId(NonZeroU64);

impl fmt::Display for BufferId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<NonZeroU64> for BufferId {
    fn from(id: NonZeroU64) -> Self {
        Self(id)
    }
}

impl BufferId {
    pub fn new(id: u64) -> anyhow::Result<Self> {
        let id = NonZeroU64::new(id).ok_or_else(|| anyhow!("Buffer id cannot be 0."))?;
        Ok(Self(id))
    }
}
