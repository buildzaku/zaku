use gpui::{App, Context, Entity, SharedString, Task, Window};
use std::{collections::HashMap, ops::Range};
use text::ToOffset as _;

use language::{Buffer, CodeLabel};
use path::PathStyle;
use project::{Completion, CompletionDocumentation, Variable, project_config_store::VariableScope};

use crate::{CompletionProvider, Editor};

pub trait VariableCompletionProviderDelegate: 'static {
    fn variables(&self, cx: &App) -> Option<Vec<(VariableScope, Variable)>>;
    fn path_style(&self, cx: &App) -> PathStyle;
}

pub struct VariableCompletionProvider<T: VariableCompletionProviderDelegate> {
    source: T,
}

impl<T: VariableCompletionProviderDelegate> VariableCompletionProvider<T> {
    pub fn new(source: T) -> Self {
        Self { source }
    }
}

impl<T: VariableCompletionProviderDelegate> CompletionProvider for VariableCompletionProvider<T> {
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
        let Some(scoped_variables) = self.source.variables(cx) else {
            return Task::ready(Ok(Vec::new()));
        };
        let path_style = self.source.path_style(cx);

        let mut variables: HashMap<_, _> = HashMap::default();
        for (scope, variable) in &scoped_variables {
            if !variable.name.is_empty() {
                variables.insert(variable.name.as_str(), (scope, variable.value.as_str()));
            }
        }
        let values = variables
            .iter()
            .map(|(name, (_, value))| (name.to_string(), value.to_string()))
            .collect::<HashMap<_, _>>();

        let replace_range = buffer.anchor_before(variable_completion.source_range.start)
            ..buffer.anchor_after(variable_completion.source_range.end);
        let completions = variables
            .into_iter()
            .map(|(name, (scope, value))| Completion {
                replace_range: replace_range.clone(),
                new_text: format!("{name}{}", variable_completion.closing_braces),
                label: CodeLabel::plain(name.to_string(), None),
                documentation: Some(CompletionDocumentation {
                    single_line: self::scope_label(scope, path_style),
                    plain_text: project::substitute_variables_in_str(value, &values).into(),
                }),
                match_start: Some(replace_range.start),
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

fn scope_label(scope: &VariableScope, path_style: PathStyle) -> SharedString {
    match scope {
        VariableScope::Project => SharedString::from("Project"),
        VariableScope::Environment(name) => SharedString::from(name.clone()),
        VariableScope::Folder(folder) => {
            SharedString::from(folder.display(path_style).into_owned())
        }
    }
}
