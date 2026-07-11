use anyhow::Context as _;
use gpui::{
    Action, App, ClickEvent, Context, CursorStyle, DismissEvent, Entity, EventEmitter, FocusHandle,
    Focusable, MouseButton, PathPromptOptions, Render, SharedString, Subscription, WeakEntity,
    Window, prelude::*,
};
use std::path::{Component, Path, PathBuf};

use input::{ErasedEditorEvent, InputField};
use theme::ActiveTheme;
use ui::{
    Button, ButtonCommon, ButtonSize, ButtonVariant, Clickable, Color, Disableable, Headline,
    HeadlineSize, StyledExt, Text, TextCommon, TextSize,
};

use crate::{DismissDecision, ModalView, OpenMode, Workspace, notifications::DetachAndPromptErr};

pub(crate) struct CreateProjectModal {
    focus_handle: FocusHandle,
    workspace: WeakEntity<Workspace>,
    project_name: Entity<InputField>,
    location: PathBuf,
    is_creating: bool,
    error: Option<SharedString>,
    _project_name_subscription: Subscription,
}

impl CreateProjectModal {
    pub(crate) fn new(
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let project_name = cx.new(|cx| {
            InputField::new(window, cx, "My Project")
                .label("Name")
                .tab_index(0)
        });
        let create_project_modal = cx.weak_entity();
        let name_editor = project_name.read(cx).editor().clone();
        let project_name_subscription = name_editor.subscribe(
            Box::new(move |event, _, cx| {
                if event == ErasedEditorEvent::BufferEdited
                    && let Err(error) = create_project_modal.update(cx, |modal, cx| {
                        modal.error = None;
                        cx.notify();
                    })
                {
                    log::debug!("Failed to update create project modal input state: {error:?}");
                }
            }),
            window,
            cx,
        );

        Self {
            focus_handle: cx.focus_handle(),
            workspace,
            project_name,
            location: path::home_dir().clone(),
            is_creating: false,
            error: None,
            _project_name_subscription: project_name_subscription,
        }
    }

    fn set_error(&mut self, error: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.is_creating = false;
        self.error = Some(error.into());
        let name_editor = self.project_name.read(cx).editor().clone();
        name_editor.set_read_only(false, cx);
        cx.notify();
    }

    fn cancel(&mut self, _: &actions::menu::Cancel, _: &mut Window, cx: &mut Context<Self>) {
        if !self.is_creating {
            cx.emit(DismissEvent);
        }
    }

    fn choose_location(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_creating {
            return;
        }

        let path_prompt = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Select".into()),
        });

        cx.spawn_in(window, async move |create_project_modal, cx| {
            let selection = match path_prompt.await {
                Ok(selection) => selection,
                Err(error) => {
                    log::debug!("Project location prompt dropped: {error:?}");
                    return;
                }
            };

            match selection {
                Ok(Some(paths)) => {
                    let Some(location) = paths.into_iter().next() else {
                        return;
                    };
                    if let Err(error) = create_project_modal.update(cx, |modal, cx| {
                        modal.location = location;
                        modal.error = None;
                        cx.notify();
                    }) {
                        log::debug!("Failed to update create project location: {error:?}");
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    if let Err(update_error) = create_project_modal.update(cx, |modal, cx| {
                        modal.set_error(format!("Failed to select project location: {error}"), cx);
                    }) {
                        log::debug!(
                            "Failed to show create project location error: {update_error:?}"
                        );
                    }
                }
            }
        })
        .detach();
    }

    fn create_project(
        &mut self,
        _: &actions::menu::Confirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_creating {
            return;
        }

        let project_name = self.project_name.read(cx).value(cx);
        let project_name = project_name.trim();
        if project_name.is_empty() {
            self.set_error("Enter a project name.", cx);
            return;
        }

        let mut components = Path::new(project_name).components();
        let is_single_normal_component =
            matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none();
        if !is_single_normal_component || project_name.contains('/') || project_name.contains('\\')
        {
            self.set_error("Project name must be a single directory name.", cx);
            return;
        }

        let project_path = self.location.join(project_name);
        let workspace = self.workspace.clone();
        let Some(workspace_entity) = workspace.upgrade() else {
            self.set_error("Workspace is no longer available.", cx);
            return;
        };
        let fs = workspace_entity.read(cx).app_state().fs.clone();

        self.is_creating = true;
        self.error = None;
        let name_editor = self.project_name.read(cx).editor().clone();
        name_editor.set_read_only(true, cx);
        cx.notify();

        cx.spawn_in(window, async move |create_project_modal, cx| {
            let create_result: anyhow::Result<bool> = async {
                let metadata = fs
                    .metadata(&project_path)
                    .await
                    .with_context(|| format!("checking project path {}", project_path.display()))?;
                if metadata.is_some() {
                    return Ok(false);
                }

                fs.create_dir(&project_path)
                    .await
                    .with_context(|| {
                        format!("creating project directory {}", project_path.display())
                    })?;
                Ok(true)
            }
            .await;

            match create_result {
                Ok(true) => {
                    if let Err(error) = create_project_modal.update(cx, |modal, cx| {
                        modal.is_creating = false;
                        let name_editor = modal.project_name.read(cx).editor().clone();
                        name_editor.set_read_only(false, cx);
                        cx.emit(DismissEvent);
                    }) {
                        log::debug!("Failed to dismiss create project modal: {error:?}");
                    }

                    if let Err(error) = workspace.update_in(cx, |workspace, window, cx| {
                        workspace
                            .open_workspace_for_path(
                                project_path,
                                OpenMode::NewWindow,
                                window,
                                cx,
                            )
                            .detach_and_prompt_err(
                                "Failed to open project",
                                window,
                                cx,
                                |_, _, _| None,
                            );
                    }) {
                        log::debug!("Failed to open created project: {error:?}");
                    }
                }
                Ok(false) => {
                    if let Err(error) = create_project_modal.update(cx, |modal, cx| {
                        modal.set_error(
                            "A file or directory already exists at this location. Choose a different name or location.",
                            cx,
                        );
                    }) {
                        log::debug!("Failed to show existing project path error: {error:?}");
                    }
                }
                Err(error) => {
                    if let Err(update_error) = create_project_modal.update(cx, |modal, cx| {
                        modal.set_error(format!("Failed to create project: {error:#}"), cx);
                    }) {
                        log::debug!("Failed to show project creation error: {update_error:?}");
                    }
                }
            }
        })
        .detach();
    }
}

impl EventEmitter<DismissEvent> for CreateProjectModal {}

impl Focusable for CreateProjectModal {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.project_name.focus_handle(cx)
    }
}

impl ModalView for CreateProjectModal {
    fn on_before_dismiss(&mut self, _: &mut Window, _: &mut Context<Self>) -> DismissDecision {
        if self.is_creating {
            DismissDecision::Pending
        } else {
            DismissDecision::Dismiss(true)
        }
    }
}

impl Render for CreateProjectModal {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let project_name = self.project_name.read(cx).value(cx);
        let create_disabled = self.is_creating || project_name.trim().is_empty();
        let focus_handle = self.focus_handle.clone();
        let location = self.location.to_string_lossy().into_owned();
        let create_label = if self.is_creating {
            "Creating…"
        } else {
            "Create"
        };

        gpui::div()
            .key_context("CreateProjectModal")
            .track_focus(&self.focus_handle)
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                window.focus(&focus_handle, cx);
            })
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::create_project))
            .flex()
            .flex_col()
            .w(gpui::rems(34.0))
            .overflow_hidden()
            .elevation_3(cx)
            .child(
                gpui::div()
                    .flex()
                    .items_center()
                    .w_full()
                    .px_3()
                    .pt_3()
                    .pb_2()
                    .child(Headline::new("New Project").size(HeadlineSize::Small)),
            )
            .child(
                gpui::div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .gap_3()
                    .px_3()
                    .pb_3()
                    .child(self.project_name.clone())
                    .child(
                        gpui::div()
                            .flex()
                            .flex_col()
                            .w_full()
                            .gap_1()
                            .child(Text::new("Location").size(TextSize::Small))
                            .child(
                                gpui::div()
                                    .id("create-project-location")
                                    .tab_index(1)
                                    .flex()
                                    .items_center()
                                    .w_full()
                                    .h_8()
                                    .overflow_hidden()
                                    .rounded_md()
                                    .bg(cx.theme().colors().editor_background)
                                    .border_1()
                                    .border_color(cx.theme().colors().border_variant)
                                    .when(self.is_creating, |this| {
                                        this.cursor(CursorStyle::Arrow).opacity(0.4)
                                    })
                                    .when(!self.is_creating, |this| {
                                        this.cursor_pointer()
                                            .on_click(cx.listener(Self::choose_location))
                                    })
                                    .child(
                                        gpui::div()
                                            .flex()
                                            .items_center()
                                            .flex_1()
                                            .h_full()
                                            .min_w_0()
                                            .px_2()
                                            .child(Text::new(location).truncate()),
                                    )
                                    .child(
                                        gpui::div()
                                            .flex()
                                            .items_center()
                                            .h_full()
                                            .px_2()
                                            .border_l_1()
                                            .border_color(cx.theme().colors().border_variant)
                                            .bg(cx.theme().colors().panel_tab_bar_background)
                                            .child(Text::new("Choose…").size(TextSize::Small)),
                                    ),
                            ),
                    )
                    .when_some(self.error.clone(), |this, error| {
                        this.child(Text::new(error).size(TextSize::Small).color(Color::Error))
                    }),
            )
            .child(
                gpui::div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .w_full()
                    .gap_1()
                    .p_3()
                    .border_t_1()
                    .border_color(cx.theme().colors().border_variant)
                    .child(
                        Button::new("create-project-cancel", "Cancel")
                            .tab_index(2)
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Medium)
                            .disabled(self.is_creating)
                            .on_click(|_, window, cx| {
                                window.dispatch_action(actions::menu::Cancel.boxed_clone(), cx);
                            }),
                    )
                    .child(
                        Button::new("create-project-create", create_label)
                            .tab_index(3)
                            .variant(ButtonVariant::Solid)
                            .size(ButtonSize::Medium)
                            .disabled(create_disabled)
                            .on_click(|_, window, cx| {
                                window.dispatch_action(actions::menu::Confirm.boxed_clone(), cx);
                            }),
                    ),
            )
    }
}
