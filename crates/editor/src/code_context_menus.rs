use fuzzy::{StringMatch, StringMatchCandidate};
use gpui::{
    AnyElement, Context, Entity, FontWeight, ListSizingBehavior, Pixels, ScrollHandle,
    ScrollStrategy, Size, StyledText, Task, UniformListScrollHandle, Window, prelude::*,
};
use itertools::Itertools;
use num_traits::ToPrimitive;
use ordered_float::OrderedFloat;
use std::{
    cell::RefCell,
    cmp::Reverse,
    ops::Range,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use actions::editor::ConfirmCompletion;
use language::Buffer;
use multi_buffer::Anchor;
use project::Completion;
use ui::{
    Color, ListItem, Popover, ScrollAxes, Scrollbars, Text, TextCommon, TextSize, Toggleable,
    WithScrollbar,
};

use crate::{CompletionId, Editor, EditorStyle, completions::split_words};

pub(crate) const MENU_GAP: Pixels = gpui::px(4.0);
pub(crate) const MENU_ASIDE_X_PADDING: Pixels = gpui::px(16.0);
pub(crate) const MENU_ASIDE_MIN_WIDTH: Pixels = gpui::px(260.0);
pub(crate) const MENU_ASIDE_MAX_WIDTH: Pixels = gpui::px(500.0);
const COMPLETION_MENU_MIN_WIDTH: Pixels = gpui::px(280.0);
const COMPLETION_MENU_MAX_WIDTH: Pixels = gpui::px(540.0);

pub(crate) struct CompletionsMenu {
    pub(crate) id: CompletionId,
    pub(crate) initial_position: Anchor,
    pub(crate) initial_query: Option<Arc<String>>,
    pub(crate) buffer: Entity<Buffer>,
    pub(crate) completions: Rc<RefCell<Box<[Completion]>>>,
    match_candidates: Arc<[(Option<text::Anchor>, Vec<StringMatchCandidate>)]>,
    pub(crate) entries: Rc<RefCell<Box<[StringMatch]>>>,
    pub(crate) selected_item: usize,
    filter_task: Task<()>,
    cancel_filter: Arc<AtomicBool>,
    pub(crate) scroll_handle: UniformListScrollHandle,
    scroll_handle_aside: ScrollHandle,
}

impl Drop for CompletionsMenu {
    fn drop(&mut self) {
        self.cancel_filter.store(true, Ordering::Relaxed);
    }
}

impl CompletionsMenu {
    pub(crate) fn new(
        id: CompletionId,
        initial_position: Anchor,
        initial_query: Option<Arc<String>>,
        buffer: Entity<Buffer>,
        completions: Box<[Completion]>,
        scroll_handle: Option<UniformListScrollHandle>,
    ) -> Self {
        let match_candidates = completions
            .iter()
            .enumerate()
            .map(|(id, completion)| StringMatchCandidate::new(id, completion.filter_text()))
            .into_group_map_by(|candidate| {
                completions
                    .get(candidate.id)
                    .and_then(|completion| completion.match_start)
            })
            .into_iter()
            .collect();

        Self {
            id,
            initial_position,
            initial_query,
            buffer,
            completions: RefCell::new(completions).into(),
            match_candidates,
            entries: Rc::new(RefCell::new(Box::new([]))),
            selected_item: 0,
            filter_task: Task::ready(()),
            cancel_filter: Arc::new(AtomicBool::new(false)),
            scroll_handle: scroll_handle.unwrap_or_default(),
            scroll_handle_aside: ScrollHandle::new(),
        }
    }

    pub(crate) fn select_first(&mut self, cx: &mut Context<Editor>) {
        if self.entries.borrow().is_empty() {
            return;
        }
        self.update_selection_index(0, cx);
    }

    pub(crate) fn select_last(&mut self, cx: &mut Context<Editor>) {
        let len = self.entries.borrow().len();
        if len == 0 {
            return;
        }
        self.update_selection_index(len - 1, cx);
    }

    pub(crate) fn select_prev(&mut self, cx: &mut Context<Editor>) {
        let index = self.prev_match_index();
        self.update_selection_index(index, cx);
    }

    pub(crate) fn select_next(&mut self, cx: &mut Context<Editor>) {
        let index = self.next_match_index();
        self.update_selection_index(index, cx);
    }

    fn update_selection_index(&mut self, match_index: usize, cx: &mut Context<Editor>) {
        if self.selected_item != match_index {
            self.selected_item = match_index;
            self.handle_selection_changed(cx);
        }
    }

    fn prev_match_index(&self) -> usize {
        let len = self.entries.borrow().len();
        if len == 0 {
            return 0;
        }
        if self.selected_item > 0 {
            self.selected_item - 1
        } else {
            len - 1
        }
    }

    fn next_match_index(&self) -> usize {
        let len = self.entries.borrow().len();
        if len == 0 {
            return 0;
        }
        if self.selected_item + 1 < len {
            self.selected_item + 1
        } else {
            0
        }
    }

    fn handle_selection_changed(&mut self, cx: &mut Context<Editor>) {
        self.scroll_handle
            .scroll_to_item(self.selected_item, ScrollStrategy::Nearest);
        cx.notify();
    }

    pub(crate) fn visible(&self) -> bool {
        !self.entries.borrow().is_empty()
    }

    pub(crate) fn render(
        &self,
        style: &EditorStyle,
        max_height_in_lines: u32,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> AnyElement {
        let selected_item = self.selected_item;
        let completions = self.completions.clone();
        let entries = self.entries.clone();
        let style = style.clone();
        let list = gpui::uniform_list(
            "completions",
            self.entries.borrow().len(),
            cx.processor(move |_editor, range: Range<usize>, _window, cx| {
                let completions_guard = completions.borrow();
                let entries_guard = entries.borrow();

                entries_guard
                    .get(range.clone())
                    .unwrap_or_default()
                    .iter()
                    .enumerate()
                    .map(|(index, string_match)| {
                        let item_index = range.start + index;
                        let completion = completions_guard
                            .get(string_match.candidate_id)
                            .expect("completion match should have a candidate");

                        let filter_start = completion.label.filter_range.start;
                        let highlights = string_match
                            .ranges()
                            .map(|range| {
                                (
                                    filter_start + range.start..filter_start + range.end,
                                    FontWeight::BOLD.into(),
                                )
                            })
                            .collect::<Vec<_>>();
                        let label = StyledText::new(completion.label.text.clone())
                            .with_default_highlights(&style.text, highlights);

                        let documentation_label = completion
                            .documentation
                            .as_ref()
                            .map(|documentation| documentation.single_line.trim())
                            .filter(|text| !text.is_empty())
                            .map(|text| {
                                Text::new(text.to_string())
                                    .ml_4()
                                    .size(TextSize::Small)
                                    .color(Color::Muted)
                            });

                        gpui::div()
                            .min_w(COMPLETION_MENU_MIN_WIDTH)
                            .max_w(COMPLETION_MENU_MAX_WIDTH)
                            .child(
                                ListItem::new(string_match.candidate_id)
                                    .inset(true)
                                    .toggle_state(item_index == selected_item)
                                    .on_click(cx.listener(move |editor, _event, window, cx| {
                                        cx.stop_propagation();
                                        editor.confirm_completion(
                                            &ConfirmCompletion {
                                                item_index: Some(item_index),
                                            },
                                            window,
                                            cx,
                                        );
                                    }))
                                    .child(gpui::div().flex().items_center().flex_grow_1().child(
                                        gpui::div().flex_none().whitespace_nowrap().child(label),
                                    ))
                                    .when_some(documentation_label, |this, documentation| {
                                        this.child(
                                            gpui::div()
                                                .flex_shrink(1.0)
                                                .ml_auto()
                                                .min_w_0()
                                                .child(documentation.truncate()),
                                        )
                                    }),
                            )
                            .into_any_element()
                    })
                    .collect()
            }),
        )
        .occlude()
        .max_h(
            max_height_in_lines
                .to_f32()
                .expect("max height in lines should fit in f32")
                * window.line_height(),
        )
        .track_scroll(&self.scroll_handle)
        .with_sizing_behavior(ListSizingBehavior::Infer)
        .w(gpui::rems(34.0));

        Popover::new()
            .child(gpui::div().child(list).custom_scrollbars(
                Scrollbars::new(ScrollAxes::Vertical).tracked_scroll_handle(&self.scroll_handle),
                window,
                cx,
            ))
            .into_any_element()
    }

    pub(crate) fn render_aside(&mut self, max_size: Size<Pixels>) -> Option<AnyElement> {
        let entries = self.entries.borrow();
        let string_match = entries.get(self.selected_item)?;
        let completions = self.completions.borrow();
        let completion = completions.get(string_match.candidate_id)?;
        let multiline_docs =
            gpui::div().child(completion.documentation.as_ref()?.plain_text.clone());

        Some(
            Popover::new()
                .child(
                    multiline_docs
                        .id("multiline_docs")
                        .px(MENU_ASIDE_X_PADDING / 2.0)
                        .max_w(max_size.width)
                        .max_h(max_size.height)
                        .overflow_y_scroll()
                        .track_scroll(&self.scroll_handle_aside)
                        .occlude(),
                )
                .into_any_element(),
        )
    }

    pub(crate) fn filter(
        &mut self,
        query: Arc<String>,
        query_end: text::Anchor,
        buffer: &Entity<Buffer>,
        cx: &mut Context<Editor>,
    ) {
        self.cancel_filter.store(true, Ordering::Relaxed);
        self.cancel_filter = Arc::new(AtomicBool::new(false));
        let matches = self.do_async_filtering(query, query_end, buffer, cx);
        let id = self.id;
        self.filter_task = cx.spawn(async move |editor, cx| {
            let matches = matches.await;
            if let Err(error) = editor.update(cx, |editor, cx| {
                editor.with_completions_menu_matching_id(id, |this| {
                    if let Some(this) = this {
                        this.set_filter_results(matches, cx);
                    }
                });
            }) {
                log::debug!("Failed to update completions menu: {error:?}");
            }
        });
    }

    pub(crate) fn do_async_filtering(
        &self,
        query: Arc<String>,
        query_end: text::Anchor,
        buffer: &Entity<Buffer>,
        cx: &Context<Editor>,
    ) -> Task<Vec<StringMatch>> {
        let buffer_snapshot = buffer.read(cx).snapshot();
        let background_executor = cx.background_executor().clone();
        let match_candidates = self.match_candidates.clone();
        let cancel_filter = self.cancel_filter.clone();
        let default_query = query.clone();

        let matches_task = cx.background_spawn(async move {
            let queries_and_candidates = match_candidates
                .iter()
                .map(|(query_start, candidates)| {
                    let query_for_batch = match query_start {
                        Some(start) => {
                            Arc::new(buffer_snapshot.text_for_range(*start..query_end).collect())
                        }
                        None => default_query.clone(),
                    };
                    (query_for_batch, candidates)
                })
                .collect_vec();

            let mut filter_match_results = vec![];
            for (query, match_candidates) in queries_and_candidates {
                let smart_case = query.chars().any(|character| character.is_uppercase());
                let filter_matches = fuzzy::match_strings(
                    match_candidates,
                    &query,
                    smart_case,
                    false,
                    1000,
                    &cancel_filter,
                    background_executor.clone(),
                )
                .await;
                filter_match_results.extend(filter_matches);
            }
            filter_match_results
        });

        let completions = self.completions.clone();
        cx.foreground_executor().spawn(async move {
            let matches = matches_task.await;
            let completions_ref = completions.borrow();
            Self::sort_string_matches(matches, Some(query.as_str()), &completions_ref)
        })
    }

    pub(crate) fn set_filter_results(
        &mut self,
        matches: Vec<StringMatch>,
        cx: &mut Context<Editor>,
    ) {
        *self.entries.borrow_mut() = matches.into_boxed_slice();
        self.selected_item = 0;
        self.handle_selection_changed(cx);
    }

    pub(crate) fn sort_string_matches(
        matches: Vec<StringMatch>,
        query: Option<&str>,
        completions: &[Completion],
    ) -> Vec<StringMatch> {
        #[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
        enum MatchTier<'a> {
            WordStartMatch {
                sort_exact: Reverse<i32>,
                sort_score: Reverse<OrderedFloat<f64>>,
                sort_positions: Vec<usize>,
                sort_exact_case_matches: Reverse<usize>,
                sort_label: &'a str,
            },
            OtherMatch {
                sort_score: Reverse<OrderedFloat<f64>>,
            },
        }

        let mut matches = matches;

        let query_start_lower = query
            .as_ref()
            .and_then(|query| query.chars().next())
            .and_then(|character| character.to_lowercase().next());

        matches.sort_unstable_by_key(|string_match| {
            let completion = completions
                .get(string_match.candidate_id)
                .expect("completion match should have a candidate");
            let sort_score = Reverse(OrderedFloat(string_match.score));

            let query_start_doesnt_match_split_words =
                query_start_lower.is_some_and(|query_character| {
                    !split_words(&string_match.string).any(|word| {
                        word.chars()
                            .next()
                            .and_then(|character| character.to_lowercase().next())
                            == Some(query_character)
                    })
                });

            if query_start_doesnt_match_split_words {
                MatchTier::OtherMatch { sort_score }
            } else {
                let sort_positions = string_match.positions.clone();
                let sort_exact_case_matches = Reverse(exact_case_match_count(
                    query.unwrap_or_default(),
                    string_match,
                ));
                let sort_exact = Reverse(i32::from(Some(completion.filter_text()) == query));

                MatchTier::WordStartMatch {
                    sort_exact,
                    sort_score,
                    sort_positions,
                    sort_exact_case_matches,
                    sort_label: completion.label.filter_text(),
                }
            }
        });

        matches
    }
}

fn exact_case_match_count(query: &str, string_match: &StringMatch) -> usize {
    let mut exact_matches = 0;
    let mut query_chars = query.chars();
    let mut next_query_char = query_chars.next();
    let mut matched_positions = string_match.positions.iter().copied().peekable();

    for (index, candidate_char) in string_match.string.char_indices() {
        if matched_positions.peek() == Some(&index) {
            let Some(query_char) = next_query_char else {
                break;
            };

            if query_char == candidate_char {
                exact_matches += 1;
            }

            matched_positions.next();
            next_query_char = query_chars.next();
        }
    }

    exact_matches
}

#[cfg(test)]
mod tests {
    use super::*;

    use gpui::TestAppContext;
    use text::BufferId;

    use language::CodeLabel;

    struct CompletionBuilder;

    impl CompletionBuilder {
        fn variable(label: &str) -> Completion {
            Completion {
                replace_range: text::Anchor::min_max_range_for_buffer(BufferId::new(1).unwrap()),
                new_text: label.to_string(),
                label: CodeLabel::plain(label.to_string(), None),
                documentation: None,
                match_start: None,
            }
        }
    }

    async fn filter_and_sort_matches(
        query: &str,
        completions: &[Completion],
        cx: &mut TestAppContext,
    ) -> Vec<StringMatch> {
        let candidates: Arc<[StringMatchCandidate]> = completions
            .iter()
            .enumerate()
            .map(|(id, completion)| StringMatchCandidate::new(id, completion.filter_text()))
            .collect();
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let background_executor = cx.executor();
        let matches = fuzzy::match_strings(
            &candidates,
            query,
            query.chars().any(|character| character.is_uppercase()),
            false,
            100,
            &cancel_flag,
            background_executor,
        )
        .await;
        CompletionsMenu::sort_string_matches(matches, Some(query), completions)
    }

    #[gpui::test]
    async fn test_sort_exact(cx: &mut TestAppContext) {
        let completions = vec![
            CompletionBuilder::variable("token"),
            CompletionBuilder::variable("access_token"),
            CompletionBuilder::variable("refresh_token"),
            CompletionBuilder::variable("token_url"),
        ];
        let matches = filter_and_sort_matches("token", &completions, cx).await;
        assert_eq!(matches[0].string, "token");
    }

    #[gpui::test]
    async fn test_sort_positions(cx: &mut TestAppContext) {
        let completions = vec![
            CompletionBuilder::variable("db-host"),
            CompletionBuilder::variable("db-primary-host"),
            CompletionBuilder::variable("db-replica-host"),
        ];

        let matches = filter_and_sort_matches("db-host", &completions, cx).await;
        assert_eq!(matches[0].string, "db-host");

        let matches = filter_and_sort_matches("dbhost", &completions, cx).await;
        assert_eq!(matches[0].string, "db-host");
    }

    #[gpui::test]
    async fn test_case_sensitive_match_tie_breaker(cx: &mut TestAppContext) {
        let completions = vec![
            CompletionBuilder::variable("api_key"),
            CompletionBuilder::variable("API_KEY"),
        ];

        let matches = filter_and_sort_matches("a", &completions, cx).await;
        assert_eq!(
            matches
                .iter()
                .map(|string_match| string_match.string.as_str())
                .collect::<Vec<_>>(),
            vec!["api_key", "API_KEY"]
        );

        let matches = filter_and_sort_matches("A", &completions, cx).await;
        assert_eq!(
            matches
                .iter()
                .map(|string_match| string_match.string.as_str())
                .collect::<Vec<_>>(),
            vec!["API_KEY", "api_key"]
        );

        let matches = filter_and_sort_matches("ap", &completions, cx).await;
        assert_eq!(
            matches
                .iter()
                .map(|string_match| string_match.string.as_str())
                .collect::<Vec<_>>(),
            vec!["api_key", "API_KEY"]
        );

        let matches = filter_and_sort_matches("AP", &completions, cx).await;
        assert_eq!(
            matches
                .iter()
                .map(|string_match| string_match.string.as_str())
                .collect::<Vec<_>>(),
            vec!["API_KEY", "api_key"]
        );

        let completions = vec![
            CompletionBuilder::variable("apiKey"),
            CompletionBuilder::variable("ApiKey"),
        ];

        let matches = filter_and_sort_matches("Ap", &completions, cx).await;
        assert_eq!(
            matches
                .iter()
                .map(|string_match| string_match.string.as_str())
                .collect::<Vec<_>>(),
            vec!["ApiKey", "apiKey"]
        );

        let matches = filter_and_sort_matches("ap", &completions, cx).await;
        assert_eq!(
            matches
                .iter()
                .map(|string_match| string_match.string.as_str())
                .collect::<Vec<_>>(),
            vec!["apiKey", "ApiKey"]
        );
    }

    #[gpui::test]
    async fn test_fuzzy_over_sort_positions(cx: &mut TestAppContext) {
        let completions = vec![
            CompletionBuilder::variable("full_request_log"),
            CompletionBuilder::variable("default_retry_limit"),
            CompletionBuilder::variable("api_url"),
            CompletionBuilder::variable("base_url"),
            CompletionBuilder::variable("callback_url"),
        ];

        let matches = filter_and_sort_matches("url", &completions, cx).await;

        assert_eq!(matches[0].string, "api_url");
        assert_eq!(matches[1].string, "base_url");
        assert_eq!(matches[2].string, "callback_url");
    }
}
