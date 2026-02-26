mod raw_chunks;
mod tab_map;

pub use tab_map::{TabMap, TabPoint, TabSnapshot};

use gpui::{Context, Entity, Pixels, TextRun};
use serde::Deserialize;
use std::{
    fmt::Debug,
    num::NonZeroU32,
    ops::{Add, Sub},
    sync::Arc,
};
use text::{Bias, Point, subscription::Subscription as BufferSubscription};

use multi_buffer::{Anchor, MultiBuffer, MultiBufferOffset, MultiBufferPoint, MultiBufferSnapshot};

use crate::movement::TextLayoutDetails;

pub trait ToDisplayPoint {
    fn to_display_point(&self, map: &DisplaySnapshot) -> DisplayPoint;
}

pub struct DisplayMap {
    buffer: Entity<MultiBuffer>,
    buffer_subscription: BufferSubscription<MultiBufferOffset>,
    tab_map: TabMap,
    tab_size: NonZeroU32,
}

impl DisplayMap {
    pub fn new(buffer: Entity<MultiBuffer>, tab_size: NonZeroU32, cx: &mut Context<Self>) -> Self {
        let buffer_subscription = buffer.update(cx, |buffer, _| buffer.subscribe());
        let buffer_snapshot = buffer.read(cx).snapshot(cx);
        let (tab_map, _) = TabMap::new(buffer_snapshot, tab_size);

        Self {
            buffer,
            buffer_subscription,
            tab_map,
            tab_size,
        }
    }

    fn sync_through_tab(&mut self, cx: &mut Context<Self>) -> TabSnapshot {
        let buffer_snapshot = self.buffer.read(cx).snapshot(cx);
        let edits = self.buffer_subscription.consume().into_inner();
        let (snapshot, _tab_edits) = self.tab_map.sync(buffer_snapshot, edits, self.tab_size);
        snapshot
    }

    pub fn snapshot(&mut self, cx: &mut Context<Self>) -> DisplaySnapshot {
        let tab_snapshot = self.sync_through_tab(cx);

        DisplaySnapshot { tab_snapshot }
    }
}

#[derive(Clone)]
pub struct DisplaySnapshot {
    tab_snapshot: TabSnapshot,
}

impl DisplaySnapshot {
    pub fn tab_snapshot(&self) -> &TabSnapshot {
        &self.tab_snapshot
    }

    pub fn buffer_snapshot(&self) -> &MultiBufferSnapshot {
        self.tab_snapshot.buffer_snapshot()
    }

    pub fn point_to_display_point(&self, point: MultiBufferPoint, bias: Bias) -> DisplayPoint {
        DisplayPoint::from_tab_point(self.tab_snapshot.point_to_tab_point(point, bias))
    }

    pub fn display_point_to_point(&self, point: DisplayPoint, bias: Bias) -> MultiBufferPoint {
        self.tab_snapshot
            .tab_point_to_point(point.to_tab_point(), bias)
    }

    pub fn display_point_to_anchor(&self, point: DisplayPoint, bias: Bias) -> Anchor {
        let offset = point.to_offset(self, bias);
        match bias {
            Bias::Left => self.buffer_snapshot().anchor_before(offset),
            Bias::Right => self.buffer_snapshot().anchor_after(offset),
        }
    }

    pub fn max_point(&self) -> DisplayPoint {
        DisplayPoint::from_tab_point(self.tab_snapshot.max_point())
    }

    pub fn text_chunks(&self, display_row: DisplayRow) -> impl Iterator<Item = &str> {
        let max_point = self.max_point().to_tab_point();
        let start = if display_row.0 > max_point.row() {
            max_point
        } else {
            TabPoint::new(display_row.0, 0)
        };

        self.tab_snapshot
            .chunks(start..max_point)
            .map(|chunk| chunk.text)
    }

    pub fn clip_point(&self, point: DisplayPoint, bias: Bias) -> DisplayPoint {
        DisplayPoint::from_tab_point(self.tab_snapshot.clip_point(point.to_tab_point(), bias))
    }

    pub fn line_len(&self, row: DisplayRow) -> u32 {
        self.tab_snapshot.line_len(row.0)
    }

    pub fn longest_row(&self) -> DisplayRow {
        DisplayRow(self.buffer_snapshot().text_summary().longest_row)
    }

    pub fn layout_row(
        &self,
        display_row: DisplayRow,
        TextLayoutDetails {
            text_system,
            editor_style,
            rem_size,
        }: &TextLayoutDetails,
    ) -> Arc<gpui::LineLayout> {
        let mut line = String::new();
        for chunk in self.text_chunks(display_row) {
            if let Some(newline_index) = chunk.find('\n') {
                line.push_str(&chunk[..newline_index]);
                break;
            }
            line.push_str(chunk);
        }

        let runs = [TextRun {
            len: line.len(),
            font: editor_style.text.font(),
            color: editor_style.text.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        }];
        let font_size = editor_style.text.font_size.to_pixels(*rem_size);
        text_system.layout_line(&line, font_size, &runs, None)
    }

    pub fn x_for_display_point(
        &self,
        display_point: DisplayPoint,
        text_layout_details: &TextLayoutDetails,
    ) -> Pixels {
        let line = self.layout_row(display_point.row(), text_layout_details);
        line.x_for_index(display_point.column() as usize)
    }

    pub fn display_column_for_x(
        &self,
        display_row: DisplayRow,
        x: Pixels,
        details: &TextLayoutDetails,
    ) -> u32 {
        let layout_line = self.layout_row(display_row, details);
        layout_line.closest_index_for_x(x) as u32
    }
}

/// A zero-indexed point in a display buffer consisting of a row and column.
#[derive(Copy, Clone, Default, Eq, Ord, PartialOrd, PartialEq)]
pub struct DisplayPoint(Point);

impl Debug for DisplayPoint {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_fmt(format_args!(
            "DisplayPoint({}, {})",
            self.row().0,
            self.column(),
        ))
    }
}

impl Add for DisplayPoint {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        DisplayPoint(self.0 + other.0)
    }
}

impl Sub for DisplayPoint {
    type Output = Self;

    fn sub(self, other: Self) -> Self::Output {
        DisplayPoint(self.0 - other.0)
    }
}

#[derive(Debug, Copy, Clone, Default, Eq, Ord, PartialOrd, PartialEq, Deserialize, Hash)]
#[serde(transparent)]
pub struct DisplayRow(pub u32);

impl Add<DisplayRow> for DisplayRow {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        DisplayRow(self.0 + other.0)
    }
}

impl Add<u32> for DisplayRow {
    type Output = Self;

    fn add(self, other: u32) -> Self::Output {
        DisplayRow(self.0 + other)
    }
}

impl Sub<DisplayRow> for DisplayRow {
    type Output = Self;

    fn sub(self, other: Self) -> Self::Output {
        DisplayRow(self.0 - other.0)
    }
}

impl Sub<u32> for DisplayRow {
    type Output = Self;

    fn sub(self, other: u32) -> Self::Output {
        DisplayRow(self.0 - other)
    }
}

impl DisplayPoint {
    pub fn new(row: DisplayRow, column: u32) -> Self {
        Self(Point::new(row.0, column))
    }

    pub fn row(self) -> DisplayRow {
        DisplayRow(self.0.row)
    }

    pub fn column(self) -> u32 {
        self.0.column
    }

    pub fn row_mut(&mut self) -> &mut u32 {
        &mut self.0.row
    }

    pub fn column_mut(&mut self) -> &mut u32 {
        &mut self.0.column
    }

    pub fn to_point(self, map: &DisplaySnapshot) -> Point {
        map.display_point_to_point(self, Bias::Left)
    }

    pub fn to_offset(self, map: &DisplaySnapshot, bias: Bias) -> MultiBufferOffset {
        map.buffer_snapshot()
            .point_to_offset(map.display_point_to_point(self, bias))
    }

    fn to_tab_point(self) -> TabPoint {
        TabPoint(self.0)
    }

    fn from_tab_point(point: TabPoint) -> Self {
        Self(point.0)
    }
}

impl ToDisplayPoint for usize {
    fn to_display_point(&self, map: &DisplaySnapshot) -> DisplayPoint {
        let offset = map.buffer_snapshot().clip_offset(
            MultiBufferOffset((*self).min(map.buffer_snapshot().len().0)),
            Bias::Left,
        );
        map.point_to_display_point(map.buffer_snapshot().offset_to_point(offset), Bias::Left)
    }
}

impl ToDisplayPoint for MultiBufferOffset {
    fn to_display_point(&self, map: &DisplaySnapshot) -> DisplayPoint {
        let offset = map.buffer_snapshot().clip_offset(*self, Bias::Left);
        map.point_to_display_point(map.buffer_snapshot().offset_to_point(offset), Bias::Left)
    }
}

impl ToDisplayPoint for Point {
    fn to_display_point(&self, map: &DisplaySnapshot) -> DisplayPoint {
        map.point_to_display_point(*self, Bias::Left)
    }
}

impl ToDisplayPoint for Anchor {
    fn to_display_point(&self, map: &DisplaySnapshot) -> DisplayPoint {
        map.point_to_display_point(map.buffer_snapshot().point_for_anchor(*self), self.bias())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use gpui::AppContext;
    use settings::SettingsStore;
    use text::{Buffer as TextBuffer, ReplicaId};

    use crate::tests::util::marked_display_snapshot;

    fn init_test(cx: &mut gpui::App) {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        theme::init(theme::LoadThemes::JustBase, cx);
        crate::init(cx);
    }

    fn display_snapshot_for_text(text: &str, cx: &mut gpui::App) -> DisplaySnapshot {
        let text_buffer =
            cx.new(|_| TextBuffer::new(ReplicaId::LOCAL, crate::next_buffer_id(), text));
        let multi_buffer = cx.new(|cx| MultiBuffer::singleton(text_buffer, cx));
        let display_map = cx.new(|cx| {
            crate::display_map::DisplayMap::new(multi_buffer, crate::DEFAULT_TAB_SIZE, cx)
        });
        display_map.update(cx, |display_map, cx| display_map.snapshot(cx))
    }

    #[gpui::test]
    fn test_text_chunks(cx: &mut gpui::App) {
        init_test(cx);

        let text = "aaaaaa\nbbbbbb\ncccccc\ndddddd\neeeeee\nffffff";
        let text_buffer =
            cx.new(|_| TextBuffer::new(ReplicaId::LOCAL, crate::next_buffer_id(), text));
        let multi_buffer = cx.new(|cx| MultiBuffer::singleton(text_buffer, cx));
        let display_map = cx.new(|cx| {
            crate::display_map::DisplayMap::new(multi_buffer.clone(), crate::DEFAULT_TAB_SIZE, cx)
        });

        multi_buffer.update(cx, |multi_buffer, cx| {
            multi_buffer.edit(
                vec![
                    (
                        MultiBufferPoint::new(1, 0)..MultiBufferPoint::new(1, 0),
                        "\t",
                    ),
                    (
                        MultiBufferPoint::new(1, 1)..MultiBufferPoint::new(1, 1),
                        "\t",
                    ),
                    (
                        MultiBufferPoint::new(2, 1)..MultiBufferPoint::new(2, 1),
                        "\t",
                    ),
                ],
                cx,
            );
        });

        assert_eq!(
            display_map
                .update(cx, |display_map, cx| display_map.snapshot(cx))
                .text_chunks(DisplayRow(1))
                .collect::<String>()
                .lines()
                .next(),
            Some("    b   bbbbb")
        );
        assert_eq!(
            display_map
                .update(cx, |display_map, cx| display_map.snapshot(cx))
                .text_chunks(DisplayRow(2))
                .collect::<String>()
                .lines()
                .next(),
            Some("c   ccccc")
        );
    }

    #[gpui::test]
    fn test_clip_point(cx: &mut gpui::App) {
        init_test(cx);

        fn assert(marked_text: &str, shift_right: bool, bias: Bias, cx: &mut gpui::App) {
            let (unmarked_snapshot, mut markers) = marked_display_snapshot(marked_text, cx);

            match bias {
                Bias::Left => {
                    if shift_right {
                        *markers[1].column_mut() += 1;
                    }

                    assert_eq!(unmarked_snapshot.clip_point(markers[1], bias), markers[0]);
                }
                Bias::Right => {
                    if shift_right {
                        *markers[0].column_mut() += 1;
                    }

                    assert_eq!(unmarked_snapshot.clip_point(markers[0], bias), markers[1]);
                }
            }
        }

        assert("ˇˇα", false, Bias::Left, cx);
        assert("ˇˇα", true, Bias::Left, cx);
        assert("ˇˇα", false, Bias::Right, cx);
        assert("ˇαˇ", true, Bias::Right, cx);
        assert("ˇˇ🌙", false, Bias::Left, cx);
        assert("ˇˇ🌙", true, Bias::Left, cx);
        assert("ˇˇ🌙", false, Bias::Right, cx);
        assert("ˇ🌙ˇ", true, Bias::Right, cx);
        assert("ˇˇ⚽", false, Bias::Left, cx);
        assert("ˇˇ⚽", true, Bias::Left, cx);
        assert("ˇˇ⚽", false, Bias::Right, cx);
        assert("ˇ⚽ˇ", true, Bias::Right, cx);
        assert("ˇˇ\t", false, Bias::Left, cx);
        assert("ˇˇ\t", true, Bias::Left, cx);
        assert("ˇˇ\t", false, Bias::Right, cx);
        assert("ˇ\tˇ", true, Bias::Right, cx);
        assert(" ˇˇ\t", false, Bias::Left, cx);
        assert(" ˇˇ\t", true, Bias::Left, cx);
        assert(" ˇˇ\t", false, Bias::Right, cx);
        assert(" ˇ\tˇ", true, Bias::Right, cx);
        assert("   ˇˇ\t", false, Bias::Left, cx);
        assert("   ˇˇ\t", false, Bias::Right, cx);
    }

    #[gpui::test]
    fn test_tabs_with_multibyte_chars(cx: &mut gpui::App) {
        init_test(cx);

        let map = display_snapshot_for_text("🌙\t\tα\nβ\t\n🏀β\t\tγ", cx);
        assert_eq!(
            map.text_chunks(DisplayRow(0)).collect::<String>(),
            "🌙       α\nβ   \n🏀β      γ"
        );
        assert_eq!(
            map.text_chunks(DisplayRow(1)).collect::<String>(),
            "β   \n🏀β      γ"
        );
        assert_eq!(
            map.text_chunks(DisplayRow(2)).collect::<String>(),
            "🏀β      γ"
        );

        let point = MultiBufferPoint::new(0, "🌙\t\t".len() as u32);
        let display_point = DisplayPoint::new(DisplayRow(0), "🌙       ".len() as u32);
        assert_eq!(point.to_display_point(&map), display_point);
        assert_eq!(display_point.to_point(&map), point);

        let point = MultiBufferPoint::new(1, "β\t".len() as u32);
        let display_point = DisplayPoint::new(DisplayRow(1), "β   ".len() as u32);
        assert_eq!(point.to_display_point(&map), display_point);
        assert_eq!(display_point.to_point(&map), point);

        let point = MultiBufferPoint::new(2, "🏀β\t\t".len() as u32);
        let display_point = DisplayPoint::new(DisplayRow(2), "🏀β      ".len() as u32);
        assert_eq!(point.to_display_point(&map), display_point);
        assert_eq!(display_point.to_point(&map), point);

        assert_eq!(
            DisplayPoint::new(DisplayRow(0), "🌙      ".len() as u32).to_point(&map),
            MultiBufferPoint::new(0, "🌙\t".len() as u32),
        );
        assert_eq!(
            DisplayPoint::new(DisplayRow(0), "🌙 ".len() as u32).to_point(&map),
            MultiBufferPoint::new(0, "🌙".len() as u32),
        );

        assert_eq!(
            map.clip_point(
                DisplayPoint::new(DisplayRow(0), "🌙".len() as u32 - 1),
                Bias::Left,
            ),
            DisplayPoint::new(DisplayRow(0), 0),
        );
        assert_eq!(
            map.clip_point(
                DisplayPoint::new(DisplayRow(0), "🌙".len() as u32 - 1),
                Bias::Right,
            ),
            DisplayPoint::new(DisplayRow(0), "🌙".len() as u32),
        );
    }

    #[gpui::test]
    fn test_max_point(cx: &mut gpui::App) {
        init_test(cx);

        let map = display_snapshot_for_text("aaa\n\t\tbbb", cx);
        assert_eq!(map.max_point(), DisplayPoint::new(DisplayRow(1), 11));
    }
}
