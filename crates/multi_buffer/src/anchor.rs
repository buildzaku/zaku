use std::{
    cmp::Ordering,
    fmt,
    ops::{Add, AddAssign, Sub},
};
use text::{Bias, BufferId, BufferSnapshot, Point, PointUtf16};

use crate::{
    MultiBufferDimension, MultiBufferOffset, MultiBufferOffsetUtf16, MultiBufferSnapshot, ToOffset,
    ToPoint,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ExcerptAnchor {
    pub(crate) text_anchor: text::Anchor,
}

/// A stable reference to a position within a [`MultiBuffer`].
///
/// Unlike simple offsets, anchors remain valid as the text is edited, automatically
/// adjusting to reflect insertions and deletions around them.
#[derive(Clone, Copy, Eq, PartialEq, Hash)]
pub enum Anchor {
    Min,
    Excerpt(ExcerptAnchor),
    Max,
}

impl fmt::Debug for Anchor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Anchor::Min => write!(f, "Anchor::Min"),
            Anchor::Max => write!(f, "Anchor::Max"),
            Anchor::Excerpt(excerpt_anchor) => write!(f, "{excerpt_anchor:?}"),
        }
    }
}

impl From<ExcerptAnchor> for Anchor {
    fn from(anchor: ExcerptAnchor) -> Self {
        Anchor::Excerpt(anchor)
    }
}

impl ExcerptAnchor {
    pub(crate) fn buffer_id(&self) -> BufferId {
        self.text_anchor.buffer_id
    }

    pub(crate) fn text_anchor(&self) -> text::Anchor {
        self.text_anchor
    }

    pub(crate) fn cmp(&self, other: &Self, snapshot: &MultiBufferSnapshot) -> Ordering {
        if self.buffer_id() != other.buffer_id() {
            return self.buffer_id().cmp(&other.buffer_id());
        }

        self.text_anchor()
            .cmp(&other.text_anchor(), &snapshot.buffer)
    }

    fn bias_left(&self, snapshot: &MultiBufferSnapshot) -> Self {
        if self.text_anchor.bias == Bias::Left {
            return *self;
        }

        let text_anchor = self.text_anchor().bias_left(&snapshot.buffer);
        Self::in_buffer(text_anchor)
    }

    fn bias_right(&self, snapshot: &MultiBufferSnapshot) -> Self {
        if self.text_anchor.bias == Bias::Right {
            return *self;
        }

        let text_anchor = self.text_anchor().bias_right(&snapshot.buffer);
        Self::in_buffer(text_anchor)
    }

    #[track_caller]
    pub(crate) fn in_buffer(text_anchor: text::Anchor) -> Self {
        Self { text_anchor }
    }

    fn is_valid(&self, snapshot: &MultiBufferSnapshot) -> bool {
        self.buffer_id() == snapshot.buffer.remote_id()
            && self.text_anchor.is_valid(&snapshot.buffer)
    }
}

impl ToOffset for ExcerptAnchor {
    fn to_offset(&self, snapshot: &MultiBufferSnapshot) -> MultiBufferOffset {
        Anchor::from(*self).to_offset(snapshot)
    }

    fn to_offset_utf16(&self, snapshot: &MultiBufferSnapshot) -> MultiBufferOffsetUtf16 {
        Anchor::from(*self).to_offset_utf16(snapshot)
    }
}

impl ToPoint for ExcerptAnchor {
    fn to_point(&self, snapshot: &MultiBufferSnapshot) -> Point {
        Anchor::from(*self).to_point(snapshot)
    }

    fn to_point_utf16(&self, snapshot: &MultiBufferSnapshot) -> PointUtf16 {
        Anchor::from(*self).to_point_utf16(snapshot)
    }
}

impl Anchor {
    pub fn is_min(&self) -> bool {
        matches!(self, Self::Min)
    }

    pub fn is_max(&self) -> bool {
        matches!(self, Self::Max)
    }

    pub(crate) fn in_buffer(text_anchor: text::Anchor) -> Self {
        Self::Excerpt(ExcerptAnchor::in_buffer(text_anchor))
    }

    pub fn cmp(&self, other: &Anchor, snapshot: &MultiBufferSnapshot) -> Ordering {
        match (self, other) {
            (Anchor::Min, Anchor::Min) => Ordering::Equal,
            (Anchor::Max, Anchor::Max) => Ordering::Equal,
            (Anchor::Min, _) => Ordering::Less,
            (Anchor::Max, _) => Ordering::Greater,
            (_, Anchor::Max) => Ordering::Less,
            (_, Anchor::Min) => Ordering::Greater,
            (Anchor::Excerpt(self_excerpt_anchor), Anchor::Excerpt(other_excerpt_anchor)) => {
                self_excerpt_anchor.cmp(other_excerpt_anchor, snapshot)
            }
        }
    }

    pub fn bias(&self) -> Bias {
        match self {
            Anchor::Min => Bias::Left,
            Anchor::Max => Bias::Right,
            Anchor::Excerpt(excerpt_anchor) => excerpt_anchor.text_anchor().bias,
        }
    }

    pub fn bias_left(&self, snapshot: &MultiBufferSnapshot) -> Anchor {
        match self {
            Anchor::Min => *self,
            Anchor::Max => snapshot.anchor_before(snapshot.max_point()),
            Anchor::Excerpt(excerpt_anchor) => Anchor::Excerpt(excerpt_anchor.bias_left(snapshot)),
        }
    }

    pub fn bias_right(&self, snapshot: &MultiBufferSnapshot) -> Anchor {
        match self {
            Anchor::Max => *self,
            Anchor::Min => snapshot.anchor_after(Point::zero()),
            Anchor::Excerpt(excerpt_anchor) => Anchor::Excerpt(excerpt_anchor.bias_right(snapshot)),
        }
    }

    pub fn summary<D>(&self, snapshot: &MultiBufferSnapshot) -> D
    where
        D: MultiBufferDimension
            + Ord
            + Sub<Output = D::TextDimension>
            + Sub<D::TextDimension, Output = D>
            + AddAssign<D::TextDimension>
            + Add<D::TextDimension, Output = D>,
        D::TextDimension: Sub<Output = D::TextDimension> + Ord,
    {
        snapshot.summary_for_anchor(self)
    }

    pub fn is_valid(&self, snapshot: &MultiBufferSnapshot) -> bool {
        match self {
            Anchor::Min | Anchor::Max => true,
            Anchor::Excerpt(excerpt_anchor) => excerpt_anchor.is_valid(snapshot),
        }
    }

    pub(crate) fn excerpt_anchor(&self) -> Option<ExcerptAnchor> {
        match self {
            Anchor::Min | Anchor::Max => None,
            Anchor::Excerpt(excerpt_anchor) => Some(*excerpt_anchor),
        }
    }

    pub(crate) fn text_anchor(&self) -> Option<text::Anchor> {
        match self {
            Anchor::Min | Anchor::Max => None,
            Anchor::Excerpt(excerpt_anchor) => Some(excerpt_anchor.text_anchor()),
        }
    }

    pub fn opaque_id(&self) -> Option<[u8; 20]> {
        self.text_anchor().map(|anchor| anchor.opaque_id())
    }

    pub fn raw_text_anchor(&self) -> Option<text::Anchor> {
        match self {
            Anchor::Min | Anchor::Max => None,
            Anchor::Excerpt(excerpt_anchor) => Some(excerpt_anchor.text_anchor),
        }
    }

    pub fn text_anchor_in(&self, buffer: &BufferSnapshot) -> text::Anchor {
        match self {
            Anchor::Min => text::Anchor::min_for_buffer(buffer.remote_id()),
            Anchor::Excerpt(excerpt_anchor) => {
                let text_anchor = excerpt_anchor.text_anchor;
                assert_eq!(text_anchor.buffer_id, buffer.remote_id());
                text_anchor
            }
            Anchor::Max => text::Anchor::max_for_buffer(buffer.remote_id()),
        }
    }
}

impl ToOffset for Anchor {
    fn to_offset(&self, snapshot: &MultiBufferSnapshot) -> MultiBufferOffset {
        self.summary(snapshot)
    }

    fn to_offset_utf16(&self, snapshot: &MultiBufferSnapshot) -> MultiBufferOffsetUtf16 {
        self.summary(snapshot)
    }
}

impl ToPoint for Anchor {
    fn to_point(&self, snapshot: &MultiBufferSnapshot) -> Point {
        self.summary(snapshot)
    }

    fn to_point_utf16(&self, snapshot: &MultiBufferSnapshot) -> PointUtf16 {
        self.summary(snapshot)
    }
}
