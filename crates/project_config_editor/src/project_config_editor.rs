mod items;

use gpui::{
    App, Context, Div, Entity, EventEmitter, FocusHandle, Focusable, Hsla, ScrollHandle,
    SharedString, Subscription, TextStyleRefinement, Window, prelude::*,
};
use std::collections::HashMap;

use editor::{Editor, EditorEvent};
use path::PathStyle;
use project::{
    ConfigFile, ConfigFileState, ConfigFileType, EnvironmentColor, Project, ProjectConfigBuffer,
    ProjectConfigBufferEvent, ProjectPath, Variable,
};
use ui::{
    ActiveTheme, Button, ButtonCommon, ButtonSize, ButtonVariant, Checkbox, Clickable, Color,
    FixedWidth, IconAsset, IconButton, IconSize, ScrollAxes, Scrollbars, Text, TextCommon,
    TextSize, ToggleState, Tooltip, TrackLayout, WithScrollbar,
};

pub fn init(cx: &mut App) {
    workspace::register_project_item::<ProjectConfigEditor>(cx);
}

struct VariableRow {
    name: Entity<Editor>,
    value: Entity<Editor>,
    disabled: bool,
    _input_subscriptions: Vec<Subscription>,
}

impl VariableRow {
    fn variable(&self, cx: &App) -> Variable {
        Variable {
            name: self.name.read(cx).text(cx),
            value: self.value.read(cx).text(cx),
            disabled: self.disabled,
        }
    }

    fn set_disabled(&mut self, disabled: bool, cx: &mut App) {
        self.disabled = disabled;
        for editor in [&self.name, &self.value] {
            editor.update(cx, |editor, cx| {
                editor.set_muted(disabled);
                cx.notify();
            });
        }
    }
}

enum ValidationState {
    None,
    Warning(String),
    Error(String),
}

fn is_empty_variable(variable: &Variable) -> bool {
    variable.name.trim().is_empty() && variable.value.trim().is_empty()
}

fn is_valid_variable_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic())
        && characters.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
        })
}

fn validation_states(variables: &[Variable]) -> Vec<ValidationState> {
    let mut enabled_name_counts = HashMap::<&str, usize>::new();
    for variable in variables.iter().filter(|variable| !variable.disabled) {
        *enabled_name_counts
            .entry(variable.name.as_str())
            .or_default() += 1;
    }

    variables
        .iter()
        .map(|variable| {
            if is_empty_variable(variable) {
                ValidationState::None
            } else if variable.name.trim().is_empty() {
                ValidationState::Error("Variable name must be provided.".to_string())
            } else if !is_valid_variable_name(&variable.name) {
                ValidationState::Error(
                    "Variable name must start with a letter and contain only letters, digits, '_', '-' and '.'."
                        .to_string(),
                )
            } else if !variable.disabled
                && enabled_name_counts
                    .get(variable.name.as_str())
                    .is_some_and(|count| *count > 1)
            {
                ValidationState::Warning(format!(
                    "Variable '{}' is defined more than once; the last one is used.",
                    variable.name
                ))
            } else {
                ValidationState::None
            }
        })
        .collect()
}

fn variables_from_rows(rows: &[VariableRow], cx: &App) -> Vec<Variable> {
    rows.iter()
        .map(|row| row.variable(cx))
        .filter(|variable| !is_empty_variable(variable))
        .collect()
}

fn input_box(
    editor: &Entity<Editor>,
    disabled: bool,
    validation_color: Option<Hsla>,
    cx: &mut App,
) -> Div {
    let text_style = TextStyleRefinement {
        color: Some(cx.theme().colors().text),
        line_height: Some(gpui::relative(1.2)),
        ..Default::default()
    };
    editor.update(cx, |editor, _| {
        editor.set_text_style_refinement(text_style);
    });

    let theme_colors = cx.theme().colors();
    let focus_handle = editor.focus_handle(cx).tab_index(0).tab_stop(true);
    gpui::div()
        .flex()
        .flex_1()
        .min_w_0()
        .min_h_8()
        .px_2()
        .py_1p5()
        .rounded_md()
        .border_1()
        .border_color(validation_color.unwrap_or(if disabled {
            theme_colors.border_disabled
        } else {
            theme_colors.border_variant
        }))
        .bg(theme_colors.editor_background)
        .track_focus(&focus_handle)
        .focus(|this| {
            this.border_color(validation_color.unwrap_or(if disabled {
                theme_colors.border_disabled
            } else {
                theme_colors.border_focused
            }))
        })
        .child(editor.clone())
}

pub enum ProjectConfigEditorEvent {
    ProjectConfigBufferEdited,
    DirtyChanged,
    Saved,
    TitleChanged,
    FileHandleChanged,
}

enum ProjectConfigEditorState {
    Ready(Vec<VariableRow>),
    Invalid { error: String },
}

pub struct ProjectConfigEditor {
    focus_handle: FocusHandle,
    project: Entity<Project>,
    buffer: Entity<ProjectConfigBuffer>,
    state: ProjectConfigEditorState,
    saved_variables: Option<Vec<Variable>>,
    variables_scroll_handle: ScrollHandle,
    _buffer_subscription: Subscription,
}

impl ProjectConfigEditor {
    fn for_buffer(
        project: Entity<Project>,
        buffer: Entity<ProjectConfigBuffer>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let config_file = buffer.read(cx).config_file().clone();
        let (state, saved_variables) = Self::state_from_config_file(config_file, window, cx);
        let buffer_subscription = cx.subscribe_in(
            &buffer,
            window,
            |project_config_editor, buffer, event: &ProjectConfigBufferEvent, window, cx| {
                match event {
                    ProjectConfigBufferEvent::DirtyChanged => {
                        cx.emit(ProjectConfigEditorEvent::DirtyChanged);
                        cx.notify();
                    }
                    ProjectConfigBufferEvent::FileHandleChanged => {
                        cx.emit(ProjectConfigEditorEvent::TitleChanged);
                        cx.emit(ProjectConfigEditorEvent::FileHandleChanged);
                        cx.notify();
                    }
                    ProjectConfigBufferEvent::Saved => {
                        cx.emit(ProjectConfigEditorEvent::Saved);
                        cx.notify();
                    }
                    ProjectConfigBufferEvent::Reloaded => {
                        let config_file = buffer.read(cx).config_file().clone();
                        let (state, saved_variables) =
                            Self::state_from_config_file(config_file, window, cx);
                        project_config_editor.state = state;
                        project_config_editor.saved_variables = saved_variables;
                        cx.emit(ProjectConfigEditorEvent::TitleChanged);
                        cx.notify();
                    }
                    ProjectConfigBufferEvent::ReloadNeeded => {}
                }
            },
        );

        Self {
            focus_handle: cx.focus_handle(),
            project,
            buffer,
            state,
            saved_variables,
            variables_scroll_handle: ScrollHandle::new(),
            _buffer_subscription: buffer_subscription,
        }
    }

    fn state_from_config_file(
        config_file: ConfigFileState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (ProjectConfigEditorState, Option<Vec<Variable>>) {
        match config_file {
            ConfigFileState::Parsed(config_file) => {
                let variables = config_file.variables();
                let rows = variables
                    .iter()
                    .map(|variable| Self::new_variable_row(Some(variable), window, cx))
                    .collect();
                let saved_variables = variables
                    .iter()
                    .filter(|variable| !is_empty_variable(variable))
                    .cloned()
                    .collect();
                (ProjectConfigEditorState::Ready(rows), Some(saved_variables))
            }
            ConfigFileState::Invalid(error) => (ProjectConfigEditorState::Invalid { error }, None),
        }
    }

    fn new_variable_row(
        variable: Option<&Variable>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> VariableRow {
        let name = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("Name", cx);
            if let Some(variable) = variable {
                editor.set_text(&variable.name, cx);
            }
            editor
        });
        let value = cx.new(|cx| {
            let mut editor = Editor::auto_height(1, Some(4), window, cx);
            editor.set_placeholder_text("Value", cx);
            if let Some(variable) = variable {
                editor.set_text(&variable.value, cx);
            }
            editor
        });
        let input_subscriptions = [&name, &value]
            .into_iter()
            .map(|editor| {
                cx.subscribe_in(
                    editor,
                    window,
                    |project_config_editor, _, event: &EditorEvent, _, cx| {
                        if *event == EditorEvent::BufferEdited {
                            project_config_editor.mark_edited(cx);
                        }
                    },
                )
            })
            .collect();

        let mut row = VariableRow {
            name,
            value,
            disabled: false,
            _input_subscriptions: input_subscriptions,
        };
        if let Some(variable) = variable {
            row.set_disabled(variable.disabled, cx);
        }
        row
    }

    fn mark_edited(&mut self, cx: &mut Context<Self>) {
        let ProjectConfigEditorState::Ready(rows) = &self.state else {
            return;
        };

        let variables = self::variables_from_rows(rows, cx);
        let is_dirty = self
            .saved_variables
            .as_ref()
            .is_some_and(|saved_variables| *saved_variables != variables);
        let dirty_changed = self.buffer.update(cx, |buffer, cx| {
            if let ConfigFileState::Parsed(config_file) = buffer.config_file() {
                let mut config_file = config_file.clone();
                config_file.set_variables(variables);
                buffer.set_config_file(ConfigFileState::Parsed(config_file), cx);
            }
            buffer.set_dirty(is_dirty, cx)
        });

        cx.emit(ProjectConfigEditorEvent::ProjectConfigBufferEdited);
        if dirty_changed {
            cx.emit(ProjectConfigEditorEvent::DirtyChanged);
        }
        cx.notify();
    }

    fn project_path(&self, cx: &App) -> Option<ProjectPath> {
        project::ProjectItem::project_path(self.buffer.read(cx), cx)
    }

    fn path_style(&self, cx: &App) -> PathStyle {
        self.project.read(cx).path_style(cx)
    }

    fn title(&self, cx: &App) -> SharedString {
        let buffer = self.buffer.read(cx);
        match buffer.file_type() {
            ConfigFileType::Project => SharedString::from("Project Variables"),
            ConfigFileType::Environment => buffer
                .file()
                .path
                .file_stem()
                .map(|name| SharedString::from(name.to_owned()))
                .unwrap_or_default(),
        }
    }

    fn environment_color(&self, cx: &App) -> Option<EnvironmentColor> {
        match self.buffer.read(cx).config_file() {
            ConfigFileState::Parsed(ConfigFile::Environment(environment_file)) => {
                environment_file.environment.color
            }
            ConfigFileState::Parsed(ConfigFile::Project(_)) | ConfigFileState::Invalid(_) => None,
        }
    }

    fn render_invalid(&self, error: &str, cx: &mut Context<Self>) -> Div {
        gpui::div()
            .flex()
            .flex_col()
            .track_focus(&self.focus_handle)
            .w_full()
            .flex_1()
            .min_h_0()
            .gap_2()
            .p_3()
            .bg(cx.theme().colors().panel_background)
            .child(
                Text::new(match self.buffer.read(cx).file_type() {
                    ConfigFileType::Project => "Invalid Project Variables",
                    ConfigFileType::Environment => "Invalid Environment",
                })
                .size(TextSize::Large)
                .color(Color::Error),
            )
            .child(Text::new(error.to_string()).color(Color::Muted))
    }

    fn render_variables(
        &self,
        rows: &[VariableRow],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let variables = rows.iter().map(|row| row.variable(cx)).collect::<Vec<_>>();
        let rows = rows
            .iter()
            .zip(self::validation_states(&variables))
            .enumerate()
            .map(|(index, (row, validation_state))| {
                let validation_color_and_message = match validation_state {
                    ValidationState::None => None,
                    ValidationState::Error(message) => Some((Color::Error.color(cx), message)),
                    ValidationState::Warning(message) => Some((Color::Warning.color(cx), message)),
                };
                let checkbox = ui::checkbox(
                    ("variable-checkbox", index),
                    ToggleState::from(!row.disabled),
                )
                .on_click(cx.listener(
                    move |project_config_editor, new_state: &ToggleState, _, cx| {
                        let disabled = !new_state.selected();
                        if let ProjectConfigEditorState::Ready(rows) =
                            &mut project_config_editor.state
                            && let Some(row) = rows.get_mut(index)
                            && row.disabled != disabled
                        {
                            row.set_disabled(disabled, cx);
                            project_config_editor.mark_edited(cx);
                        }
                    },
                ));
                let theme_colors = cx.theme().colors();
                let remove_button = IconButton::new(("variable-remove", index), IconAsset::Close)
                    .variant(ButtonVariant::Custom {
                        background: theme_colors.element_background.opacity(0.5),
                        foreground: theme_colors.button_secondary_foreground,
                        hover_background: theme_colors.button_secondary_hover_background,
                        border: gpui::transparent_black(),
                    })
                    .size(ButtonSize::None)
                    .width(gpui::rems(1.0))
                    .height(gpui::rems(1.0))
                    .icon_size(IconSize::Small)
                    .icon_color(Color::Muted)
                    .tooltip(Tooltip::text("Remove"))
                    .on_click(cx.listener(move |project_config_editor, _, _, cx| {
                        let ProjectConfigEditorState::Ready(rows) =
                            &mut project_config_editor.state
                        else {
                            return;
                        };
                        if index >= rows.len() {
                            return;
                        }
                        rows.remove(index);
                        project_config_editor.mark_edited(cx);
                    }));
                let name_input = self::input_box(
                    &row.name,
                    row.disabled,
                    validation_color_and_message
                        .as_ref()
                        .map(|(color, _)| *color),
                    cx,
                )
                .items_center();

                gpui::div()
                    .id(("variable-row", index))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .w_full()
                    .child(
                        gpui::div()
                            .flex()
                            .items_center()
                            .w_full()
                            .child(gpui::div().pr_1p5().child(checkbox))
                            .child(
                                gpui::div()
                                    .flex()
                                    .items_stretch()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_2p5()
                                    .child(name_input)
                                    .child(self::input_box(&row.value, row.disabled, None, cx))
                                    .child(gpui::div().flex().items_center().child(remove_button)),
                            ),
                    )
                    .when_some(validation_color_and_message, |this, (color, message)| {
                        this.child(
                            gpui::div().ml(Checkbox::container_size()).pl_1p5().child(
                                Text::new(message)
                                    .color(Color::from(color))
                                    .size(TextSize::Small),
                            ),
                        )
                    })
            })
            .collect::<Vec<_>>();
        let add_button = Button::new("variable-add", "Add Variable")
            .icon(IconAsset::Plus)
            .icon_size(IconSize::Small)
            .icon_color(Color::Muted)
            .variant(ButtonVariant::OutlinedGhost)
            .size(ButtonSize::Medium)
            .on_click(cx.listener(|project_config_editor, _, window, cx| {
                let row = Self::new_variable_row(None, window, cx);
                if let ProjectConfigEditorState::Ready(rows) = &mut project_config_editor.state {
                    rows.push(row);
                }
                project_config_editor.mark_edited(cx);
            }));
        let theme_colors = cx.theme().colors();

        gpui::div()
            .flex()
            .flex_col()
            .track_focus(&self.focus_handle)
            .w_full()
            .flex_1()
            .min_h_0()
            .bg(theme_colors.panel_background)
            .child(
                gpui::div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .child(
                        gpui::div()
                            .id("variables")
                            .flex()
                            .flex_col()
                            .track_scroll(&self.variables_scroll_handle)
                            .size_full()
                            .min_w_0()
                            .overflow_y_scroll()
                            .pl_2()
                            .pr_6()
                            .gap_2()
                            .py_3()
                            .children(rows)
                            .child(gpui::div().flex().items_center().pl_1().child(add_button)),
                    )
                    .custom_scrollbars(
                        Scrollbars::new(ScrollAxes::Vertical)
                            .id("variables-scrollbar")
                            .tracked_scroll_handle(&self.variables_scroll_handle)
                            .with_track_along(
                                ScrollAxes::Vertical,
                                theme_colors.scrollbar_track_background,
                                TrackLayout::Overlay,
                            ),
                        window,
                        cx,
                    ),
            )
    }
}

impl EventEmitter<ProjectConfigEditorEvent> for ProjectConfigEditor {}

impl Focusable for ProjectConfigEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ProjectConfigEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match &self.state {
            ProjectConfigEditorState::Ready(rows) => self.render_variables(rows, window, cx),
            ProjectConfigEditorState::Invalid { error } => self.render_invalid(error, cx),
        }
        .key_context("ProjectConfigEditor")
    }
}
