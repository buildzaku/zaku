use std::fmt;
use text::{Bias, Point, PointUtf16};

use crate::{
    ExcerptId, MultiBufferOffset, MultiBufferOffsetUtf16, MultiBufferSnapshot, ToOffset, ToPoint,
};

/// A stable reference to a position within a [`MultiBuffer`].
///
/// Unlike simple offsets, anchors remain valid as the text is edited, automatically
/// adjusting to reflect insertions and deletions around them.
#[derive(Clone, Copy, Eq, PartialEq, Hash)]
pub struct Anchor {
    /// Identifies which excerpt within the multi-buffer this anchor belongs to.
    pub excerpt_id: ExcerptId,
    /// The position within the excerpt's underlying buffer.
    pub text_anchor: text::Anchor,
}

impl fmt::Debug for Anchor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_min() {
            return write!(f, "Anchor::min({:?})", self.text_anchor.buffer_id);
        }
        if self.is_max() {
            return write!(f, "Anchor::max({:?})", self.text_anchor.buffer_id);
        }

        f.debug_struct("Anchor")
            .field("excerpt_id", &self.excerpt_id)
            .field("text_anchor", &self.text_anchor)
            .finish()
    }
}

impl Anchor {
    pub fn min() -> Self {
        Self {
            excerpt_id: ExcerptId::min(),
            text_anchor: text::Anchor::MIN,
        }
    }

    pub fn max() -> Self {
        Self {
            excerpt_id: ExcerptId::max(),
            text_anchor: text::Anchor::MAX,
        }
    }

    pub fn is_min(&self) -> bool {
        self.excerpt_id == ExcerptId::min() && self.text_anchor.is_min()
    }

    pub fn is_max(&self) -> bool {
        self.excerpt_id == ExcerptId::max() && self.text_anchor.is_max()
    }

    pub fn bias(&self) -> Bias {
        self.text_anchor.bias
    }
}

impl ToOffset for Anchor {
    fn to_offset(&self, snapshot: &MultiBufferSnapshot) -> MultiBufferOffset {
        snapshot.offset_for_anchor(*self)
    }

    fn to_offset_utf16(&self, snapshot: &MultiBufferSnapshot) -> MultiBufferOffsetUtf16 {
        snapshot.offset_to_offset_utf16(snapshot.offset_for_anchor(*self))
    }
}

impl ToPoint for Anchor {
    fn to_point(&self, snapshot: &MultiBufferSnapshot) -> Point {
        snapshot.point_for_anchor(*self)
    }

    fn to_point_utf16(&self, snapshot: &MultiBufferSnapshot) -> PointUtf16 {
        snapshot.point_to_point_utf16(snapshot.point_for_anchor(*self))
    }
}
