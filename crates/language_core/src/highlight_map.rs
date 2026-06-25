use std::{num::NonZeroU32, sync::Arc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighlightId(NonZeroU32);

impl HighlightId {
    pub const TABSTOP_INSERT_ID: HighlightId = HighlightId(NonZeroU32::new(u32::MAX - 1).unwrap());
    pub const TABSTOP_REPLACE_ID: HighlightId = HighlightId(NonZeroU32::new(u32::MAX - 2).unwrap());

    pub fn new(capture_id: u32) -> Self {
        let value = capture_id
            .checked_add(1)
            .expect("highlight capture id should fit in non-zero u32");

        Self(NonZeroU32::new(value).expect("highlight capture id should not be zero"))
    }
}

impl From<HighlightId> for usize {
    fn from(value: HighlightId) -> Self {
        value.0.get() as usize - 1
    }
}

#[derive(Debug, Clone)]
pub struct HighlightMap(Arc<[Option<HighlightId>]>);

impl HighlightMap {
    #[inline]
    pub fn from_ids(highlight_ids: impl IntoIterator<Item = Option<HighlightId>>) -> Self {
        Self(highlight_ids.into_iter().collect())
    }

    #[inline]
    pub fn get(&self, capture_id: u32) -> Option<HighlightId> {
        self.0.get(capture_id as usize).copied().flatten()
    }
}

impl Default for HighlightMap {
    fn default() -> Self {
        Self(Arc::new([]))
    }
}
