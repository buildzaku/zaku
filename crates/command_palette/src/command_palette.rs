mod persistence;

use fuzzy_nucleo::{StringMatch, StringMatchCandidate};
use gpui::{
    Action, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
    ParentElement, Render, Styled, Task, TaskExt, WeakEntity, Window, prelude::*,
};
use smol::channel::Receiver;
use std::{
    cmp::{self, Reverse},
    collections::{HashMap, VecDeque},
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use command_palette_hooks::CommandPaletteFilter;
use picker::{Direction, Picker, PickerDelegate};
use ui::{
    HighlightedLabel, KeyBinding, ListItem, ListItemSpacing, Toggleable, prelude::ActiveTheme,
};
use workspace::{ModalView, Workspace};

use crate::persistence::CommandPaletteDB;

pub fn init(cx: &mut App) {
    command_palette_hooks::init(cx);
    smol::block_on(CommandPaletteDB::global(cx).initialize_schema())
        .expect("command palette persistence schema should initialize");

    cx.observe_new(CommandPalette::register).detach();
}

pub struct CommandPalette {
    picker: Entity<Picker<CommandPaletteDelegate>>,
}

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
        let filter = CommandPaletteFilter::try_global(cx);

        let commands = window
            .available_actions(cx)
            .into_iter()
            .filter_map(|action| {
                if filter.is_some_and(|filter| filter.is_hidden(action.as_ref())) {
                    return None;
                }

                Some(Command {
                    name: humanize_action_name(action.name()),
                    action,
                })
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

impl ModalView for CommandPalette {}

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
    latest_query: String,
    command_palette: WeakEntity<CommandPalette>,
    all_commands: Vec<Command>,
    commands: Vec<Command>,
    matches: Vec<StringMatch>,
    selected_index: usize,
    previous_focus_handle: FocusHandle,
    updating_matches: Option<(Task<()>, Receiver<(Vec<Command>, Vec<StringMatch>)>)>,
    query_history: QueryHistory,
}

impl CommandPaletteDelegate {
    fn new(
        command_palette: WeakEntity<CommandPalette>,
        commands: Vec<Command>,
        previous_focus_handle: FocusHandle,
    ) -> Self {
        Self {
            latest_query: String::new(),
            command_palette,
            all_commands: commands.clone(),
            commands,
            matches: Vec::new(),
            selected_index: 0,
            previous_focus_handle,
            updating_matches: None,
            query_history: QueryHistory::default(),
        }
    }

    fn matches_updated(
        &mut self,
        query: String,
        commands: Vec<Command>,
        matches: Vec<StringMatch>,
        _: &mut Context<Picker<Self>>,
    ) {
        drop(self.updating_matches.take());
        self.latest_query = query;
        self.commands = commands;
        self.matches = matches;
        if self.matches.is_empty() {
            self.selected_index = 0;
        } else {
            self.selected_index = cmp::min(self.selected_index, self.matches.len() - 1);
        }
    }

    fn hit_counts(cx: &App) -> HashMap<String, u16> {
        match CommandPaletteDB::global(cx).list_commands_used() {
            Ok(commands) => commands
                .into_iter()
                .map(|command| (command.command_name, command.invocations))
                .collect(),
            Err(error) => {
                log::debug!("Failed to load command palette usage history: {error:?}");
                HashMap::new()
            }
        }
    }
}

#[derive(Default)]
struct QueryHistory {
    history: Option<VecDeque<String>>,
    cursor: Option<usize>,
    prefix: Option<String>,
}

impl QueryHistory {
    fn history(&mut self, cx: &App) -> &mut VecDeque<String> {
        self.history.get_or_insert_with(|| {
            match CommandPaletteDB::global(cx).list_recent_queries() {
                Ok(queries) => queries.into_iter().collect(),
                Err(error) => {
                    log::debug!("Failed to load command palette query history: {error:?}");
                    VecDeque::new()
                }
            }
        })
    }

    fn add(&mut self, query: String, cx: &App) {
        if let Some(position) = self
            .history(cx)
            .iter()
            .position(|history| history == &query)
        {
            self.history(cx).remove(position);
        }
        self.history(cx).push_back(query);
        self.cursor = None;
        self.prefix = None;
    }

    fn validate_cursor(&mut self, current_query: &str, cx: &App) -> Option<usize> {
        if let Some(position) = self.cursor
            && self.history(cx).get(position).map(String::as_str) != Some(current_query)
        {
            self.cursor = None;
            self.prefix = None;
        }
        self.cursor
    }

    fn previous(&mut self, current_query: &str, cx: &App) -> Option<&str> {
        if self.validate_cursor(current_query, cx).is_none() {
            self.prefix = Some(current_query.to_string());
        }

        let prefix = self.prefix.clone().unwrap_or_default();
        let start_index = self.cursor.unwrap_or(self.history(cx).len());

        for index in (0..start_index).rev() {
            if self
                .history(cx)
                .get(index)
                .is_some_and(|history| history.starts_with(&prefix))
            {
                self.cursor = Some(index);
                return self.history(cx).get(index).map(String::as_str);
            }
        }
        None
    }

    fn next(&mut self, current_query: &str, cx: &App) -> Option<&str> {
        let selected = self.validate_cursor(current_query, cx)?;
        let prefix = self.prefix.clone().unwrap_or_default();

        for index in (selected + 1)..self.history(cx).len() {
            if self
                .history(cx)
                .get(index)
                .is_some_and(|history| history.starts_with(&prefix))
            {
                self.cursor = Some(index);
                return self.history(cx).get(index).map(String::as_str);
            }
        }
        None
    }

    fn reset_cursor(&mut self) {
        self.cursor = None;
        self.prefix = None;
    }

    fn is_navigating(&self) -> bool {
        self.cursor.is_some()
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

    fn select_history(
        &mut self,
        direction: Direction,
        query: &str,
        _: &mut Window,
        cx: &mut App,
    ) -> Option<String> {
        match direction {
            Direction::Up => {
                let should_use_history =
                    self.selected_index == 0 || self.query_history.is_navigating();
                if should_use_history
                    && let Some(query) = self
                        .query_history
                        .previous(query, cx)
                        .map(ToString::to_string)
                {
                    return Some(query);
                }
            }
            Direction::Down => {
                if self.query_history.is_navigating() {
                    if let Some(query) = self.query_history.next(query, cx).map(ToString::to_string)
                    {
                        return Some(query);
                    }
                    let prefix = self.query_history.prefix.take().unwrap_or_default();
                    self.query_history.reset_cursor();
                    return Some(prefix);
                }
            }
        }
        None
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
            let hit_counts = Self::hit_counts(cx);
            let executor = cx.background_executor().clone();
            async move {
                commands.sort_by_key(|command| {
                    (
                        Reverse(hit_counts.get(&command.name).copied()),
                        command.name.clone(),
                    )
                });

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
                picker
                    .delegate
                    .matches_updated(query, commands, matches, cx);
            }) {
                log::debug!("Failed to update command palette matches: {error:?}");
            }
        })
    }

    fn finalize_update_matches(
        &mut self,
        query: String,
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
                self.matches_updated(query, commands, matches, cx);
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

        if !self.latest_query.is_empty() {
            self.query_history.add(self.latest_query.clone(), cx);
            self.query_history.reset_cursor();
        }

        let command = self.commands.swap_remove(action_index);
        self.matches.clear();
        self.commands.clear();
        let command_name = command.name.clone();
        let latest_query = self.latest_query.clone();
        let db = CommandPaletteDB::global(cx);
        cx.background_spawn(async move {
            db.write_command_invocation(command_name, latest_query)
                .await
        })
        .detach_and_log_err(cx);
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
        let colors = cx.theme().colors();
        let hover_background = if selected {
            colors.element_selection_background
        } else {
            colors.element_hover
        };
        let active_background = if selected {
            colors.element_selection_background
        } else {
            colors.element_active
        };

        Some(
            ListItem::new(index)
                .inset(true)
                .spacing(ListItemSpacing::Sparse)
                .hover_background(hover_background)
                .active_background(active_background)
                .selected_background(colors.element_selection_background)
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
        assert_eq!(humanize_action_name("zaku::OpenLogs"), "zaku: open logs");
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
