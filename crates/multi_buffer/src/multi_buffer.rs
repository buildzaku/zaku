mod anchor;
mod transaction;

#[cfg(test)]
mod tests;

use gpui::{App, Context, Entity};
use std::{
    cell::{Ref, RefCell},
    cmp, fmt, mem,
    ops::{self, Add, AddAssign, Range, Sub},
    sync::Arc,
};
use text::{
    Bias, Buffer as TextBuffer, BufferSnapshot as TextBufferSnapshot, Edit as TextEdit,
    OffsetUtf16, Point, PointUtf16, TextSummary,
    subscription::{Subscription, Topic},
};

pub use anchor::Anchor;

#[derive(Debug, Default, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExcerptId(u32);

pub type MultiBufferPoint = Point;

#[derive(Copy, Clone, Debug, Default, Eq, Ord, PartialOrd, PartialEq, Hash)]
pub struct MultiBufferRow(pub u32);

impl MultiBufferRow {
    pub const MIN: Self = Self(0);
    pub const MAX: Self = Self(u32::MAX);
}

impl Add<usize> for MultiBufferRow {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        MultiBufferRow(self.0 + rhs as u32)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CharKind {
    Whitespace,
    Punctuation,
    Word,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum CharScopeContext {
    Completion,
    LinkedEdit,
}

#[derive(Clone, Debug, Default)]
pub struct CharClassifier {
    word_chars: Arc<[char]>,
}

impl CharClassifier {
    pub fn new(word_chars: Arc<[char]>) -> Self {
        Self { word_chars }
    }

    pub fn kind(&self, character: char) -> CharKind {
        if self.is_word(character) {
            return CharKind::Word;
        }
        if character.is_whitespace() {
            return CharKind::Whitespace;
        }
        CharKind::Punctuation
    }

    pub fn is_whitespace(&self, character: char) -> bool {
        self.kind(character) == CharKind::Whitespace
    }

    pub fn is_word(&self, character: char) -> bool {
        character == '_' || self.word_chars.contains(&character) || character.is_alphanumeric()
    }

    pub fn is_punctuation(&self, character: char) -> bool {
        self.kind(character) == CharKind::Punctuation
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, Ord, PartialOrd, PartialEq, Hash)]
pub struct MultiBufferOffset(pub usize);

impl MultiBufferOffset {
    pub const ZERO: Self = Self(0);

    pub fn saturating_sub(self, other: MultiBufferOffset) -> usize {
        self.0.saturating_sub(other.0)
    }
}

impl fmt::Display for MultiBufferOffset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl ops::Sub for MultiBufferOffset {
    type Output = usize;

    fn sub(self, other: MultiBufferOffset) -> Self::Output {
        self.0 - other.0
    }
}

impl ops::Sub<usize> for MultiBufferOffset {
    type Output = Self;

    fn sub(self, other: usize) -> Self::Output {
        MultiBufferOffset(self.0 - other)
    }
}

impl ops::SubAssign<usize> for MultiBufferOffset {
    fn sub_assign(&mut self, other: usize) {
        self.0 -= other;
    }
}

impl ops::Add<usize> for MultiBufferOffset {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        MultiBufferOffset(self.0 + rhs)
    }
}

impl ops::AddAssign<usize> for MultiBufferOffset {
    fn add_assign(&mut self, other: usize) {
        self.0 += other;
    }
}

impl ops::Add<isize> for MultiBufferOffset {
    type Output = Self;

    fn add(self, rhs: isize) -> Self::Output {
        MultiBufferOffset((self.0 as isize + rhs) as usize)
    }
}

impl ops::Add for MultiBufferOffset {
    type Output = Self;

    fn add(self, rhs: MultiBufferOffset) -> Self::Output {
        MultiBufferOffset(self.0 + rhs.0)
    }
}

impl ops::AddAssign<MultiBufferOffset> for MultiBufferOffset {
    fn add_assign(&mut self, other: MultiBufferOffset) {
        self.0 += other.0;
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, Ord, PartialOrd, PartialEq)]
pub struct MultiBufferOffsetUtf16(pub OffsetUtf16);

impl ops::Add<usize> for MultiBufferOffsetUtf16 {
    type Output = MultiBufferOffsetUtf16;

    fn add(self, rhs: usize) -> Self::Output {
        MultiBufferOffsetUtf16(OffsetUtf16(self.0.0 + rhs))
    }
}

impl ops::Add<OffsetUtf16> for MultiBufferOffsetUtf16 {
    type Output = Self;

    fn add(self, rhs: OffsetUtf16) -> Self::Output {
        MultiBufferOffsetUtf16(self.0 + rhs)
    }
}

impl AddAssign<OffsetUtf16> for MultiBufferOffsetUtf16 {
    fn add_assign(&mut self, rhs: OffsetUtf16) {
        self.0 += rhs;
    }
}

impl AddAssign<usize> for MultiBufferOffsetUtf16 {
    fn add_assign(&mut self, rhs: usize) {
        self.0.0 += rhs;
    }
}

impl Sub for MultiBufferOffsetUtf16 {
    type Output = OffsetUtf16;

    fn sub(self, other: MultiBufferOffsetUtf16) -> Self::Output {
        self.0 - other.0
    }
}

impl Sub<OffsetUtf16> for MultiBufferOffsetUtf16 {
    type Output = MultiBufferOffsetUtf16;

    fn sub(self, other: OffsetUtf16) -> Self::Output {
        MultiBufferOffsetUtf16(self.0 - other)
    }
}

pub trait MultiBufferDimension: 'static + Copy + Default + fmt::Debug {
    type TextDimension;
    fn from_summary(summary: &MBTextSummary) -> Self;
    fn add_text_dim(&mut self, summary: &Self::TextDimension);
    fn add_mb_text_summary(&mut self, summary: &MBTextSummary);
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct MBTextSummary {
    pub len: MultiBufferOffset,
    pub len_utf16: OffsetUtf16,
    pub lines: Point,
    pub last_line_len_utf16: u32,
}

impl MBTextSummary {
    pub fn lines_utf16(&self) -> PointUtf16 {
        PointUtf16 {
            row: self.lines.row,
            column: self.last_line_len_utf16,
        }
    }
}

impl MultiBufferDimension for Point {
    type TextDimension = Point;

    fn from_summary(summary: &MBTextSummary) -> Self {
        summary.lines
    }

    fn add_text_dim(&mut self, summary: &Self::TextDimension) {
        *self += *summary;
    }

    fn add_mb_text_summary(&mut self, summary: &MBTextSummary) {
        *self += summary.lines;
    }
}

impl MultiBufferDimension for PointUtf16 {
    type TextDimension = PointUtf16;

    fn from_summary(summary: &MBTextSummary) -> Self {
        summary.lines_utf16()
    }

    fn add_text_dim(&mut self, summary: &Self::TextDimension) {
        *self += *summary;
    }

    fn add_mb_text_summary(&mut self, summary: &MBTextSummary) {
        *self += summary.lines_utf16();
    }
}

impl MultiBufferDimension for MultiBufferOffset {
    type TextDimension = usize;

    fn from_summary(summary: &MBTextSummary) -> Self {
        summary.len
    }

    fn add_text_dim(&mut self, summary: &Self::TextDimension) {
        self.0 += *summary;
    }

    fn add_mb_text_summary(&mut self, summary: &MBTextSummary) {
        *self += summary.len;
    }
}

impl MultiBufferDimension for MultiBufferOffsetUtf16 {
    type TextDimension = OffsetUtf16;

    fn from_summary(summary: &MBTextSummary) -> Self {
        MultiBufferOffsetUtf16(summary.len_utf16)
    }

    fn add_text_dim(&mut self, summary: &Self::TextDimension) {
        self.0 += *summary;
    }

    fn add_mb_text_summary(&mut self, summary: &MBTextSummary) {
        self.0 += summary.len_utf16;
    }
}

pub trait ToOffset: 'static + fmt::Debug {
    fn to_offset(&self, snapshot: &MultiBufferSnapshot) -> MultiBufferOffset;
    fn to_offset_utf16(&self, snapshot: &MultiBufferSnapshot) -> MultiBufferOffsetUtf16;
}

pub trait ToPoint: 'static + fmt::Debug {
    fn to_point(&self, snapshot: &MultiBufferSnapshot) -> Point;
    fn to_point_utf16(&self, snapshot: &MultiBufferSnapshot) -> PointUtf16;
}

#[derive(Clone)]
pub struct MultiBufferSnapshot {
    buffer: TextBufferSnapshot,
    excerpt_id: ExcerptId,
    edit_count: usize,
}

pub struct MultiBuffer {
    snapshot: RefCell<MultiBufferSnapshot>,
    buffer: Entity<TextBuffer>,
    subscriptions: Topic<MultiBufferOffset>,
    singleton: bool,
}

pub struct MultiBufferChunk<'a> {
    pub text: &'a str,
}

pub struct MultiBufferChunks<'a> {
    text_chunks: text::Chunks<'a>,
}

impl<'a> Iterator for MultiBufferChunks<'a> {
    type Item = MultiBufferChunk<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.text_chunks
            .next()
            .map(|text| MultiBufferChunk { text })
    }
}

impl MultiBuffer {
    pub fn singleton(buffer: Entity<TextBuffer>, cx: &mut Context<Self>) -> Self {
        let buffer_snapshot = buffer.read(cx).snapshot().clone();
        Self {
            snapshot: RefCell::new(MultiBufferSnapshot {
                buffer: buffer_snapshot,
                excerpt_id: ExcerptId(1),
                edit_count: 0,
            }),
            buffer,
            subscriptions: Topic::default(),
            singleton: true,
        }
    }

    pub fn snapshot(&self, cx: &App) -> MultiBufferSnapshot {
        self.sync(cx);
        self.snapshot.borrow().clone()
    }

    pub fn read(&self, cx: &App) -> Ref<'_, MultiBufferSnapshot> {
        self.sync(cx);
        self.snapshot.borrow()
    }

    pub fn as_singleton(&self) -> Option<Entity<TextBuffer>> {
        if self.singleton {
            Some(self.buffer.clone())
        } else {
            None
        }
    }

    pub fn subscribe(&mut self) -> Subscription<MultiBufferOffset> {
        self.subscriptions.subscribe()
    }

    pub fn len(&self, cx: &App) -> MultiBufferOffset {
        self.read(cx).len()
    }

    pub fn edit<I, S, T>(&mut self, edits: I, cx: &mut Context<Self>)
    where
        I: IntoIterator<Item = (Range<S>, T)>,
        S: ToOffset,
        T: Into<Arc<str>>,
    {
        self.edit_internal(edits, cx);
    }

    pub fn set_text(&mut self, text: impl Into<Arc<str>>, cx: &mut Context<Self>) {
        let range = MultiBufferOffset::ZERO..self.len(cx);
        self.edit([(range, text.into())], cx);
    }

    fn edit_internal<I, S, T>(&mut self, edits_iter: I, cx: &mut Context<Self>)
    where
        I: IntoIterator<Item = (Range<S>, T)>,
        S: ToOffset,
        T: Into<Arc<str>>,
    {
        self.sync_mut(cx);

        let snapshot = self.snapshot.get_mut();
        let mut edits = edits_iter
            .into_iter()
            .map(|(range, new_text)| {
                let mut range = range.start.to_offset(snapshot)..range.end.to_offset(snapshot);
                if range.start > range.end {
                    mem::swap(&mut range.start, &mut range.end);
                }
                (range, new_text.into())
            })
            .collect::<Vec<_>>();
        let _ = snapshot;

        edits.sort_by_key(|(range, _)| range.start);

        let mut normalized_edits: Vec<(Range<MultiBufferOffset>, Arc<str>)> = Vec::new();
        for (range, new_text) in edits {
            if new_text.is_empty() && range.is_empty() {
                continue;
            }

            let previous_edit = normalized_edits.last_mut();
            let should_coalesce = previous_edit
                .as_ref()
                .is_some_and(|(previous_range, _)| previous_range.end >= range.start);

            if let Some((previous_range, previous_text)) = previous_edit
                && should_coalesce
            {
                previous_range.end = cmp::max(previous_range.end, range.end);
                *previous_text = format!("{previous_text}{new_text}").into();
            } else {
                normalized_edits.push((range, new_text));
            }
        }

        if normalized_edits.is_empty() {
            return;
        }

        let mut buffer_edits = normalized_edits
            .into_iter()
            .map(|(range, new_text)| (range.start.0..range.end.0, new_text))
            .collect::<Vec<_>>();
        buffer_edits.sort_by_key(|(range, _)| range.start);

        self.buffer.update(cx, |buffer, _| {
            buffer.edit(
                buffer_edits
                    .iter()
                    .map(|(range, new_text)| (range.clone(), new_text.clone())),
            );
        });

        self.sync_mut(cx);
    }

    fn sync(&self, cx: &App) {
        let buffer_snapshot = self.buffer.read(cx).snapshot().clone();
        let previous_version = {
            let snapshot = self.snapshot.borrow();
            if snapshot.buffer.version() == buffer_snapshot.version() {
                return;
            }
            snapshot.buffer.version().clone()
        };

        let edits = buffer_snapshot
            .edits_since::<usize>(&previous_version)
            .map(|edit| TextEdit {
                old: MultiBufferOffset(edit.old.start)..MultiBufferOffset(edit.old.end),
                new: MultiBufferOffset(edit.new.start)..MultiBufferOffset(edit.new.end),
            })
            .collect::<Vec<_>>();

        {
            let mut snapshot = self.snapshot.borrow_mut();
            snapshot.buffer = buffer_snapshot;
            if !edits.is_empty() {
                snapshot.edit_count = snapshot.edit_count.saturating_add(1);
            }
        }

        if !edits.is_empty() {
            self.subscriptions.publish(edits);
        }
    }

    fn sync_mut(&mut self, cx: &App) {
        let buffer_snapshot = self.buffer.read(cx).snapshot().clone();
        let previous_version = {
            let snapshot = self.snapshot.get_mut();
            if snapshot.buffer.version() == buffer_snapshot.version() {
                return;
            }
            snapshot.buffer.version().clone()
        };

        let edits = buffer_snapshot
            .edits_since::<usize>(&previous_version)
            .map(|edit| TextEdit {
                old: MultiBufferOffset(edit.old.start)..MultiBufferOffset(edit.old.end),
                new: MultiBufferOffset(edit.new.start)..MultiBufferOffset(edit.new.end),
            })
            .collect::<Vec<_>>();

        {
            let snapshot = self.snapshot.get_mut();
            snapshot.buffer = buffer_snapshot;
            if !edits.is_empty() {
                snapshot.edit_count = snapshot.edit_count.saturating_add(1);
            }
        }

        if !edits.is_empty() {
            self.subscriptions.publish(edits);
        }
    }
}

impl MultiBufferSnapshot {
    #[inline]
    pub fn text(&self) -> String {
        self.text_for_range(MultiBufferOffset::ZERO..self.len())
            .collect()
    }

    #[inline]
    pub fn len(&self) -> MultiBufferOffset {
        MultiBufferOffset(self.buffer.len())
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    #[inline]
    pub fn max_point(&self) -> Point {
        self.buffer.max_point()
    }

    pub fn char_classifier_at<T: ToOffset>(&self, point: T) -> CharClassifier {
        let _ = point.to_offset(self);
        CharClassifier::default()
    }

    #[inline]
    pub fn clip_offset(&self, offset: MultiBufferOffset, bias: Bias) -> MultiBufferOffset {
        MultiBufferOffset(self.buffer.clip_offset(offset.0, bias))
    }

    #[inline]
    pub fn clip_offset_utf16(
        &self,
        offset: MultiBufferOffsetUtf16,
        bias: Bias,
    ) -> MultiBufferOffsetUtf16 {
        let offset = self.offset_utf16_to_offset(offset);
        let clipped = self.clip_offset(offset, bias);
        self.offset_to_offset_utf16(clipped)
    }

    #[inline]
    pub fn clip_point(&self, point: Point, bias: Bias) -> Point {
        self.buffer.clip_point(point, bias)
    }

    #[inline]
    pub fn offset_to_point(&self, offset: MultiBufferOffset) -> Point {
        self.buffer.offset_to_point(offset.0)
    }

    #[inline]
    pub fn point_to_offset(&self, point: Point) -> MultiBufferOffset {
        MultiBufferOffset(self.buffer.point_to_offset(point))
    }

    #[inline]
    pub fn offset_to_point_utf16(&self, offset: MultiBufferOffset) -> PointUtf16 {
        self.buffer.offset_to_point_utf16(offset.0)
    }

    #[inline]
    pub fn point_to_offset_utf16(&self, point: Point) -> MultiBufferOffsetUtf16 {
        MultiBufferOffsetUtf16(self.buffer.point_to_offset_utf16(point))
    }

    #[inline]
    pub fn point_utf16_to_offset_utf16(&self, point: PointUtf16) -> MultiBufferOffsetUtf16 {
        MultiBufferOffsetUtf16(self.buffer.point_utf16_to_offset_utf16(point))
    }

    #[inline]
    pub fn point_utf16_to_offset(&self, point: PointUtf16) -> MultiBufferOffset {
        MultiBufferOffset(self.buffer.point_utf16_to_offset(point))
    }

    #[inline]
    pub fn point_to_point_utf16(&self, point: Point) -> PointUtf16 {
        self.buffer.point_to_point_utf16(point)
    }

    #[inline]
    pub fn point_utf16_to_point(&self, point: PointUtf16) -> Point {
        self.buffer.point_utf16_to_point(point)
    }

    #[inline]
    pub fn offset_utf16_to_offset(&self, offset: MultiBufferOffsetUtf16) -> MultiBufferOffset {
        MultiBufferOffset(self.buffer.offset_utf16_to_offset(offset.0))
    }

    #[inline]
    pub fn offset_to_offset_utf16(&self, offset: MultiBufferOffset) -> MultiBufferOffsetUtf16 {
        MultiBufferOffsetUtf16(self.buffer.offset_to_offset_utf16(offset.0))
    }

    pub fn anchor_before<T: ToOffset>(&self, position: T) -> Anchor {
        self.anchor_at(position, Bias::Left)
    }

    pub fn anchor_after<T: ToOffset>(&self, position: T) -> Anchor {
        self.anchor_at(position, Bias::Right)
    }

    pub fn anchor_at<T: ToOffset>(&self, position: T, bias: Bias) -> Anchor {
        let position = self.clip_offset(position.to_offset(self), bias);
        let text_anchor = match bias {
            Bias::Left => self.buffer.anchor_before(position.0),
            Bias::Right => self.buffer.anchor_after(position.0),
        };

        Anchor {
            excerpt_id: self.excerpt_id,
            text_anchor,
        }
    }

    pub fn offset_for_anchor(&self, anchor: Anchor) -> MultiBufferOffset {
        if anchor.is_min() {
            return MultiBufferOffset::ZERO;
        }
        if anchor.is_max() {
            return self.len();
        }
        MultiBufferOffset(anchor.text_anchor.summary::<usize>(&self.buffer))
    }

    pub fn point_for_anchor(&self, anchor: Anchor) -> Point {
        if anchor.is_min() {
            return Point::zero();
        }
        if anchor.is_max() {
            return self.max_point();
        }
        anchor.text_anchor.summary::<Point>(&self.buffer)
    }

    fn summary_for_anchor(&self, anchor: Anchor) -> MBTextSummary {
        let offset = self.offset_for_anchor(anchor);
        let point = self.point_for_anchor(anchor);
        let offset_utf16 = self.offset_to_offset_utf16(offset);
        let point_utf16 = self.point_to_point_utf16(point);
        MBTextSummary {
            len: offset,
            len_utf16: offset_utf16.0,
            lines: point,
            last_line_len_utf16: point_utf16.column,
        }
    }

    pub fn summaries_for_anchors<'a, D, I>(&self, anchors: I) -> Vec<D>
    where
        D: MultiBufferDimension,
        I: IntoIterator<Item = &'a Anchor>,
    {
        anchors
            .into_iter()
            .map(|anchor| D::from_summary(&self.summary_for_anchor(*anchor)))
            .collect()
    }

    pub fn chars_at<T: ToOffset>(&self, position: T) -> impl Iterator<Item = char> + '_ {
        let offset = position.to_offset(self);
        self.text_for_range(offset..self.len())
            .flat_map(|chunk| chunk.chars())
    }

    pub fn is_inside_word<T: ToOffset>(&self, position: T, _: Option<CharScopeContext>) -> bool {
        let position = position.to_offset(self);
        let classifier = self.char_classifier_at(position);
        let next_char_kind = self.chars_at(position).next().map(|ch| classifier.kind(ch));
        let prev_char_kind = self
            .reversed_chars_at(position)
            .next()
            .map(|ch| classifier.kind(ch));
        prev_char_kind.zip(next_char_kind) == Some((CharKind::Word, CharKind::Word))
    }

    pub fn surrounding_word<T: ToOffset>(
        &self,
        start: T,
        _: Option<CharScopeContext>,
    ) -> (Range<MultiBufferOffset>, Option<CharKind>) {
        let mut start = start.to_offset(self);
        let mut end = start;
        let mut next_chars = self.chars_at(start).peekable();
        let mut prev_chars = self.reversed_chars_at(start).peekable();
        let classifier = self.char_classifier_at(start);

        let word_kind = cmp::max(
            prev_chars.peek().copied().map(|ch| classifier.kind(ch)),
            next_chars.peek().copied().map(|ch| classifier.kind(ch)),
        );

        for ch in prev_chars {
            if Some(classifier.kind(ch)) == word_kind && ch != '\n' {
                start -= ch.len_utf8();
            } else {
                break;
            }
        }

        for ch in next_chars {
            if Some(classifier.kind(ch)) == word_kind && ch != '\n' {
                end += ch.len_utf8();
            } else {
                break;
            }
        }

        (start..end, word_kind)
    }

    pub fn reversed_chars_at<T: ToOffset>(&self, position: T) -> impl Iterator<Item = char> + '_ {
        self.reversed_chunks_in_range(MultiBufferOffset::ZERO..position.to_offset(self))
            .flat_map(|chunk| chunk.chars().rev())
    }

    fn reversed_chunks_in_range(
        &self,
        range: Range<MultiBufferOffset>,
    ) -> impl Iterator<Item = &str> + '_ {
        self.buffer
            .reversed_chunks_in_range(range.start.0..range.end.0)
    }

    pub fn line_len(&self, row: MultiBufferRow) -> u32 {
        self.buffer.line_len(row.0)
    }

    pub fn text_for_range<T: ToOffset>(&self, range: Range<T>) -> impl Iterator<Item = &str> + '_ {
        self.chunks(range).map(|chunk| chunk.text)
    }

    pub fn chunks<T: ToOffset>(&self, range: Range<T>) -> MultiBufferChunks<'_> {
        let start = range.start.to_offset(self);
        let end = range.end.to_offset(self);
        MultiBufferChunks {
            text_chunks: self.buffer.text_for_range(start.0..end.0),
        }
    }

    pub fn text_summary(&self) -> TextSummary {
        self.buffer
            .text_summary_for_range(MultiBufferOffset::ZERO.0..self.len().0)
    }

    pub fn bytes_in_range<T: ToOffset>(&self, range: Range<T>) -> impl Iterator<Item = &[u8]> + '_ {
        let start = range.start.to_offset(self);
        let end = range.end.to_offset(self);
        self.buffer.bytes_in_range(start.0..end.0)
    }

    pub fn edit_count(&self) -> usize {
        self.edit_count
    }
}

impl ExcerptId {
    pub fn min() -> Self {
        Self(0)
    }

    pub fn max() -> Self {
        Self(u32::MAX)
    }

    pub fn cmp(&self, other: &Self, _: &MultiBufferSnapshot) -> cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl From<ExcerptId> for usize {
    fn from(val: ExcerptId) -> Self {
        val.0 as usize
    }
}

impl ToOffset for Point {
    fn to_offset(&self, snapshot: &MultiBufferSnapshot) -> MultiBufferOffset {
        snapshot.point_to_offset(*self)
    }

    fn to_offset_utf16(&self, snapshot: &MultiBufferSnapshot) -> MultiBufferOffsetUtf16 {
        snapshot.point_to_offset_utf16(*self)
    }
}

impl ToOffset for MultiBufferOffset {
    #[track_caller]
    fn to_offset(&self, snapshot: &MultiBufferSnapshot) -> MultiBufferOffset {
        assert!(
            *self <= snapshot.len(),
            "offset {} is greater than the snapshot.len() {}",
            self.0,
            snapshot.len().0,
        );
        *self
    }

    fn to_offset_utf16(&self, snapshot: &MultiBufferSnapshot) -> MultiBufferOffsetUtf16 {
        snapshot.offset_to_offset_utf16(*self)
    }
}

impl ToOffset for MultiBufferOffsetUtf16 {
    fn to_offset(&self, snapshot: &MultiBufferSnapshot) -> MultiBufferOffset {
        snapshot.offset_utf16_to_offset(*self)
    }

    fn to_offset_utf16(&self, _: &MultiBufferSnapshot) -> MultiBufferOffsetUtf16 {
        *self
    }
}

impl ToOffset for PointUtf16 {
    fn to_offset(&self, snapshot: &MultiBufferSnapshot) -> MultiBufferOffset {
        snapshot.point_utf16_to_offset(*self)
    }

    fn to_offset_utf16(&self, snapshot: &MultiBufferSnapshot) -> MultiBufferOffsetUtf16 {
        snapshot.point_utf16_to_offset_utf16(*self)
    }
}

impl ToPoint for MultiBufferOffset {
    fn to_point(&self, snapshot: &MultiBufferSnapshot) -> Point {
        snapshot.offset_to_point(*self)
    }

    fn to_point_utf16(&self, snapshot: &MultiBufferSnapshot) -> PointUtf16 {
        snapshot.offset_to_point_utf16(*self)
    }
}

impl ToPoint for Point {
    fn to_point(&self, _: &MultiBufferSnapshot) -> Point {
        *self
    }

    fn to_point_utf16(&self, snapshot: &MultiBufferSnapshot) -> PointUtf16 {
        snapshot.point_to_point_utf16(*self)
    }
}

impl ToPoint for PointUtf16 {
    fn to_point(&self, snapshot: &MultiBufferSnapshot) -> Point {
        snapshot.point_utf16_to_point(*self)
    }

    fn to_point_utf16(&self, _: &MultiBufferSnapshot) -> PointUtf16 {
        *self
    }
}
