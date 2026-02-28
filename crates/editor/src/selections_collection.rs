use std::{
    fmt, iter,
    ops::{AddAssign, Deref, DerefMut, Range, Sub},
    sync::Arc,
};

use multi_buffer::{
    Anchor, MultiBufferDimension, MultiBufferOffset, MultiBufferSnapshot, ToOffset,
};
use text::{Bias, Point, Selection, SelectionGoal};

use crate::{
    SelectMode,
    display_map::{DisplayPoint, DisplaySnapshot},
};

#[derive(Debug, Clone)]
pub struct PendingSelection {
    selection: Selection<Anchor>,
    mode: SelectMode,
}

#[derive(Clone, Debug)]
pub struct SelectionsCollection {
    next_selection_id: usize,
    disjoint: Arc<[Selection<Anchor>]>,
    pending: Option<PendingSelection>,
    select_mode: SelectMode,
    is_extending: bool,
}

impl SelectionsCollection {
    pub fn new() -> Self {
        Self {
            next_selection_id: 1,
            disjoint: Arc::default(),
            pending: Some(PendingSelection {
                selection: Selection {
                    id: 0,
                    start: Anchor::min(),
                    end: Anchor::min(),
                    reversed: false,
                    goal: SelectionGoal::None,
                },
                mode: SelectMode::Character,
            }),
            select_mode: SelectMode::Character,
            is_extending: false,
        }
    }

    pub fn disjoint_anchors_arc(&self) -> Arc<[Selection<Anchor>]> {
        self.disjoint.clone()
    }

    pub fn disjoint_anchors(&self) -> &[Selection<Anchor>] {
        &self.disjoint
    }

    pub fn pending_anchor(&self) -> Option<&Selection<Anchor>> {
        self.pending.as_ref().map(|pending| &pending.selection)
    }

    pub(crate) fn pending_mode(&self) -> Option<SelectMode> {
        self.pending.as_ref().map(|pending| pending.mode.clone())
    }

    pub fn newest_anchor(&self) -> &Selection<Anchor> {
        self.pending
            .as_ref()
            .map(|pending| &pending.selection)
            .or_else(|| self.disjoint.iter().max_by_key(|selection| selection.id))
            .expect("there must be at least one selection")
    }

    pub fn newest<D>(&self, snapshot: &DisplaySnapshot) -> Selection<D>
    where
        D: MultiBufferDimension + Sub + AddAssign<<D as Sub>::Output> + Ord,
    {
        resolve_selections_wrapping_blocks([self.newest_anchor()], snapshot)
            .next()
            .expect("there must be at least one selection")
    }

    pub fn pending<D>(&self, snapshot: &DisplaySnapshot) -> Option<Selection<D>>
    where
        D: MultiBufferDimension + Sub + AddAssign<<D as Sub>::Output> + Ord,
    {
        resolve_selections_wrapping_blocks(self.pending_anchor(), snapshot).next()
    }

    pub fn all<D>(&self, snapshot: &DisplaySnapshot) -> Vec<Selection<D>>
    where
        D: MultiBufferDimension + Sub + AddAssign<<D as Sub>::Output> + Ord,
    {
        let mut disjoint =
            resolve_selections_wrapping_blocks(self.disjoint.iter(), snapshot).peekable();
        let mut pending = self.pending(snapshot);
        iter::from_fn(move || {
            if let Some(pending_selection) = pending.as_mut() {
                while let Some(next_selection) = disjoint.peek() {
                    if should_merge(
                        pending_selection.start,
                        pending_selection.end,
                        next_selection.start,
                        next_selection.end,
                        false,
                    ) {
                        let next_selection = disjoint.next().expect("peek just returned Some");
                        if next_selection.start < pending_selection.start {
                            pending_selection.start = next_selection.start;
                        }
                        if next_selection.end > pending_selection.end {
                            pending_selection.end = next_selection.end;
                        }
                    } else if next_selection.end < pending_selection.start {
                        return disjoint.next();
                    } else {
                        break;
                    }
                }

                pending.take()
            } else {
                disjoint.next()
            }
        })
        .collect()
    }

    pub fn all_display(&self, snapshot: &DisplaySnapshot) -> Vec<Selection<DisplayPoint>> {
        let mut disjoint = resolve_selections_display(self.disjoint.iter(), snapshot).peekable();
        let mut pending = resolve_selections_display(self.pending_anchor(), snapshot).next();
        iter::from_fn(move || {
            if let Some(pending_selection) = pending.as_mut() {
                while let Some(next_selection) = disjoint.peek() {
                    if should_merge(
                        pending_selection.start,
                        pending_selection.end,
                        next_selection.start,
                        next_selection.end,
                        false,
                    ) {
                        let next_selection = disjoint.next().expect("peek just returned Some");
                        if next_selection.start < pending_selection.start {
                            pending_selection.start = next_selection.start;
                        }
                        if next_selection.end > pending_selection.end {
                            pending_selection.end = next_selection.end;
                        }
                    } else if next_selection.end < pending_selection.start {
                        return disjoint.next();
                    } else {
                        break;
                    }
                }

                pending.take()
            } else {
                disjoint.next()
            }
        })
        .collect()
    }

    pub fn select_mode(&self) -> &SelectMode {
        &self.select_mode
    }

    pub fn set_select_mode(&mut self, select_mode: SelectMode) {
        self.select_mode = select_mode;
    }

    pub fn is_extending(&self) -> bool {
        self.is_extending
    }

    pub fn set_is_extending(&mut self, is_extending: bool) {
        self.is_extending = is_extending;
    }

    pub fn change_with<R>(
        &mut self,
        snapshot: &DisplaySnapshot,
        change: impl FnOnce(&mut MutableSelectionsCollection<'_, '_>) -> R,
    ) -> (bool, R) {
        let mut mutable_collection = MutableSelectionsCollection {
            snapshot,
            collection: self,
            selections_changed: false,
        };

        let result = change(&mut mutable_collection);
        assert!(
            !mutable_collection.disjoint.is_empty() || mutable_collection.pending.is_some(),
            "There must be at least one selection"
        );

        (mutable_collection.selections_changed, result)
    }
}

pub struct MutableSelectionsCollection<'snap, 'a> {
    collection: &'a mut SelectionsCollection,
    snapshot: &'snap DisplaySnapshot,
    selections_changed: bool,
}

impl<'snap, 'a> fmt::Debug for MutableSelectionsCollection<'snap, 'a> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MutableSelectionsCollection")
            .field("collection", &self.collection)
            .field("selections_changed", &self.selections_changed)
            .finish()
    }
}

impl MutableSelectionsCollection<'_, '_> {
    pub fn display_snapshot(&self) -> DisplaySnapshot {
        self.snapshot.clone()
    }

    pub fn new_selection_id(&mut self) -> usize {
        let id = self.collection.next_selection_id;
        self.collection.next_selection_id += 1;
        id
    }

    pub fn clear_disjoint(&mut self) {
        if !self.collection.disjoint.is_empty() {
            self.collection.disjoint = Arc::default();
            self.selections_changed = true;
        }
    }

    pub fn clear_pending(&mut self) {
        if self.collection.pending.is_some() {
            self.collection.pending = None;
            self.selections_changed = true;
        }
    }

    pub(crate) fn set_pending_anchor_range(&mut self, range: Range<Anchor>, mode: SelectMode) {
        let snapshot = self.snapshot.buffer_snapshot();
        let mut start = range.start;
        let mut end = range.end;
        let reversed = snapshot.offset_for_anchor(start) > snapshot.offset_for_anchor(end);
        if reversed {
            std::mem::swap(&mut start, &mut end);
        }

        self.collection.pending = Some(PendingSelection {
            selection: Selection {
                id: self.new_selection_id(),
                start,
                end,
                reversed,
                goal: SelectionGoal::None,
            },
            mode,
        });
        self.selections_changed = true;
    }

    pub(crate) fn set_pending(&mut self, selection: Selection<Anchor>, mode: SelectMode) {
        self.collection.pending = Some(PendingSelection { selection, mode });
        self.selections_changed = true;
    }

    pub fn select<T>(&mut self, selections: Vec<Selection<T>>)
    where
        T: ToOffset + Copy + fmt::Debug,
    {
        let mut selections = selections
            .into_iter()
            .map(|selection| selection.map(|it| it.to_offset(self.snapshot.buffer_snapshot())))
            .map(|mut selection| {
                if selection.start > selection.end {
                    std::mem::swap(&mut selection.start, &mut selection.end);
                    selection.reversed = true;
                }
                selection
            })
            .collect::<Vec<_>>();

        selections.sort_unstable_by_key(|selection| selection.start);

        let mut index = 1;
        while index < selections.len() {
            let previous = &selections[index - 1];
            let current = &selections[index];

            if should_merge(
                previous.start,
                previous.end,
                current.start,
                current.end,
                true,
            ) {
                let removed = selections.remove(index);
                if removed.start < selections[index - 1].start {
                    selections[index - 1].start = removed.start;
                }
                if selections[index - 1].end < removed.end {
                    selections[index - 1].end = removed.end;
                }
            } else {
                index += 1;
            }
        }

        let new_disjoint = Arc::from_iter(selections.into_iter().map(|selection| {
            selection_to_anchor_selection(selection, self.snapshot.buffer_snapshot())
        }));

        let had_pending = self.collection.pending.is_some();
        self.collection.pending = None;
        if self.collection.disjoint != new_disjoint || had_pending {
            self.collection.disjoint = new_disjoint;
            self.selections_changed = true;
        }
    }

    pub fn select_anchors(&mut self, selections: Vec<Selection<Anchor>>) {
        let map = self.display_snapshot();
        let resolved =
            resolve_selections_wrapping_blocks::<MultiBufferOffset, _>(selections.iter(), &map)
                .collect::<Vec<_>>();
        self.select(resolved);
    }

    pub fn select_ranges<I, T>(&mut self, ranges: I)
    where
        I: IntoIterator<Item = Range<T>>,
        T: ToOffset,
    {
        let snapshot = self.snapshot.buffer_snapshot();
        let selections = ranges
            .into_iter()
            .map(|range| {
                let mut start = snapshot.clip_offset(range.start.to_offset(snapshot), Bias::Left);
                let mut end = snapshot.clip_offset(range.end.to_offset(snapshot), Bias::Right);
                let reversed = end < start;
                if reversed {
                    std::mem::swap(&mut start, &mut end);
                }

                Selection {
                    id: self.new_selection_id(),
                    start,
                    end,
                    reversed,
                    goal: SelectionGoal::None,
                }
            })
            .collect::<Vec<_>>();
        self.select(selections);
    }

    pub fn move_with(
        &mut self,
        move_selection: &mut dyn FnMut(&DisplaySnapshot, &mut Selection<DisplayPoint>),
    ) {
        let mut changed = false;
        let display_map = self.display_snapshot();
        let selections = self
            .collection
            .all_display(&display_map)
            .into_iter()
            .map(|selection| {
                let mut moved = selection.clone();
                move_selection(&display_map, &mut moved);
                if selection != moved {
                    changed = true;
                }
                moved.map(|display_point| display_point.to_point(&display_map))
            })
            .collect::<Vec<_>>();

        if changed {
            self.select(selections);
        }
    }

    pub fn move_offsets_with(
        &mut self,
        move_selection: &mut dyn FnMut(&MultiBufferSnapshot, &mut Selection<MultiBufferOffset>),
    ) {
        let mut changed = false;
        let display_map = self.display_snapshot();
        let selections = self
            .collection
            .all::<MultiBufferOffset>(&display_map)
            .into_iter()
            .map(|selection| {
                let mut moved = selection.clone();
                move_selection(self.snapshot.buffer_snapshot(), &mut moved);
                if selection != moved {
                    changed = true;
                }
                moved
            })
            .collect::<Vec<_>>();

        if changed {
            self.select(selections);
        }
    }

    pub fn move_heads_with(
        &mut self,
        update_head: &mut dyn FnMut(
            &DisplaySnapshot,
            DisplayPoint,
            SelectionGoal,
        ) -> (DisplayPoint, SelectionGoal),
    ) {
        self.move_with(&mut |map, selection| {
            let (new_head, new_goal) = update_head(map, selection.head(), selection.goal);
            selection.set_head(new_head, new_goal);
        });
    }

    pub fn move_cursors_with(
        &mut self,
        update_cursor_position: &mut dyn FnMut(
            &DisplaySnapshot,
            DisplayPoint,
            SelectionGoal,
        ) -> (DisplayPoint, SelectionGoal),
    ) {
        self.move_with(&mut |map, selection| {
            let (cursor, new_goal) = update_cursor_position(map, selection.head(), selection.goal);
            selection.collapse_to(cursor, new_goal);
        });
    }
}

impl Deref for MutableSelectionsCollection<'_, '_> {
    type Target = SelectionsCollection;

    fn deref(&self) -> &Self::Target {
        self.collection
    }
}

impl DerefMut for MutableSelectionsCollection<'_, '_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.collection
    }
}

fn selection_to_anchor_selection(
    selection: Selection<MultiBufferOffset>,
    buffer: &MultiBufferSnapshot,
) -> Selection<Anchor> {
    let end_bias = if selection.start == selection.end {
        Bias::Right
    } else {
        Bias::Left
    };

    Selection {
        id: selection.id,
        start: buffer.anchor_after(selection.start),
        end: buffer.anchor_at(selection.end, end_bias),
        reversed: selection.reversed,
        goal: selection.goal,
    }
}

fn resolve_selections_point<'a>(
    selections: impl 'a + IntoIterator<Item = &'a Selection<Anchor>>,
    map: &'a DisplaySnapshot,
) -> impl 'a + Iterator<Item = Selection<Point>> {
    let buffer_snapshot = map.buffer_snapshot();
    selections.into_iter().map(move |selection| Selection {
        id: selection.id,
        start: buffer_snapshot.point_for_anchor(selection.start),
        end: buffer_snapshot.point_for_anchor(selection.end),
        reversed: selection.reversed,
        goal: selection.goal,
    })
}

fn resolve_selections_display<'a>(
    selections: impl 'a + IntoIterator<Item = &'a Selection<Anchor>>,
    map: &'a DisplaySnapshot,
) -> impl 'a + Iterator<Item = Selection<DisplayPoint>> {
    resolve_selections_point(selections, map).map(move |selection| {
        let start = map.point_to_display_point(selection.start, Bias::Left);
        let end = map.point_to_display_point(
            selection.end,
            if selection.start == selection.end {
                Bias::Right
            } else {
                Bias::Left
            },
        );

        Selection {
            id: selection.id,
            start,
            end,
            reversed: selection.reversed,
            goal: selection.goal,
        }
    })
}

pub(crate) fn resolve_selections_wrapping_blocks<'a, D, I>(
    selections: I,
    map: &'a DisplaySnapshot,
) -> impl 'a + Iterator<Item = Selection<D>>
where
    D: MultiBufferDimension + Sub + AddAssign<<D as Sub>::Output> + Ord,
    I: 'a + IntoIterator<Item = &'a Selection<Anchor>>,
{
    let buffer_snapshot = map.buffer_snapshot();
    selections.into_iter().map(move |selection| {
        let mut summaries = buffer_snapshot
            .summaries_for_anchors::<D, _>([&selection.start, &selection.end])
            .into_iter();
        let start = summaries.next().expect("start summary must exist");
        let end = summaries.next().expect("end summary must exist");
        Selection {
            id: selection.id,
            start,
            end,
            reversed: selection.reversed,
            goal: selection.goal,
        }
    })
}

fn should_merge<T: Ord + Copy>(
    first_start: T,
    first_end: T,
    second_start: T,
    second_end: T,
    sorted: bool,
) -> bool {
    let is_overlapping = if sorted {
        second_start < first_end
    } else {
        first_start < second_end && second_start < first_end
    };

    let same_start = first_start == second_start;

    let is_cursor_first = first_start == first_end;
    let is_cursor_second = second_start == second_end;
    let cursor_at_boundary = (is_cursor_first || is_cursor_second)
        && (first_start == second_start || first_end == second_end);

    is_overlapping || same_start || cursor_at_boundary
}
