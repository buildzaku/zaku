use gpui::HighlightStyle;
use std::{cmp, collections::BTreeMap, ops::Range};

use language::{Chunk, LanguageAwareStyling};
use multi_buffer::{MultiBufferChunks, MultiBufferOffset, MultiBufferSnapshot};

use super::{HighlightKey, TextHighlights};

pub(super) struct CustomHighlightsChunks<'a> {
    buffer_chunks: MultiBufferChunks<'a>,
    buffer_chunk: Option<Chunk<'a>>,
    offset: MultiBufferOffset,
    highlight_endpoints: Vec<HighlightEndpoint>,
    active_highlights: BTreeMap<HighlightKey, HighlightStyle>,
}

impl<'a> CustomHighlightsChunks<'a> {
    pub(super) fn new(
        range: Range<MultiBufferOffset>,
        language_aware: LanguageAwareStyling,
        text_highlights: Option<&'a TextHighlights>,
        multibuffer_snapshot: &'a MultiBufferSnapshot,
    ) -> Self {
        Self {
            highlight_endpoints: create_highlight_endpoints(
                &range,
                text_highlights,
                multibuffer_snapshot,
            ),
            buffer_chunks: multibuffer_snapshot.chunks(range.clone(), language_aware),
            buffer_chunk: None,
            offset: range.start,
            active_highlights: BTreeMap::default(),
        }
    }
}

impl<'a> Iterator for CustomHighlightsChunks<'a> {
    type Item = Chunk<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut next_highlight_endpoint = MultiBufferOffset(usize::MAX);
        while let Some(endpoint) = self.highlight_endpoints.last().copied() {
            if endpoint.offset <= self.offset {
                if let Some(style) = endpoint.style {
                    self.active_highlights.insert(endpoint.tag, style);
                } else {
                    self.active_highlights.remove(&endpoint.tag);
                }
                self.highlight_endpoints.pop();
            } else {
                next_highlight_endpoint = endpoint.offset;
                break;
            }
        }

        let chunk = match &mut self.buffer_chunk {
            Some(chunk) => chunk,
            slot => slot.insert(self.buffer_chunks.next()?),
        };
        while chunk.text.is_empty() {
            *chunk = self.buffer_chunks.next()?;
        }

        let split_index = chunk.text.len().min(next_highlight_endpoint - self.offset);
        let (prefix, suffix) = chunk.text.split_at(split_index);
        self.offset += prefix.len();

        let shift = u32::try_from(split_index).expect("split index should fit in u32");
        let mask = 1u128.unbounded_shl(shift).wrapping_sub(1);
        let chars = chunk.chars & mask;
        let tabs = chunk.tabs & mask;
        let newlines = chunk.newlines & mask;
        let mut prefix = Chunk {
            text: prefix,
            chars,
            tabs,
            newlines,
            ..chunk.clone()
        };

        chunk.chars = chunk.chars.unbounded_shr(shift);
        chunk.tabs = chunk.tabs.unbounded_shr(shift);
        chunk.newlines = chunk.newlines.unbounded_shr(shift);
        chunk.text = suffix;
        for style in self.active_highlights.values() {
            prefix.highlight_style = Some(match prefix.highlight_style {
                Some(active_highlight) => active_highlight.highlight(*style),
                None => *style,
            });
        }
        Some(prefix)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HighlightEndpoint {
    offset: MultiBufferOffset,
    tag: HighlightKey,
    style: Option<HighlightStyle>,
}

impl PartialOrd for HighlightEndpoint {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HighlightEndpoint {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.offset
            .cmp(&other.offset)
            .then_with(|| self.style.is_some().cmp(&other.style.is_some()))
            .then_with(|| self.tag.cmp(&other.tag))
    }
}

fn create_highlight_endpoints(
    range: &Range<MultiBufferOffset>,
    text_highlights: Option<&TextHighlights>,
    buffer: &MultiBufferSnapshot,
) -> Vec<HighlightEndpoint> {
    let mut highlight_endpoints = Vec::new();
    if let Some(text_highlights) = text_highlights {
        let start = buffer.anchor_after(&range.start);
        let end = buffer.anchor_after(&range.end);
        let mut text_highlights_scratch = Vec::new();

        for (&tag, text_highlights) in text_highlights.iter() {
            let style = text_highlights.0;
            let ranges = &text_highlights.1;

            let start_index = ranges
                .binary_search_by(|probe| probe.end.cmp(&start, buffer).then(cmp::Ordering::Less))
                .unwrap_or_else(|index| index);
            let ranges = ranges
                .get(start_index..)
                .expect("highlight start index should be in bounds");
            let end_index = ranges
                .binary_search_by(|probe| {
                    probe.start.cmp(&end, buffer).then(cmp::Ordering::Greater)
                })
                .unwrap_or_else(|index| index);
            let ranges = ranges
                .get(..end_index)
                .expect("highlight end index should be in bounds");

            text_highlights_scratch.clear();
            text_highlights_scratch.reserve(ranges.len());
            highlight_endpoints.reserve(2 * ranges.len());

            let mut remaining_ranges = ranges.iter();
            buffer.for_each_summary_for_anchors(
                ranges.iter().map(|range| &range.start),
                |start: MultiBufferOffset| {
                    let range = remaining_ranges
                        .next()
                        .expect("highlight range should exist for each start");
                    text_highlights_scratch.push((start, range.end));
                },
            );
            text_highlights_scratch.sort_by(|left, right| left.1.cmp(&right.1, buffer));
            let mut remaining_starts = text_highlights_scratch.iter();
            buffer.for_each_summary_for_anchors(
                text_highlights_scratch.iter().map(|(_, end)| end),
                |end: MultiBufferOffset| {
                    let (start, _) = remaining_starts
                        .next()
                        .expect("highlight start should exist for each end");
                    if *start == end {
                        return;
                    }
                    highlight_endpoints.push(HighlightEndpoint {
                        offset: *start,
                        tag,
                        style: Some(style),
                    });
                    highlight_endpoints.push(HighlightEndpoint {
                        offset: end,
                        tag,
                        style: None,
                    });
                },
            );
        }
    }
    highlight_endpoints.sort_by(|left, right| left.cmp(right).reverse());
    highlight_endpoints
}

#[cfg(test)]
mod tests {
    use super::*;

    use gpui::App;
    use indoc::indoc;
    use std::{collections::HashMap, sync::Arc};

    use multi_buffer::MultiBuffer;
    use util::test::{TextRangeMarker, marked_text_ranges_by};

    fn text_highlights_for_ranges(
        buffer_snapshot: &MultiBufferSnapshot,
        highlighted_ranges: &[Range<usize>],
        highlight_style: HighlightStyle,
    ) -> TextHighlights {
        let ranges = highlighted_ranges
            .iter()
            .map(|range| {
                buffer_snapshot.anchor_before(&MultiBufferOffset(range.start))
                    ..buffer_snapshot.anchor_after(&MultiBufferOffset(range.end))
            })
            .collect();
        Arc::new(HashMap::from_iter([(
            HighlightKey::InputComposition,
            Arc::new((highlight_style, ranges)),
        )]))
    }

    #[gpui::test]
    fn test_chunks_with_range_inside_text_highlights(cx: &mut App) {
        let highlight_marker = TextRangeMarker::from(('«', '»'));
        let visible_marker = TextRangeMarker::from(('[', ']'));
        let (text, mut marked_ranges) = marked_text_ranges_by(
            indoc! {r#"
                {
                  "name": "«{{webhook_name}}»",
                  "url": "«{{base_[url}}»/hooks/«{{hook_id}}»",]
                  "secret": "«{{webhook_secret}}»"
                }"#},
            vec![highlight_marker.clone(), visible_marker.clone()],
        );
        let highlighted_ranges = marked_ranges.remove(&highlight_marker).unwrap();
        let visible_range = marked_ranges.remove(&visible_marker).unwrap().remove(0);
        let highlight_style = HighlightStyle {
            color: Some(gpui::green()),
            ..Default::default()
        };
        let buffer = MultiBuffer::build_simple(&text, cx);
        let buffer_snapshot = buffer.read(cx).snapshot(cx);
        let text_highlights =
            text_highlights_for_ranges(&buffer_snapshot, &highlighted_ranges, highlight_style);

        let chunks = CustomHighlightsChunks::new(
            MultiBufferOffset(visible_range.start)..MultiBufferOffset(visible_range.end),
            LanguageAwareStyling {
                tree_sitter: false,
                diagnostics: false,
            },
            Some(&text_highlights),
            &buffer_snapshot,
        );

        assert_eq!(
            chunks
                .map(|chunk| (chunk.text, chunk.highlight_style))
                .collect::<Vec<_>>(),
            [
                ("url}}", Some(highlight_style)),
                ("/hooks/", None),
                ("{{hook_id}}", Some(highlight_style)),
                ("\",", None),
            ]
        );
    }
}
