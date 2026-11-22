use gpui::{Context, Entity, Task, Window};
use std::{rc::Rc, sync::Arc};

use actions::editor::{ConfirmCompletion, ShowCompletions};
use language::Buffer;
use multi_buffer::{CharKind, CharScopeContext, MultiBufferSnapshot, ToOffset};
use project::Completion;
use util::ResultExt;

use crate::{
    CompletionId, Editor, SelectionEffects, code_context_menus::CompletionsMenu, hover_popover,
};

impl Editor {
    pub fn set_completion_provider(&mut self, provider: Option<Rc<dyn CompletionProvider>>) {
        self.completion_provider = provider;
    }

    pub fn show_completions(
        &mut self,
        _: &ShowCompletions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_or_update_completions_menu(None, window, cx);
    }

    pub fn confirm_completion(
        &mut self,
        action: &ConfirmCompletion,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<()> {
        if self.read_only(cx) {
            return None;
        }
        self.do_completion(action.item_index, window, cx)
    }

    pub fn has_visible_completions_menu(&self) -> bool {
        self.context_menu
            .borrow()
            .as_ref()
            .is_some_and(|menu| menu.visible())
    }

    pub(crate) fn trigger_completion_on_input(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_or_update_completions_menu(
            Some(text).filter(|text| !text.is_empty()),
            window,
            cx,
        );
    }

    pub(crate) fn open_or_update_completions_menu(
        &mut self,
        trigger: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let multibuffer_snapshot = self.buffer.read(cx).read(cx);
        let position = self
            .selections
            .newest_anchor()
            .head()
            .bias_right(&multibuffer_snapshot);
        let multibuffer_position = multibuffer_snapshot.anchor_before(&position);
        let Some((buffer_position, _)) =
            multibuffer_snapshot.anchor_to_buffer_anchor(multibuffer_position)
        else {
            return;
        };
        let Some(buffer) = self.buffer.read(cx).buffer(buffer_position.buffer_id) else {
            return;
        };

        let menu_is_open = self.context_menu.borrow().is_some();

        let query: Option<Arc<String>> =
            Self::completion_query(&multibuffer_snapshot, &multibuffer_position)
                .map(|query| query.into());

        drop(multibuffer_snapshot);

        // After a separator there's no word to filter by, so the old menu would list every
        // candidate until the new query finishes.
        if query.is_none() && menu_is_open {
            self.hide_context_menu(cx);
        }

        let Some(provider) = self.completion_provider.clone() else {
            return;
        };

        if let Some(menu) = self.context_menu.borrow_mut().as_mut() {
            menu.filter(
                query.clone().unwrap_or_default(),
                buffer_position,
                &buffer,
                cx,
            );
            let query_matches = match (&menu.initial_query, &query) {
                (Some(initial_query), Some(query)) => query.starts_with(initial_query.as_ref()),
                (None, _) => true,
                _ => false,
            };
            if query_matches {
                let position_matches = if menu.initial_position == position {
                    true
                } else {
                    let snapshot = self.buffer.read(cx).read(cx);
                    menu.initial_position.to_offset(&snapshot) == position.to_offset(&snapshot)
                };
                if position_matches {
                    return;
                }
            }
        }

        let load_provider_completions = trigger.is_none_or(|trigger| {
            provider.is_completion_trigger(&buffer, buffer_position, trigger, cx)
        });

        let provider_completions = if load_provider_completions {
            provider.completions(&buffer, buffer_position, window, cx)
        } else {
            Task::ready(Ok(Vec::new()))
        };

        let id = self.next_completion_id;
        self.next_completion_id += 1;
        let task = cx.spawn_in(window, async move |editor, cx| {
            let Ok(()) = editor.update(cx, |this, _| {
                this.completion_tasks.retain(|(task_id, _)| *task_id >= id);
            }) else {
                return;
            };

            let completions = provider_completions.await.log_err().unwrap_or_default();

            let menu = if completions.is_empty() {
                None
            } else {
                let Ok((mut menu, matches_task)) = editor.update(cx, |editor, cx| {
                    let menu = CompletionsMenu::new(
                        id,
                        position,
                        query.clone(),
                        buffer.clone(),
                        completions.into(),
                        editor
                            .context_menu
                            .borrow()
                            .as_ref()
                            .map(|menu| menu.scroll_handle.clone()),
                    );
                    let matches_task = menu.do_async_filtering(
                        query.unwrap_or_default(),
                        buffer_position,
                        &buffer,
                        cx,
                    );
                    (menu, matches_task)
                }) else {
                    return;
                };

                let matches = matches_task.await;

                let Ok(()) = editor.update(cx, |editor, cx| {
                    if let Some(prev_menu) = editor.context_menu.borrow().as_ref()
                        && prev_menu.id > id
                    {
                        return;
                    }

                    menu.set_filter_results(matches, cx);
                }) else {
                    return;
                };

                menu.visible().then_some(menu)
            };

            if let Err(error) = editor.update_in(cx, |editor, window, cx| {
                if editor.focus_handle.is_focused(window)
                    && let Some(menu) = menu
                {
                    *editor.context_menu.borrow_mut() = Some(menu);
                    hover_popover::hide_hover(editor, cx);
                    cx.notify();
                    return;
                }

                if editor.completion_tasks.len() <= 1 {
                    editor.hide_context_menu(cx);
                }
            }) {
                log::debug!("Failed to update completions menu: {error:?}");
            }
        });

        self.completion_tasks.push((id, task));
    }

    pub(crate) fn with_completions_menu_matching_id<R>(
        &self,
        id: CompletionId,
        update: impl FnOnce(Option<&mut CompletionsMenu>) -> R,
    ) -> R {
        let mut context_menu = self.context_menu.borrow_mut();
        let Some(completions_menu) = &mut *context_menu else {
            return update(None);
        };
        if completions_menu.id != id {
            return update(None);
        }
        update(Some(completions_menu))
    }

    fn completion_query(buffer: &MultiBufferSnapshot, position: &impl ToOffset) -> Option<String> {
        let offset = position.to_offset(buffer);
        let (word_range, kind) =
            buffer.surrounding_word(&offset, Some(CharScopeContext::Completion));
        if offset > word_range.start && kind == Some(CharKind::Word) {
            Some(
                buffer
                    .text_for_range(word_range.start..offset)
                    .collect::<String>(),
            )
        } else {
            None
        }
    }

    fn do_completion(
        &mut self,
        item_index: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<()> {
        let completions_menu = self.hide_context_menu(cx)?;

        let candidate_id = completions_menu
            .entries
            .borrow()
            .get(item_index.unwrap_or(completions_menu.selected_item))?
            .candidate_id;

        let completion = completions_menu
            .completions
            .borrow()
            .get(candidate_id)?
            .clone();
        cx.stop_propagation();

        let multibuffer_snapshot = self.buffer.read(cx).snapshot(cx);
        let (initial_position, _) =
            multibuffer_snapshot.anchor_to_buffer_anchor(completions_menu.initial_position)?;

        let buffer = completions_menu.buffer.read(cx).snapshot();
        let mut replace_range = completion.replace_range.clone();
        if replace_range.end.cmp(&initial_position, &buffer).is_lt() {
            replace_range.end = initial_position;
        }

        let replace_range =
            multibuffer_snapshot.buffer_anchor_range_to_anchor_range(replace_range)?;
        let range = replace_range.start.to_offset(&multibuffer_snapshot).0
            ..replace_range.end.to_offset(&multibuffer_snapshot).0;
        self.replace_range(
            range,
            &completion.new_text,
            SelectionEffects::no_scroll(),
            window,
            cx,
        );

        self.finalize_last_transaction(cx);

        Some(())
    }
}

pub trait CompletionProvider {
    fn completions(
        &self,
        buffer: &Entity<Buffer>,
        buffer_position: text::Anchor,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> Task<anyhow::Result<Vec<Completion>>>;

    fn is_completion_trigger(
        &self,
        buffer: &Entity<Buffer>,
        position: language::Anchor,
        text: &str,
        cx: &mut Context<Editor>,
    ) -> bool;
}

pub(crate) fn split_words(text: &str) -> impl Iterator<Item = &str> + '_ {
    let mut prev_index = 0;
    let mut prev_codepoint: Option<char> = None;
    text.char_indices()
        .chain([(text.len(), '\0')])
        .filter_map(move |(index, codepoint)| {
            let prev_codepoint = prev_codepoint.replace(codepoint)?;
            let is_boundary = index == text.len()
                || !prev_codepoint.is_uppercase() && codepoint.is_uppercase()
                || !prev_codepoint.is_alphanumeric() && codepoint.is_alphanumeric();
            if is_boundary {
                let chunk = text.get(prev_index..index);
                prev_index = index;
                chunk
            } else {
                None
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    use gpui::TestAppContext;
    use std::{
        ops::Range,
        sync::atomic::{AtomicUsize, Ordering},
    };
    use text::ToOffset as _;

    use actions::editor::{Backspace, ContextMenuFirst, ContextMenuLast, MoveLeft};
    use language::CodeLabel;
    use multi_buffer::CharClassifier;
    use settings::SettingsStore;
    use theme::LoadThemes;
    use util::test::{TextRangeMarker, marked_text_ranges_by};

    use crate::test::editor_test_context::EditorTestContext;

    fn init_test(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test_new(cx);
            cx.set_global(settings_store);
            theme::init(LoadThemes::JustBase, cx);
            crate::init(cx);
        });
    }

    struct FakeCompletionMenuProvider {
        complete_from_offset: usize,
        replace_range: Range<usize>,
        completions: Vec<&'static str>,
        counter: Arc<AtomicUsize>,
    }

    impl CompletionProvider for FakeCompletionMenuProvider {
        fn completions(
            &self,
            buffer: &Entity<Buffer>,
            buffer_position: text::Anchor,
            _: &mut Window,
            cx: &mut Context<Editor>,
        ) -> Task<anyhow::Result<Vec<Completion>>> {
            let buffer = buffer.read(cx);
            assert_eq!(buffer_position.to_offset(buffer), self.complete_from_offset);
            self.counter.fetch_add(1, Ordering::Release);

            let replace_range = buffer.anchor_before(self.replace_range.start)
                ..buffer.anchor_after(self.replace_range.end);
            Task::ready(Ok(self
                .completions
                .iter()
                .map(|completion_text| Completion {
                    replace_range: replace_range.clone(),
                    new_text: completion_text.to_string(),
                    label: CodeLabel::plain(completion_text.to_string(), None),
                    documentation: None,
                    match_start: None,
                })
                .collect()))
        }

        fn is_completion_trigger(
            &self,
            buffer: &Entity<Buffer>,
            position: language::Anchor,
            text: &str,
            cx: &mut Context<Editor>,
        ) -> bool {
            let mut chars = text.chars();
            let Some(character) = chars.next() else {
                return false;
            };
            if chars.next().is_some() {
                return false;
            }

            if CharClassifier::default().is_word(character) {
                return true;
            }

            character == '{'
                && buffer
                    .read(cx)
                    .reversed_chars_at(position)
                    .take(2)
                    .eq(['{', '{'])
        }
    }

    fn handle_completion_request(
        marked_string: &str,
        completions: Vec<&'static str>,
        counter: Arc<AtomicUsize>,
        cx: &mut EditorTestContext,
    ) {
        let complete_from_marker: TextRangeMarker = '|'.into();
        let replace_range_marker: TextRangeMarker = ('<', '>').into();
        let (_, mut marked_ranges) = marked_text_ranges_by(
            marked_string,
            vec![complete_from_marker.clone(), replace_range_marker.clone()],
        );

        let complete_from_offset = marked_ranges.remove(&complete_from_marker).unwrap()[0].start;
        let replace_range = marked_ranges.remove(&replace_range_marker).unwrap()[0].clone();

        cx.update_editor(|editor, _, _| {
            editor.set_completion_provider(Some(Rc::new(FakeCompletionMenuProvider {
                complete_from_offset,
                replace_range,
                completions,
                counter,
            })));
        });
    }

    fn check_displayed_completions(expected: &[&str], cx: &mut EditorTestContext) {
        cx.update_editor(|editor, _, _| {
            if let Some(menu) = editor.context_menu.borrow().as_ref() {
                let entries = menu.entries.borrow();
                let entries = entries
                    .iter()
                    .map(|string_match| string_match.string.as_str())
                    .collect::<Vec<_>>();
                assert_eq!(entries, expected);
            } else {
                panic!("expected completions menu");
            }
        });
    }

    fn completion_menu_entries(menu: &CompletionsMenu) -> Vec<String> {
        let entries = menu.entries.borrow();
        entries
            .iter()
            .map(|string_match| string_match.string.clone())
            .collect()
    }

    #[gpui::test]
    fn test_completion_first_last(cx: &mut TestAppContext) {
        init_test(cx);
        let mut cx = EditorTestContext::new(cx);

        cx.set_state("{ˇ");
        handle_completion_request(
            "{{|<>",
            vec!["base_url", "token"],
            Arc::new(AtomicUsize::new(0)),
            &mut cx,
        );
        cx.simulate_keystroke("{");
        cx.run_until_parked();

        cx.update_editor(|editor, _, _| {
            if let Some(menu) = editor.context_menu.borrow().as_ref() {
                assert_eq!(completion_menu_entries(menu), &["base_url", "token"]);
            } else {
                panic!("expected completion menu to be open");
            }
        });

        cx.update_editor(|editor, window, cx| {
            editor.context_menu_last(&ContextMenuLast, window, cx);
            if let Some(menu) = editor.context_menu.borrow().as_ref() {
                assert_eq!(menu.selected_item, 1);
            } else {
                panic!("expected completion menu to stay open after ContextMenuLast");
            }
        });

        cx.update_editor(|editor, window, cx| {
            editor.context_menu_first(&ContextMenuFirst, window, cx);
            if let Some(menu) = editor.context_menu.borrow().as_ref() {
                assert_eq!(menu.selected_item, 0);
            } else {
                panic!("expected completion menu to stay open after ContextMenuFirst");
            }
        });
    }

    #[gpui::test]
    fn test_completion_reuse(cx: &mut TestAppContext) {
        init_test(cx);
        let mut cx = EditorTestContext::new(cx);

        let counter = Arc::new(AtomicUsize::new(0));
        cx.set_state("{ˇ");
        handle_completion_request(
            "{{|<>",
            vec!["team_id", "timeout", "token"],
            counter.clone(),
            &mut cx,
        );
        cx.simulate_keystroke("{");
        cx.run_until_parked();
        assert_eq!(counter.load(Ordering::Acquire), 1);
        cx.assert_state("{{ˇ");
        check_displayed_completions(&["team_id", "timeout", "token"], &mut cx);

        cx.simulate_keystroke("t");
        cx.run_until_parked();
        assert_eq!(counter.load(Ordering::Acquire), 1);
        cx.assert_state("{{tˇ");
        check_displayed_completions(&["team_id", "timeout", "token"], &mut cx);

        cx.simulate_keystroke("o");
        cx.run_until_parked();
        assert_eq!(counter.load(Ordering::Acquire), 1);
        cx.assert_state("{{toˇ");
        check_displayed_completions(&["token", "timeout"], &mut cx);

        cx.simulate_keystroke("k");
        cx.run_until_parked();
        assert_eq!(counter.load(Ordering::Acquire), 1);
        cx.assert_state("{{tokˇ");
        check_displayed_completions(&["token"], &mut cx);

        cx.update_editor(|editor, window, cx| {
            editor.backspace(&Backspace, window, cx);
        });
        cx.run_until_parked();
        assert_eq!(counter.load(Ordering::Acquire), 1);
        cx.assert_state("{{toˇ");
        check_displayed_completions(&["token", "timeout"], &mut cx);

        cx.update_editor(|editor, window, cx| {
            editor.move_left(&MoveLeft, window, cx);
        });
        cx.run_until_parked();
        assert_eq!(counter.load(Ordering::Acquire), 1);
        cx.assert_state("{{tˇo");
        cx.update_editor(|editor, _, _| {
            assert!(!editor.context_menu_visible());
        });

        handle_completion_request(
            "{{<to|>o",
            vec!["timeout", "token"],
            counter.clone(),
            &mut cx,
        );
        cx.simulate_keystroke("o");
        cx.run_until_parked();
        assert_eq!(counter.load(Ordering::Acquire), 2);
        cx.assert_state("{{toˇo");
        check_displayed_completions(&["token", "timeout"], &mut cx);

        // The query shrinks from "to" to "t", so the menu queries again.
        handle_completion_request(
            "{{<t|>o",
            vec!["team_id", "timeout", "token"],
            counter.clone(),
            &mut cx,
        );
        cx.update_editor(|editor, window, cx| {
            editor.backspace(&Backspace, window, cx);
        });
        cx.run_until_parked();
        assert_eq!(counter.load(Ordering::Acquire), 3);
        cx.assert_state("{{tˇo");
        check_displayed_completions(&["team_id", "timeout", "token"], &mut cx);

        handle_completion_request(
            "{{|<>o",
            vec!["team_id", "timeout", "token"],
            counter.clone(),
            &mut cx,
        );
        cx.update_editor(|editor, window, cx| {
            editor.backspace(&Backspace, window, cx);
        });
        cx.run_until_parked();
        assert_eq!(counter.load(Ordering::Acquire), 4);
        cx.assert_state("{{ˇo");
        check_displayed_completions(&["team_id", "timeout", "token"], &mut cx);
    }
}
