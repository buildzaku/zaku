use std::{cmp::Ordering, collections::HashMap, ops::Range};

/// Build a string and offsets from embedded position markers.
pub fn marked_text_offsets_by(
    marked_text: &str,
    markers: &[char],
) -> (String, HashMap<char, Vec<usize>>) {
    let mut extracted_markers: HashMap<char, Vec<usize>> = HashMap::default();
    let mut unmarked_text = String::new();

    for character in marked_text.chars() {
        if markers.contains(&character) {
            let character_offsets = extracted_markers.entry(character).or_default();
            character_offsets.push(unmarked_text.len());
        } else {
            unmarked_text.push(character);
        }
    }

    (unmarked_text, extracted_markers)
}

/// Build a string and ranges from embedded range markers.
/// Ranges are grouped by the marker characters used.
pub fn marked_text_ranges_by(
    marked_text: &str,
    markers: Vec<TextRangeMarker>,
) -> (String, HashMap<TextRangeMarker, Vec<Range<usize>>>) {
    let all_markers: Vec<_> = markers.iter().flat_map(TextRangeMarker::markers).collect();

    let (unmarked_text, mut marker_offsets) = marked_text_offsets_by(marked_text, &all_markers);
    let range_lookup = markers
        .into_iter()
        .map(|marker| {
            (
                marker.clone(),
                match marker {
                    TextRangeMarker::Empty(empty_marker_char) => marker_offsets
                        .remove(&empty_marker_char)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|empty_index| empty_index..empty_index)
                        .collect::<Vec<Range<usize>>>(),
                    TextRangeMarker::Range(start_marker, end_marker) => {
                        let starts = marker_offsets.remove(&start_marker).unwrap_or_default();
                        let ends = marker_offsets.remove(&end_marker).unwrap_or_default();
                        assert_eq!(starts.len(), ends.len(), "marked ranges are unbalanced");
                        starts
                            .into_iter()
                            .zip(ends)
                            .map(|(start, end)| {
                                assert!(end >= start, "marked ranges must be disjoint");
                                start..end
                            })
                            .collect::<Vec<Range<usize>>>()
                    }
                    TextRangeMarker::ReverseRange(start_marker, end_marker) => {
                        let starts = marker_offsets.remove(&start_marker).unwrap_or_default();
                        let ends = marker_offsets.remove(&end_marker).unwrap_or_default();
                        assert_eq!(starts.len(), ends.len(), "marked ranges are unbalanced");
                        starts
                            .into_iter()
                            .zip(ends)
                            .map(|(start, end)| {
                                assert!(end >= start, "marked ranges must be disjoint");
                                end..start
                            })
                            .collect::<Vec<Range<usize>>>()
                    }
                },
            )
        })
        .collect();

    (unmarked_text, range_lookup)
}

/// Build a string and ranges from embedded markers in a single string.
/// Supports `«»` for ranges, `ˇ` for points/direction, and replaces `•` with spaces.
#[track_caller]
pub fn marked_text_ranges(
    marked_text: &str,
    ranges_are_directed: bool,
) -> (String, Vec<Range<usize>>) {
    let mut unmarked_text = String::with_capacity(marked_text.len());
    let mut ranges = Vec::new();
    let mut is_range_open = false;
    let mut range_start = 0;
    let mut range_cursor = None;

    let marked_text = marked_text.replace('•', " ");
    for (marked_index, marker) in marked_text.char_indices() {
        let unmarked_len = unmarked_text.len();

        match marker {
            'ˇ' => {
                if is_range_open {
                    assert!(
                        range_cursor.is_none(),
                        "duplicate point marker 'ˇ' at index {marked_index}"
                    );

                    range_cursor = Some(unmarked_len);
                } else {
                    ranges.push(unmarked_len..unmarked_len);
                }
            }
            '«' => {
                assert!(
                    !is_range_open,
                    "unexpected range start marker '«' at index {marked_index}"
                );

                is_range_open = true;
                range_start = unmarked_len;
                range_cursor = None;
            }
            '»' => {
                assert!(
                    is_range_open,
                    "unexpected range end marker '»' at index {marked_index}"
                );

                is_range_open = false;
                let mut reversed = false;
                if let Some(range_cursor) = range_cursor.take() {
                    if range_cursor == range_start {
                        reversed = true;
                    } else {
                        assert_eq!(
                            range_cursor, unmarked_len,
                            "unexpected 'ˇ' marker in the middle of a range"
                        );
                    }
                } else {
                    assert!(
                        !ranges_are_directed,
                        "missing 'ˇ' marker to indicate range direction"
                    );
                }

                ranges.push(if reversed {
                    unmarked_len..range_start
                } else {
                    range_start..unmarked_len
                });
            }
            _ => unmarked_text.push(marker),
        }
    }

    (unmarked_text, ranges)
}

/// Build a string and point offsets from embedded `ˇ` markers.
#[track_caller]
pub fn marked_text_offsets(marked_text: &str) -> (String, Vec<usize>) {
    let (text, ranges) = marked_text_ranges(marked_text, false);
    (
        text,
        ranges
            .into_iter()
            .map(|range| {
                assert_eq!(range.start, range.end);
                range.start
            })
            .collect(),
    )
}

/// Insert markers into text based on ranges.
pub fn generate_marked_text(
    unmarked_text: &str,
    ranges: &[Range<usize>],
    indicate_cursors: bool,
) -> String {
    let mut marked_text = unmarked_text.to_string();
    for range in ranges.iter().rev() {
        if indicate_cursors {
            match range.start.cmp(&range.end) {
                Ordering::Less => {
                    marked_text.insert_str(range.end, "ˇ»");
                    marked_text.insert(range.start, '«');
                }
                Ordering::Equal => {
                    marked_text.insert(range.start, 'ˇ');
                }
                Ordering::Greater => {
                    marked_text.insert(range.start, '»');
                    marked_text.insert_str(range.end, "«ˇ");
                }
            }
        } else if range.start.cmp(&range.end) == Ordering::Equal {
            marked_text.insert(range.start, 'ˇ');
        } else {
            marked_text.insert(range.end, '»');
            marked_text.insert(range.start, '«');
        }
    }
    marked_text
}

#[derive(Clone, Eq, PartialEq, Hash)]
pub enum TextRangeMarker {
    Empty(char),
    Range(char, char),
    ReverseRange(char, char),
}

impl TextRangeMarker {
    fn markers(&self) -> Vec<char> {
        match self {
            Self::Empty(marker) => vec![*marker],
            Self::Range(start_marker, end_marker)
            | Self::ReverseRange(start_marker, end_marker) => {
                vec![*start_marker, *end_marker]
            }
        }
    }
}

impl From<char> for TextRangeMarker {
    fn from(marker: char) -> Self {
        Self::Empty(marker)
    }
}

impl From<(char, char)> for TextRangeMarker {
    fn from((start_marker, end_marker): (char, char)) -> Self {
        Self::Range(start_marker, end_marker)
    }
}

#[cfg(test)]
mod tests {
    use super::{generate_marked_text, marked_text_ranges};

    #[allow(clippy::reversed_empty_ranges)]
    #[test]
    fn test_marked_text() {
        let (text, ranges) = marked_text_ranges("one «ˇtwo» «threeˇ» «ˇfour» fiveˇ six", true);

        assert_eq!(text, "one two three four five six");
        assert_eq!(ranges.len(), 4);
        assert_eq!(ranges[0], 7..4);
        assert_eq!(ranges[1], 8..13);
        assert_eq!(ranges[2], 18..14);
        assert_eq!(ranges[3], 23..23);

        assert_eq!(
            generate_marked_text(&text, &ranges, true),
            "one «ˇtwo» «threeˇ» «ˇfour» fiveˇ six"
        );
    }
}
