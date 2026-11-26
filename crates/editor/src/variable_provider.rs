use gpui::{App, Context, Entity, SharedString, Task, Window};
use std::{collections::HashMap, ops::Range};
use text::{ToOffset as _, ToPoint as _};

use language::{Buffer, CodeLabel};
use path::PathStyle;
use project::{Completion, CompletionDocumentation, Variable, project_config_store::VariableScope};
use ui::IconAsset;

use crate::{CompletionProvider, Editor, SemanticsProvider};

pub trait VariableProviderDelegate: 'static {
    fn variables(&self, cx: &App) -> Option<Vec<(VariableScope, Variable)>>;
}

pub struct VariableProvider<T: VariableProviderDelegate> {
    source: T,
}

impl<T: VariableProviderDelegate> VariableProvider<T> {
    pub fn new(source: T) -> Self {
        Self { source }
    }

    fn resolved_variables(
        &self,
        cx: &App,
    ) -> Option<HashMap<String, (VariableScope, SharedString)>> {
        let scoped_variables = self.source.variables(cx)?;
        let values = scoped_variables
            .iter()
            .map(|(_, variable)| (variable.name.as_str(), variable.value.as_str()))
            .collect::<HashMap<_, _>>();
        let resolved_variables = scoped_variables
            .iter()
            .filter(|(_, variable)| !variable.name.is_empty())
            .map(|(scope, variable)| {
                let value = project::substitute_variables_in_str(&variable.value, &values);
                (
                    variable.name.clone(),
                    (scope.clone(), SharedString::from(value)),
                )
            })
            .collect();
        Some(resolved_variables)
    }
}

impl<T: VariableProviderDelegate> CompletionProvider for VariableProvider<T> {
    fn completions(
        &self,
        buffer: &Entity<Buffer>,
        buffer_position: text::Anchor,
        _: &mut Window,
        cx: &mut Context<Editor>,
    ) -> Task<anyhow::Result<Vec<Completion>>> {
        let buffer = buffer.read(cx);
        let Some(variable_completion) = VariableCompletion::for_position(buffer, buffer_position)
        else {
            return Task::ready(Ok(Vec::new()));
        };
        let Some(resolved_variables) = self.resolved_variables(cx) else {
            return Task::ready(Ok(Vec::new()));
        };

        let replace_range = buffer.anchor_before(variable_completion.source_range.start)
            ..buffer.anchor_after(variable_completion.source_range.end);
        let completions = resolved_variables
            .into_iter()
            .map(|(name, (scope, value))| {
                let (scope_kind, icon, icon_color) = match scope {
                    VariableScope::Project => ("Project", IconAsset::SquareInset, None),
                    VariableScope::Environment { color, .. } => (
                        "Environment",
                        IconAsset::SquareInset,
                        color.map(|color| crate::palette_color(color).color(cx)),
                    ),
                    VariableScope::Folder(_) => ("Folder", IconAsset::Folder, None),
                };
                Completion {
                    replace_range: replace_range.clone(),
                    new_text: format!("{name}{}", variable_completion.closing_braces),
                    label: CodeLabel::plain(name, None),
                    documentation: Some(CompletionDocumentation {
                        single_line: SharedString::from(scope_kind),
                        plain_text: value,
                    }),
                    icon_path: Some(icon.path().into()),
                    icon_color,
                    match_start: Some(replace_range.start),
                }
            })
            .collect();
        Task::ready(Ok(completions))
    }

    fn is_completion_trigger(
        &self,
        buffer: &Entity<Buffer>,
        position: language::Anchor,
        _: &str,
        cx: &mut Context<Editor>,
    ) -> bool {
        VariableCompletion::for_position(buffer.read(cx), position).is_some()
    }
}

impl<T: VariableProviderDelegate> SemanticsProvider for VariableProvider<T> {
    fn hover(
        &self,
        buffer: &Entity<Buffer>,
        position: text::Anchor,
        cx: &mut App,
    ) -> Option<Task<Option<Vec<project::Hover>>>> {
        let buffer = buffer.read(cx);
        let point = position.to_point(buffer);
        let line_start = buffer.point_to_offset(text::Point::new(point.row, 0));
        let line_end =
            buffer.point_to_offset(text::Point::new(point.row, buffer.line_len(point.row)));
        let line = buffer
            .text_for_range(line_start..line_end)
            .collect::<String>();
        let cursor_in_line = position.to_offset(buffer) - line_start;
        let (range, name) = project::variable_ranges_in_str(&line)
            .find(|(range, _)| range.contains(&cursor_in_line))?;

        let hover = project::Hover {
            range: buffer.anchor_after(line_start + range.start)
                ..buffer.anchor_before(line_start + range.end),
            variable: self.resolved_variables(cx)?.remove(name),
        };
        Some(Task::ready(Some(vec![hover])))
    }
}

struct VariableCompletion {
    source_range: Range<usize>,
    closing_braces: &'static str,
}

impl VariableCompletion {
    fn for_position(buffer: &Buffer, position: text::Anchor) -> Option<Self> {
        let cursor = position.to_offset(buffer);

        let mut characters_before = buffer.reversed_chars_at(cursor).peekable();
        let mut name_start = cursor;
        while let Some(character) =
            characters_before.next_if(|character| project::is_variable_name_char(*character))
        {
            name_start -= character.len_utf8();
        }
        if characters_before.next() != Some('{') || characters_before.next() != Some('{') {
            return None;
        }
        if name_start == cursor && characters_before.next() == Some('{') {
            return None;
        }

        let mut characters_after = buffer.chars_at(cursor).peekable();
        let mut name_tail_end = cursor;
        while let Some(character) =
            characters_after.next_if(|character| project::is_variable_name_char(*character))
        {
            name_tail_end += character.len_utf8();
        }
        let closing_braces_after_tail = characters_after
            .take(2)
            .take_while(|character| *character == '}')
            .count();
        let (name_end, existing_closing_braces) = if closing_braces_after_tail == 2 {
            (name_tail_end, 2)
        } else {
            let closing_braces_after_cursor = buffer
                .chars_at(cursor)
                .take(2)
                .take_while(|character| *character == '}')
                .count();
            (cursor, closing_braces_after_cursor)
        };

        Some(Self {
            source_range: name_start..name_end,
            closing_braces: "}}".get(existing_closing_braces..)?,
        })
    }
}

pub(crate) fn scope_label(scope: &VariableScope) -> SharedString {
    match scope {
        VariableScope::Project => SharedString::from("Project"),
        VariableScope::Environment { name, .. } => SharedString::from(name.clone()),
        VariableScope::Folder(folder) => {
            SharedString::from(folder.display(PathStyle::local()).into_owned())
        }
    }
}
