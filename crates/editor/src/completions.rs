use gpui::{Context, Entity, Task, Window};
use std::{rc::Rc, sync::Arc};

use actions::editor::{ConfirmCompletion, ShowCompletions};
use language::Buffer;
use multi_buffer::{CharKind, CharScopeContext, MultiBufferSnapshot, ToOffset};
use project::Completion;
use util::ResultExt;

use crate::{CompletionId, Editor, SelectionEffects, code_context_menus::CompletionsMenu};

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
