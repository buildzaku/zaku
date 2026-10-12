use anyhow::anyhow;
use gpui::{AnyElement, App, Context, Entity, EntityId, SharedString, Task, Window, prelude::*};

use editor::items::{entry_git_aware_text_color, entry_text_color};
use path::PathExt;
use project::{ConfigFileState, ConfigFileType, Project, ProjectConfigBuffer};
use settings::{GitSettings, Settings};
use ui::{Color, Icon, IconAsset, IconSize, Text, TextCommon};
use util::truncate_and_trailoff;
use workspace::{
    Item, ItemBufferKind, ItemEvent, ProjectItem, TabContentParams, ToolbarItemLocation, pane::Pane,
};

use crate::{ProjectConfigEditor, ProjectConfigEditorEvent, ProjectConfigEditorState};

const MAX_TAB_TITLE_LEN: usize = 24;

impl Item for ProjectConfigEditor {
    type Event = ProjectConfigEditorEvent;

    fn to_item_events(event: &Self::Event, emitter: &mut dyn FnMut(ItemEvent)) {
        match event {
            ProjectConfigEditorEvent::TitleChanged => {
                emitter(ItemEvent::UpdateTab);
                emitter(ItemEvent::UpdateBreadcrumbs);
            }
            ProjectConfigEditorEvent::Saved | ProjectConfigEditorEvent::DirtyChanged => {
                emitter(ItemEvent::UpdateTab);
            }
            ProjectConfigEditorEvent::ProjectConfigBufferEdited => emitter(ItemEvent::Edit),
            ProjectConfigEditorEvent::FileHandleChanged => {}
        }
    }

    fn breadcrumb_location(&self, _: &App) -> ToolbarItemLocation {
        ToolbarItemLocation::PrimaryLeft
    }

    fn breadcrumbs(&self, cx: &App) -> Option<Vec<SharedString>> {
        let project_path = self.project_path(cx)?;
        let path = project_path.path.display(self.path_style(cx)).into_owned();

        Some(vec![path.into()])
    }

    fn tab_content_text(&self, _detail: usize, cx: &App) -> SharedString {
        self.title(cx)
    }

    fn tab_content(&self, params: TabContentParams, _: &Window, cx: &App) -> AnyElement {
        let git_settings = GitSettings::get_global(cx);
        let git_status_enabled =
            git_settings.is_git_status_enabled() && git_settings.status.tabs.colors;
        let text_color = if git_status_enabled {
            self.project_path(cx)
                .and_then(|project_path| {
                    let project = self.project.read(cx);
                    let entry = project.entry_for_path(&project_path, cx)?;
                    let git_status = project
                        .project_path_git_status(&project_path, cx)
                        .map(|status| status.summary())
                        .unwrap_or_default();

                    Some(entry_git_aware_text_color(
                        git_status,
                        entry.is_ignored,
                        params.selected,
                    ))
                })
                .unwrap_or_else(|| entry_text_color(params.selected))
        } else {
            entry_text_color(params.selected)
        };
        let was_deleted = self.buffer.read(cx).file().disk_state.is_deleted();
        let title = Text::new(truncate_and_trailoff(&self.title(cx), MAX_TAB_TITLE_LEN))
            .single_line()
            .color(text_color)
            .when(params.preview, |this| this.italic())
            .when(was_deleted, |this| this.strikethrough());
        let icon = match (&self.state, self.buffer.read(cx).file_type()) {
            (ProjectConfigEditorState::Invalid { .. }, _) => Icon::new(IconAsset::WarningCircle)
                .size(IconSize::Small)
                .color(Color::Error),
            (ProjectConfigEditorState::Ready(_), ConfigFileType::Project) => {
                Icon::new(IconAsset::SquareInset)
                    .size(IconSize::XSmall)
                    .color(text_color)
            }
            (ProjectConfigEditorState::Ready(_), ConfigFileType::Environment) => {
                environment_selector::environment_icon(self.environment_color(cx), text_color)
            }
        };

        gpui::div()
            .flex()
            .items_center()
            .min_w_0()
            .gap_2()
            .child(gpui::div().flex_none().flex().items_center().child(icon))
            .child(title)
            .into_any_element()
    }

    fn tab_tooltip_text(&self, cx: &App) -> Option<SharedString> {
        let project_path = self.project_path(cx)?;
        self.project
            .read(cx)
            .absolute_path(&project_path, cx)
            .map(|path| {
                ui::utils::replace_control_characters(&path.compact().to_string_lossy())
                    .into_owned()
                    .into()
            })
    }

    fn for_each_project_item(
        &self,
        cx: &App,
        visitor: &mut dyn FnMut(EntityId, &dyn project::ProjectItem),
    ) {
        visitor(Entity::entity_id(&self.buffer), self.buffer.read(cx));
    }

    fn buffer_kind(&self, _: &App) -> ItemBufferKind {
        ItemBufferKind::Singleton
    }

    fn is_dirty(&self, cx: &App) -> bool {
        self.buffer.read(cx).is_dirty()
    }

    fn has_conflict(&self, cx: &App) -> bool {
        self.buffer.read(cx).has_conflict()
    }

    fn can_save(&self, cx: &App) -> bool {
        matches!(&self.state, ProjectConfigEditorState::Ready(_)) && self.project_path(cx).is_some()
    }

    fn save(
        &mut self,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        let buffer = self.buffer.clone();
        let ProjectConfigEditorState::Ready(rows) = &self.state else {
            return Task::ready(Err(anyhow!("Cannot save invalid config file")));
        };
        let variables = crate::variables_from_rows(rows, cx);
        if crate::validation_states(&variables)
            .iter()
            .any(|validation_state| matches!(validation_state, crate::ValidationState::Error(_)))
        {
            return Task::ready(Err(anyhow!("Cannot save invalid variable names")));
        }
        buffer.update(cx, |buffer, cx| {
            if let ConfigFileState::Parsed(config_file) = buffer.config_file() {
                let mut config_file = config_file.clone();
                config_file.set_variables(variables.clone());
                buffer.set_config_file(ConfigFileState::Parsed(config_file), cx);
            }
        });
        cx.spawn_in(window, async move |this, cx| {
            project
                .update(cx, |project, cx| {
                    project.save_project_config_buffer(&buffer, cx)
                })
                .await?;
            this.update(cx, |project_config_editor, cx| {
                project_config_editor.saved_variables = Some(variables);
                cx.notify();
            })?;
            Ok(())
        })
    }

    fn reload(
        &mut self,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        let buffer = self.buffer.clone();
        let reload_task = project.update(cx, |project, cx| {
            project.reload_project_config_buffer(&buffer, cx)
        });

        cx.spawn_in(window, async move |_, _| {
            reload_task.await?;
            anyhow::Ok(())
        })
    }
}

impl ProjectItem for ProjectConfigEditor {
    type Item = ProjectConfigBuffer;

    fn for_project_item(
        project: Entity<Project>,
        _: Option<&Pane>,
        item: Entity<Self::Item>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self
    where
        Self: Sized,
    {
        Self::for_buffer(project, item, window, cx)
    }
}
