use std::sync::Arc;

use gpui::{Pixels, WindowTextSystem};
use text::{Bias, Point, SelectionGoal};

use multi_buffer::{CharClassifier, MultiBufferOffset, MultiBufferSnapshot};

use crate::{
    EditorStyle,
    display_map::{DisplayPoint, DisplayRow, DisplaySnapshot, ToDisplayPoint},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindRange {
    SingleLine,
    MultiLine,
}

pub struct TextLayoutDetails {
    pub(crate) text_system: Arc<WindowTextSystem>,
    pub(crate) editor_style: EditorStyle,
    pub(crate) rem_size: Pixels,
}

pub fn left(map: &DisplaySnapshot, mut point: DisplayPoint) -> DisplayPoint {
    if point.column() > 0 {
        *point.column_mut() -= 1;
    } else if point.row().0 > 0 {
        *point.row_mut() -= 1;
        *point.column_mut() = map.line_len(point.row());
    }

    map.clip_point(point, Bias::Left)
}

pub fn right(map: &DisplaySnapshot, mut point: DisplayPoint) -> DisplayPoint {
    if point.column() < map.line_len(point.row()) {
        *point.column_mut() += 1;
    } else if point.row().0 < map.buffer_snapshot().max_point().row {
        *point.row_mut() += 1;
        *point.column_mut() = 0;
    }

    map.clip_point(point, Bias::Right)
}

pub fn up(
    map: &DisplaySnapshot,
    start: DisplayPoint,
    goal: SelectionGoal,
    preserve_column_at_start: bool,
    text_layout_details: &TextLayoutDetails,
) -> (DisplayPoint, SelectionGoal) {
    up_by_rows(
        map,
        start,
        1,
        goal,
        preserve_column_at_start,
        text_layout_details,
    )
}

pub fn down(
    map: &DisplaySnapshot,
    start: DisplayPoint,
    goal: SelectionGoal,
    preserve_column_at_end: bool,
    text_layout_details: &TextLayoutDetails,
) -> (DisplayPoint, SelectionGoal) {
    down_by_rows(
        map,
        start,
        1,
        goal,
        preserve_column_at_end,
        text_layout_details,
    )
}

pub(crate) fn up_by_rows(
    map: &DisplaySnapshot,
    start: DisplayPoint,
    row_count: u32,
    goal: SelectionGoal,
    preserve_column_at_start: bool,
    text_layout_details: &TextLayoutDetails,
) -> (DisplayPoint, SelectionGoal) {
    let goal_x: Pixels = match goal {
        SelectionGoal::HorizontalPosition(x) => x.into(),
        SelectionGoal::HorizontalRange { end, .. } => end.into(),
        _ => map.x_for_display_point(start, text_layout_details),
    };

    let prev_row = DisplayRow(start.row().0.saturating_sub(row_count));
    let mut point = map.clip_point(
        DisplayPoint::new(prev_row, map.line_len(prev_row)),
        Bias::Left,
    );
    if point.row() < start.row() {
        *point.column_mut() = map.display_column_for_x(point.row(), goal_x, text_layout_details);
    } else if preserve_column_at_start {
        return (start, goal);
    } else {
        point = DisplayPoint::new(DisplayRow(0), 0);
    }

    let mut clipped_point = map.clip_point(point, Bias::Left);
    if clipped_point.row() < point.row() {
        clipped_point = map.clip_point(point, Bias::Right);
    }

    (
        clipped_point,
        SelectionGoal::HorizontalPosition(goal_x.into()),
    )
}

pub(crate) fn down_by_rows(
    map: &DisplaySnapshot,
    start: DisplayPoint,
    row_count: u32,
    goal: SelectionGoal,
    preserve_column_at_end: bool,
    text_layout_details: &TextLayoutDetails,
) -> (DisplayPoint, SelectionGoal) {
    let goal_x: Pixels = match goal {
        SelectionGoal::HorizontalPosition(x) => x.into(),
        SelectionGoal::HorizontalRange { end, .. } => end.into(),
        _ => map.x_for_display_point(start, text_layout_details),
    };

    let new_row = DisplayRow(start.row().0 + row_count);
    let mut point = map.clip_point(DisplayPoint::new(new_row, 0), Bias::Right);
    if point.row() > start.row() {
        *point.column_mut() = map.display_column_for_x(point.row(), goal_x, text_layout_details);
    } else if preserve_column_at_end {
        return (start, goal);
    } else {
        point = map.max_point();
    }

    let mut clipped_point = map.clip_point(point, Bias::Right);
    if clipped_point.row() > point.row() {
        clipped_point = map.clip_point(point, Bias::Left);
    }

    (
        clipped_point,
        SelectionGoal::HorizontalPosition(goal_x.into()),
    )
}

pub fn previous_word_start(map: &DisplaySnapshot, point: DisplayPoint) -> DisplayPoint {
    let raw_point = point.to_point(map);
    let classifier = map.buffer_snapshot().char_classifier_at(raw_point);

    let mut is_first_iteration = true;
    find_preceding_boundary_display_point(map, point, FindRange::MultiLine, |left, right| {
        if is_first_iteration
            && classifier.is_punctuation(right)
            && !classifier.is_punctuation(left)
            && left != '\n'
        {
            is_first_iteration = false;
            return false;
        }
        is_first_iteration = false;

        (classifier.kind(left) != classifier.kind(right) && !classifier.is_whitespace(right))
            || left == '\n'
    })
}

pub fn previous_word_start_or_newline(map: &DisplaySnapshot, point: DisplayPoint) -> DisplayPoint {
    let raw_point = point.to_point(map);
    let classifier = map.buffer_snapshot().char_classifier_at(raw_point);

    find_preceding_boundary_display_point(map, point, FindRange::MultiLine, |left, right| {
        (classifier.kind(left) != classifier.kind(right) && !classifier.is_whitespace(right))
            || left == '\n'
            || right == '\n'
    })
}

pub fn next_word_end(map: &DisplaySnapshot, point: DisplayPoint) -> DisplayPoint {
    let raw_point = point.to_point(map);
    let classifier = map.buffer_snapshot().char_classifier_at(raw_point);

    let mut is_first_iteration = true;
    find_boundary(map, point, FindRange::MultiLine, |left, right| {
        if is_first_iteration
            && classifier.is_punctuation(left)
            && !classifier.is_punctuation(right)
            && right != '\n'
        {
            is_first_iteration = false;
            return false;
        }
        is_first_iteration = false;

        (classifier.kind(left) != classifier.kind(right) && !classifier.is_whitespace(left))
            || right == '\n'
    })
}

pub fn next_word_end_or_newline(map: &DisplaySnapshot, point: DisplayPoint) -> DisplayPoint {
    let raw_point = point.to_point(map);
    let classifier = map.buffer_snapshot().char_classifier_at(raw_point);

    let mut on_starting_row = true;
    find_boundary(map, point, FindRange::MultiLine, |left, right| {
        if left == '\n' {
            on_starting_row = false;
        }
        (classifier.kind(left) != classifier.kind(right)
            && ((on_starting_row && !left.is_whitespace())
                || (!on_starting_row && !right.is_whitespace())))
            || right == '\n'
    })
}

pub fn previous_subword_start(map: &DisplaySnapshot, point: DisplayPoint) -> DisplayPoint {
    let raw_point = point.to_point(map);
    let classifier = map.buffer_snapshot().char_classifier_at(raw_point);

    find_preceding_boundary_display_point(map, point, FindRange::MultiLine, |left, right| {
        is_subword_start(left, right, &classifier) || left == '\n'
    })
}

pub fn previous_subword_start_or_newline(
    map: &DisplaySnapshot,
    point: DisplayPoint,
) -> DisplayPoint {
    let raw_point = point.to_point(map);
    let classifier = map.buffer_snapshot().char_classifier_at(raw_point);

    find_preceding_boundary_display_point(map, point, FindRange::MultiLine, |left, right| {
        is_subword_start(left, right, &classifier) || left == '\n' || right == '\n'
    })
}

pub fn next_subword_end(map: &DisplaySnapshot, point: DisplayPoint) -> DisplayPoint {
    let raw_point = point.to_point(map);
    let classifier = map.buffer_snapshot().char_classifier_at(raw_point);

    find_boundary(map, point, FindRange::MultiLine, |left, right| {
        is_subword_end(left, right, &classifier) || right == '\n'
    })
}

pub fn next_subword_end_or_newline(map: &DisplaySnapshot, point: DisplayPoint) -> DisplayPoint {
    let raw_point = point.to_point(map);
    let classifier = map.buffer_snapshot().char_classifier_at(raw_point);

    let mut on_starting_row = true;
    find_boundary(map, point, FindRange::MultiLine, |left, right| {
        if left == '\n' {
            on_starting_row = false;
        }
        ((classifier.kind(left) != classifier.kind(right)
            || is_subword_boundary_end(left, right, &classifier))
            && ((on_starting_row && !left.is_whitespace())
                || (!on_starting_row && !right.is_whitespace())))
            || right == '\n'
    })
}

pub fn adjust_greedy_deletion(
    map: &DisplaySnapshot,
    delete_from: DisplayPoint,
    delete_until: DisplayPoint,
    ignore_brackets: bool,
) -> DisplayPoint {
    if delete_from == delete_until {
        return delete_until;
    }
    let is_backward = delete_from > delete_until;
    let delete_range = if is_backward {
        delete_until.to_offset(map, Bias::Left)..delete_from.to_offset(map, Bias::Right)
    } else {
        delete_from.to_offset(map, Bias::Left)..delete_until.to_offset(map, Bias::Right)
    };

    let trimmed_delete_range = if ignore_brackets {
        delete_range.clone()
    } else {
        delete_range
    };

    let mut whitespace_sequences = Vec::new();
    let mut current_offset = trimmed_delete_range.start;
    let mut whitespace_sequence_length = MultiBufferOffset(0);
    let mut whitespace_sequence_start = MultiBufferOffset(0);
    for character in map
        .buffer_snapshot()
        .text_for_range(trimmed_delete_range.clone())
        .flat_map(str::chars)
    {
        if character.is_whitespace() {
            if whitespace_sequence_length == MultiBufferOffset(0) {
                whitespace_sequence_start = current_offset;
            }
            whitespace_sequence_length += 1;
        } else {
            if whitespace_sequence_length >= MultiBufferOffset(2) {
                whitespace_sequences.push((whitespace_sequence_start, current_offset));
            }
            whitespace_sequence_start = MultiBufferOffset(0);
            whitespace_sequence_length = MultiBufferOffset(0);
        }
        current_offset += character.len_utf8();
    }
    if whitespace_sequence_length >= MultiBufferOffset(2) {
        whitespace_sequences.push((whitespace_sequence_start, current_offset));
    }

    let closest_whitespace_end = if is_backward {
        whitespace_sequences.last().map(|&(start, _)| start)
    } else {
        whitespace_sequences.first().map(|&(_, end)| end)
    };

    closest_whitespace_end
        .unwrap_or_else(|| {
            if is_backward {
                trimmed_delete_range.start
            } else {
                trimmed_delete_range.end
            }
        })
        .to_display_point(map)
}

pub fn is_subword_start(left: char, right: char, classifier: &CharClassifier) -> bool {
    let is_word_start = classifier.kind(left) != classifier.kind(right) && !right.is_whitespace();
    let is_subword_start = classifier.is_word('-') && left == '-' && right != '-'
        || left == '_' && right != '_'
        || left.is_lowercase() && right.is_uppercase();
    is_word_start || is_subword_start
}

pub fn is_subword_end(left: char, right: char, classifier: &CharClassifier) -> bool {
    let is_word_end =
        classifier.kind(left) != classifier.kind(right) && !classifier.is_whitespace(left);
    is_word_end || is_subword_boundary_end(left, right, classifier)
}

fn is_subword_boundary_end(left: char, right: char, classifier: &CharClassifier) -> bool {
    classifier.is_word('-') && left != '-' && right == '-'
        || left != '_' && right == '_'
        || left.is_lowercase() && right.is_uppercase()
}

pub fn find_preceding_boundary_point(
    buffer_snapshot: &MultiBufferSnapshot,
    from: Point,
    find_range: FindRange,
    mut is_boundary: impl FnMut(char, char) -> bool,
) -> Point {
    let mut previous_character = None;
    let mut offset = buffer_snapshot.point_to_offset(from);

    for character in buffer_snapshot.reversed_chars_at(offset) {
        if find_range == FindRange::SingleLine && character == '\n' {
            break;
        }

        if let Some(previous_character) = previous_character
            && is_boundary(character, previous_character)
        {
            break;
        }

        offset -= character.len_utf8();
        previous_character = Some(character);
    }

    buffer_snapshot.offset_to_point(offset)
}

pub fn find_preceding_boundary_display_point(
    map: &DisplaySnapshot,
    from: DisplayPoint,
    find_range: FindRange,
    is_boundary: impl FnMut(char, char) -> bool,
) -> DisplayPoint {
    let result = find_preceding_boundary_point(
        map.buffer_snapshot(),
        from.to_point(map),
        find_range,
        is_boundary,
    );

    map.clip_point(result.to_display_point(map), Bias::Left)
}

pub fn find_boundary_point(
    map: &DisplaySnapshot,
    from: DisplayPoint,
    find_range: FindRange,
    mut is_boundary: impl FnMut(char, char) -> bool,
    return_point_before_boundary: bool,
) -> DisplayPoint {
    let mut offset = from.to_offset(map, Bias::Right);
    let mut previous_offset = offset;
    let mut previous_character = None;

    for character in map.buffer_snapshot().chars_at(offset) {
        if find_range == FindRange::SingleLine && character == '\n' {
            break;
        }

        if let Some(previous_character) = previous_character
            && is_boundary(previous_character, character)
        {
            if return_point_before_boundary {
                return map.clip_point(previous_offset.to_display_point(map), Bias::Right);
            }
            break;
        }

        previous_offset = offset;
        offset += character.len_utf8();
        previous_character = Some(character);
    }

    map.clip_point(offset.to_display_point(map), Bias::Right)
}

pub fn find_boundary(
    map: &DisplaySnapshot,
    from: DisplayPoint,
    find_range: FindRange,
    is_boundary: impl FnMut(char, char) -> bool,
) -> DisplayPoint {
    find_boundary_point(map, from, find_range, is_boundary, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    use indoc::indoc;

    use settings::SettingsStore;

    use crate::tests::util::marked_display_snapshot;

    fn init_test(cx: &mut gpui::App) {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        theme::init(theme::LoadThemes::JustBase, cx);
        crate::init(cx);
    }

    #[gpui::test]
    fn test_previous_word_start(cx: &mut gpui::App) {
        init_test(cx);

        fn assert_previous_word_start(marked_text: &str, cx: &mut gpui::App) {
            let (snapshot, display_points) = marked_display_snapshot(marked_text, cx);
            assert_eq!(
                display_points.len(),
                2,
                "expected exactly 2 markers in: {marked_text:?}"
            );
            let actual = previous_word_start(&snapshot, display_points[1]);
            let expected = display_points[0];
            if actual != expected {
                eprintln!(
                    "previous_word_start mismatch for '{marked_text}': actual={actual:?}, expected={expected:?}",
                );
            }
            assert_eq!(actual, expected);
        }

        assert_previous_word_start("ˇ   ˇquick", cx);
        assert_previous_word_start(
            indoc! {"
                ˇ
                ˇ   quick
            "},
            cx,
        );
        assert_previous_word_start("    ˇquickˇ", cx);
        assert_previous_word_start("ˇ    ˇquick", cx);
        assert_previous_word_start("    ˇquˇick", cx);
        assert_previous_word_start(
            indoc! {"
                quick
                ˇ   ˇbrown
            "},
            cx,
        );
        assert_previous_word_start(
            indoc! {"

                ˇ
                ˇ
            "},
            cx,
        );
        assert_previous_word_start("    ˇquick  ˇbrown", cx);
        assert_previous_word_start("ˇquick-ˇbrown", cx);
        assert_previous_word_start("quickˇ-#$@ˇbrown", cx);
        assert_previous_word_start("ˇquick_ˇbrown", cx);
        assert_previous_word_start(" ˇdefγˇ", cx);
        assert_previous_word_start(" ˇbcΔˇ", cx);
        assert_previous_word_start("ˇhello.ˇ", cx);
        assert_previous_word_start("helloˇ...ˇ", cx);
        assert_previous_word_start("helloˇ.---..ˇtest", cx);
        assert_previous_word_start("test  ˇ.--ˇtest", cx);
        assert_previous_word_start("oneˇ,;:!?ˇtwo", cx);
    }

    #[gpui::test]
    fn test_previous_subword_start(cx: &mut gpui::App) {
        init_test(cx);

        fn assert_previous_subword_start(marked_text: &str, cx: &mut gpui::App) {
            let (snapshot, display_points) = marked_display_snapshot(marked_text, cx);
            assert_eq!(
                display_points.len(),
                2,
                "expected exactly 2 markers in: {marked_text:?}"
            );
            assert_eq!(
                previous_subword_start(&snapshot, display_points[1]),
                display_points[0]
            );
        }

        assert_previous_subword_start("quick_ˇbrˇown", cx);
        assert_previous_subword_start("quick_ˇbrownˇ", cx);
        assert_previous_subword_start("ˇquick_ˇbrown", cx);
        assert_previous_subword_start("quick_ˇbrown_ˇfox", cx);
        assert_previous_subword_start("quickˇBrˇown", cx);
        assert_previous_subword_start("quickˇBrownˇ", cx);

        assert_previous_subword_start(
            indoc! {"
                ˇ   ˇquick
            "},
            cx,
        );
        assert_previous_subword_start("    ˇquickˇ", cx);
        assert_previous_subword_start("    ˇquˇick", cx);
        assert_previous_subword_start(
            indoc! {"
                quick
                ˇ   ˇbrown
            "},
            cx,
        );
        assert_previous_subword_start(
            indoc! {"

                ˇ
                ˇ
            "},
            cx,
        );
        assert_previous_subword_start("    ˇquick  ˇbrown", cx);
        assert_previous_subword_start("quickˇ-ˇbrown", cx);
        assert_previous_subword_start("quickˇ-#$@ˇbrown", cx);
        assert_previous_subword_start(" ˇdefγˇ", cx);
        assert_previous_subword_start(" bcˇΔˇ", cx);
        assert_previous_subword_start(" ˇbcδˇ", cx);
        assert_previous_subword_start(" abˇ——ˇcd", cx);
    }

    #[gpui::test]
    fn test_find_preceding_boundary(cx: &mut gpui::App) {
        init_test(cx);

        fn assert_preceding_boundary(
            marked_text: &str,
            cx: &mut gpui::App,
            is_boundary: &mut dyn FnMut(char, char) -> bool,
        ) {
            let (snapshot, display_points) = marked_display_snapshot(marked_text, cx);
            assert_eq!(
                display_points.len(),
                2,
                "expected exactly 2 markers in: {marked_text:?}"
            );
            assert_eq!(
                find_preceding_boundary_display_point(
                    &snapshot,
                    display_points[1],
                    FindRange::MultiLine,
                    is_boundary,
                ),
                display_points[0]
            );
        }

        assert_preceding_boundary(
            indoc! {"
                abcˇdef
                gh
                ijˇk
            "},
            cx,
            &mut |left, right| left == 'c' && right == 'd',
        );
        assert_preceding_boundary(
            indoc! {"
                abcdef
                ˇgh
                ijˇk
            "},
            cx,
            &mut |left, right| left == '\n' && right == 'g',
        );
        let mut line_count = 0;
        assert_preceding_boundary(
            indoc! {"
                abcdef
                ˇgh
                ijˇk
            "},
            cx,
            &mut |left, _| {
                if left == '\n' {
                    line_count += 1;
                    line_count == 2
                } else {
                    false
                }
            },
        );
    }

    #[gpui::test]
    fn test_next_word_end(cx: &mut gpui::App) {
        init_test(cx);

        fn assert_next_word_end(marked_text: &str, cx: &mut gpui::App) {
            let (snapshot, display_points) = marked_display_snapshot(marked_text, cx);
            assert_eq!(
                display_points.len(),
                2,
                "expected exactly 2 markers in: {marked_text:?}"
            );
            let actual = next_word_end(&snapshot, display_points[0]);
            let expected = display_points[1];
            if actual != expected {
                eprintln!(
                    "next_word_end mismatch for '{marked_text}': actual={actual:?}, expected={expected:?}",
                );
            }
            assert_eq!(actual, expected);
        }

        assert_next_word_end(
            indoc! {"
                ˇ   quickˇ
            "},
            cx,
        );
        assert_next_word_end("    ˇquickˇ", cx);
        assert_next_word_end("    quˇickˇ", cx);
        assert_next_word_end(
            indoc! {"
                quickˇ    ˇ
                brown
            "},
            cx,
        );
        assert_next_word_end(
            indoc! {"
                ˇ
                ˇ

            "},
            cx,
        );
        assert_next_word_end("quickˇ    brownˇ   ", cx);
        assert_next_word_end("quickˇ-brownˇ", cx);
        assert_next_word_end("quickˇ#$@-ˇbrown", cx);
        assert_next_word_end("quickˇ_brownˇ", cx);
        assert_next_word_end(" ˇbcΔˇ", cx);
        assert_next_word_end(" abˇ——ˇcd", cx);
        assert_next_word_end("ˇ.helloˇ", cx);
        assert_next_word_end("display_pointsˇ[0ˇ]", cx);
        assert_next_word_end("ˇ...ˇhello", cx);
        assert_next_word_end("helloˇ.---..ˇtest", cx);
        assert_next_word_end("testˇ.--ˇ test", cx);
        assert_next_word_end("oneˇ,;:!?ˇtwo", cx);
    }

    #[gpui::test]
    fn test_next_subword_end(cx: &mut gpui::App) {
        init_test(cx);

        fn assert_next_subword_end(marked_text: &str, cx: &mut gpui::App) {
            let (snapshot, display_points) = marked_display_snapshot(marked_text, cx);
            assert_eq!(
                display_points.len(),
                2,
                "expected exactly 2 markers in: {marked_text:?}"
            );
            assert_eq!(
                next_subword_end(&snapshot, display_points[0]),
                display_points[1]
            );
        }

        assert_next_subword_end("quˇickˇ_brown", cx);
        assert_next_subword_end("ˇquickˇ_brown", cx);
        assert_next_subword_end("quickˇ_brownˇ", cx);
        assert_next_subword_end("quickˇ_brownˇ_fox", cx);
        assert_next_subword_end("quˇickˇBrown", cx);
        assert_next_subword_end("quickˇBrownˇFox", cx);

        assert_next_subword_end(
            indoc! {"
                ˇ   quickˇ
            "},
            cx,
        );
        assert_next_subword_end("    ˇquickˇ", cx);
        assert_next_subword_end("    quˇickˇ", cx);
        assert_next_subword_end(
            indoc! {"
                quickˇ    ˇ
                brown
            "},
            cx,
        );
        assert_next_subword_end(
            indoc! {"
                ˇ
                ˇ

            "},
            cx,
        );
        assert_next_subword_end("quickˇ    brownˇ   ", cx);
        assert_next_subword_end("quickˇ-ˇbrown", cx);
        assert_next_subword_end("quickˇ#$@-ˇbrown", cx);
        assert_next_subword_end("quickˇ_brownˇ", cx);
        assert_next_subword_end(" ˇbcˇΔ", cx);
        assert_next_subword_end(" abˇ——ˇcd", cx);
    }

    #[gpui::test]
    fn test_find_boundary(cx: &mut gpui::App) {
        init_test(cx);

        fn assert_find_boundary(
            marked_text: &str,
            cx: &mut gpui::App,
            is_boundary: &mut dyn FnMut(char, char) -> bool,
        ) {
            let (snapshot, display_points) = marked_display_snapshot(marked_text, cx);
            assert_eq!(
                display_points.len(),
                2,
                "expected exactly 2 markers in: {marked_text:?}"
            );
            assert_eq!(
                find_boundary(
                    &snapshot,
                    display_points[0],
                    FindRange::MultiLine,
                    is_boundary,
                ),
                display_points[1]
            );
        }

        assert_find_boundary(
            indoc! {"
                abcˇdef
                gh
                ijˇk
            "},
            cx,
            &mut |left, right| left == 'j' && right == 'k',
        );
        assert_find_boundary(
            indoc! {"
                abˇcdef
                gh
                ˇijk
            "},
            cx,
            &mut |left, right| left == '\n' && right == 'i',
        );
        let mut line_count = 0;
        assert_find_boundary(
            indoc! {"
                abcˇdef
                gh
                ˇijk
            "},
            cx,
            &mut |left, _| {
                if left == '\n' {
                    line_count += 1;
                    line_count == 2
                } else {
                    false
                }
            },
        );
    }
}
