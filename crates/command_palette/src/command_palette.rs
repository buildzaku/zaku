use fuzzy_nucleo::{StringMatch, StringMatchCandidate};
use gpui::{
    Action, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
    ParentElement, Render, Styled, Task, WeakEntity, Window, prelude::*,
};
use smol::channel::Receiver;
use std::{
    cmp,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use picker::{Picker, PickerDelegate};
use ui::{HighlightedLabel, KeyBinding, ListItem, ListItemSpacing, Toggleable};
use workspace::{ModalView, Workspace};

pub fn init(cx: &mut App) {
    cx.observe_new(CommandPalette::register).detach();
}

pub struct CommandPalette {
    picker: Entity<Picker<CommandPaletteDelegate>>,
}

impl ModalView for CommandPalette {}

impl CommandPalette {
    fn register(workspace: &mut Workspace, _: Option<&mut Window>, _: &mut Context<Workspace>) {
        workspace.register_action(
            |workspace, _: &actions::command_palette::Toggle, window, cx| {
                Self::toggle(workspace, "", window, cx);
            },
        );
    }

    pub fn toggle(
        workspace: &mut Workspace,
        query: &str,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        let Some(previous_focus_handle) = window.focused(cx) else {
            return;
        };

        workspace.toggle_modal(window, cx, move |window, cx| {
            CommandPalette::new(previous_focus_handle, query, window, cx)
        });
    }

    fn new(
        previous_focus_handle: FocusHandle,
        query: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let commands = window
            .available_actions(cx)
            .into_iter()
            .map(|action| Command {
                name: humanize_action_name(action.name()),
                action,
            })
            .collect();

        let delegate =
            CommandPaletteDelegate::new(cx.entity().downgrade(), commands, previous_focus_handle);

        let picker = cx.new(|cx| {
            let picker = Picker::uniform_list(delegate, window, cx)
                .initial_width(gpui::rems(34.0))
                .minimum_results_width(gpui::rems(30.0))
                .height(gpui::rems(24.0))
                .no_vertical_padding();
            picker.set_query(query, window, cx);
            picker
        });

        Self { picker }
    }

    pub fn set_query(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.picker.update(cx, |picker, cx| {
            picker.set_query(query, window, cx);
        });
    }
}

impl EventEmitter<DismissEvent> for CommandPalette {}

impl Focusable for CommandPalette {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl Render for CommandPalette {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui::div()
            .flex()
            .flex_col()
            .key_context("CommandPalette")
            .child(self.picker.clone())
    }
}

pub fn normalize_action_query(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut last_char = None;

    for character in input.trim().chars() {
        let normalized_character = if character == '_' { ' ' } else { character };
        match (last_char, normalized_character) {
            (Some(':'), ':') => continue,
            (Some(last_character), character)
                if last_character.is_whitespace() && character.is_whitespace() =>
            {
                continue;
            }
            _ => {
                last_char = Some(normalized_character);
            }
        }
        result.push(normalized_character);
    }

    result
}

pub struct CommandPaletteDelegate {
    command_palette: WeakEntity<CommandPalette>,
    all_commands: Vec<Command>,
    commands: Vec<Command>,
    matches: Vec<StringMatch>,
    selected_index: usize,
    previous_focus_handle: FocusHandle,
    updating_matches: Option<(Task<()>, Receiver<(Vec<Command>, Vec<StringMatch>)>)>,
}

impl CommandPaletteDelegate {
    fn new(
        command_palette: WeakEntity<CommandPalette>,
        commands: Vec<Command>,
        previous_focus_handle: FocusHandle,
    ) -> Self {
        Self {
            command_palette,
            all_commands: commands.clone(),
            commands,
            matches: Vec::new(),
            selected_index: 0,
            previous_focus_handle,
            updating_matches: None,
        }
    }

    fn matches_updated(
        &mut self,
        commands: Vec<Command>,
        matches: Vec<StringMatch>,
        _: &mut Context<Picker<Self>>,
    ) {
        drop(self.updating_matches.take());
        self.commands = commands;
        self.matches = matches;
        if self.matches.is_empty() {
            self.selected_index = 0;
        } else {
            self.selected_index = cmp::min(self.selected_index, self.matches.len() - 1);
        }
    }
}

impl PickerDelegate for CommandPaletteDelegate {
    type ListItem = ListItem;

    fn name() -> &'static str {
        "command palette"
    }

    fn placeholder_text(&self, _: &mut Window, _: &mut App) -> Arc<str> {
        "Execute a command...".into()
    }

    fn match_count(&self) -> usize {
        self.matches.len()
    }

    fn selected_index(&self) -> usize {
        self.selected_index
    }

    fn set_selected_index(&mut self, index: usize, _: &mut Window, _: &mut Context<Picker<Self>>) {
        self.selected_index = index;
    }

    fn update_matches(
        &mut self,
        query: String,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        let (tx, rx) = smol::channel::bounded(1);
        let normalized_query = normalize_action_query(&query);

        let task = cx.background_spawn({
            let mut commands = self.all_commands.clone();
            let executor = cx.background_executor().clone();
            async move {
                commands.sort_by_key(|command| command.name.clone());

                let candidates = commands
                    .iter()
                    .enumerate()
                    .map(|(index, command)| StringMatchCandidate::new(index, &command.name))
                    .collect::<Vec<_>>();

                let matches = fuzzy_nucleo::match_strings_async(
                    &candidates,
                    &normalized_query,
                    fuzzy_nucleo::Case::Smart,
                    fuzzy_nucleo::LengthPenalty::On,
                    10000,
                    &AtomicBool::default(),
                    executor,
                )
                .await;

                if tx.send((commands, matches)).await.is_err() {
                    log::debug!("Failed to send command palette matches");
                }
            }
        });

        self.updating_matches = Some((task, rx.clone()));

        cx.spawn_in(window, async move |picker, cx| {
            let Ok((commands, matches)) = rx.recv().await else {
                return;
            };

            if let Err(error) = picker.update(cx, |picker, cx| {
                picker.delegate.matches_updated(commands, matches, cx);
            }) {
                log::debug!("Failed to update command palette matches: {error:?}");
            }
        })
    }

    fn finalize_update_matches(
        &mut self,
        _: String,
        duration: Duration,
        _: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> bool {
        let Some((task, rx)) = self.updating_matches.take() else {
            return true;
        };

        match cx
            .foreground_executor()
            .block_with_timeout(duration, rx.clone().recv())
        {
            Ok(Ok((commands, matches))) => {
                self.matches_updated(commands, matches, cx);
                true
            }
            Ok(Err(_)) => true,
            Err(_) => {
                self.updating_matches = Some((task, rx));
                false
            }
        }
    }

    fn dismissed(&mut self, _: &mut Window, cx: &mut Context<Picker<Self>>) {
        if let Err(error) = self.command_palette.update(cx, |_, cx| {
            cx.emit(DismissEvent);
        }) {
            log::debug!("Failed to dismiss command palette: {error:?}");
        }
    }

    fn confirm(&mut self, secondary: bool, window: &mut Window, cx: &mut Context<Picker<Self>>) {
        if secondary {
            return;
        }

        if self.matches.is_empty() {
            self.dismissed(window, cx);
            return;
        }

        let Some(action_index) = self
            .matches
            .get(self.selected_index)
            .map(|match_| match_.candidate_id)
        else {
            self.dismissed(window, cx);
            return;
        };

        if action_index >= self.commands.len() {
            self.dismissed(window, cx);
            return;
        }

        let command = self.commands.swap_remove(action_index);
        self.matches.clear();
        self.commands.clear();
        let action = command.action;
        self.previous_focus_handle.focus(window, cx);
        self.dismissed(window, cx);
        window.dispatch_action(action, cx);
    }

    fn render_match(
        &self,
        index: usize,
        selected: bool,
        _: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        let matching_command = self.matches.get(index)?;
        let command = self.commands.get(matching_command.candidate_id)?;

        Some(
            ListItem::new(index)
                .inset(true)
                .spacing(ListItemSpacing::Sparse)
                .toggle_state(selected)
                .child(
                    gpui::div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .w_full()
                        .py_px()
                        .justify_between()
                        .child(HighlightedLabel::new(
                            command.name.clone(),
                            matching_command.positions.clone(),
                        ))
                        .child(KeyBinding::for_action_in(
                            command.action.as_ref(),
                            &self.previous_focus_handle,
                            cx,
                        )),
                ),
        )
    }
}

struct Command {
    name: String,
    action: Box<dyn Action>,
}

impl Clone for Command {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            action: self.action.boxed_clone(),
        }
    }
}

pub fn humanize_action_name(name: &str) -> String {
    let characters = name.chars().collect::<Vec<_>>();
    let capacity = name.len()
        + characters
            .iter()
            .filter(|character| character.is_uppercase())
            .count();
    let mut result = String::with_capacity(capacity);
    let mut index = 0;

    while index < characters.len() {
        let character = characters[index];
        if character == ':' {
            if result.ends_with(':') {
                result.push(' ');
            } else {
                result.push(':');
            }
            index += 1;
        } else if character == '_' {
            result.push(' ');
            index += 1;
        } else if character.is_uppercase() {
            let start = index;
            index += 1;
            while characters
                .get(index)
                .is_some_and(|next_character| next_character.is_uppercase())
            {
                index += 1;
            }

            let uppercase_run = &characters[start..index];
            if uppercase_run.len() > 1 {
                let split_before_last = characters
                    .get(index)
                    .is_some_and(|next_character| next_character.is_lowercase());
                let acronym_end = if split_before_last {
                    uppercase_run.len() - 1
                } else {
                    uppercase_run.len()
                };

                if acronym_end > 0 {
                    if !result.ends_with(' ') {
                        result.push(' ');
                    }
                    result.extend(&uppercase_run[..acronym_end]);
                }

                if split_before_last {
                    if !result.ends_with(' ') {
                        result.push(' ');
                    }
                    result.extend(uppercase_run[acronym_end].to_lowercase());
                }
            } else {
                if !result.ends_with(' ') {
                    result.push(' ');
                }
                result.extend(character.to_lowercase());
            }
        } else {
            result.push(character);
            index += 1;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_humanize_action_name() {
        assert_eq!(
            humanize_action_name("editor::ToggleLineNumbers"),
            "editor: toggle line numbers"
        );
        assert_eq!(
            humanize_action_name("editor::Backspace"),
            "editor: backspace"
        );
        assert_eq!(
            humanize_action_name("zaku::OpenSettingsFile"),
            "zaku: open settings file"
        );
        assert_eq!(
            humanize_action_name("project_panel::ToggleFocus"),
            "project panel: toggle focus"
        );
    }

    #[test]
    fn test_normalize_action_query() {
        assert_eq!(
            normalize_action_query("editor: backspace"),
            "editor: backspace"
        );
        assert_eq!(
            normalize_action_query("editor:  backspace"),
            "editor: backspace"
        );
        assert_eq!(
            normalize_action_query("editor::::ToggleLineNumbers"),
            "editor:ToggleLineNumbers"
        );
        assert_eq!(
            normalize_action_query("zaku::OpenSettingsFile"),
            "zaku:OpenSettingsFile"
        );
        assert_eq!(
            normalize_action_query("project_panel::ToggleFocus"),
            "project panel:ToggleFocus"
        );
    }
}
