use anyhow::anyhow;
use gpui::{
    AnyElement, App, AppContext, Context, Entity, EntityId, FontWeight, SharedString, Task,
    WeakEntity, Window, prelude::*,
};

use editor::items::{entry_git_aware_text_color, entry_text_color};
use path::PathExt;
use project::{Project, RequestBuffer, RequestFileState};
use settings::{GitSettings, Settings};
use ui::{Color, Icon, IconAsset, IconSize, Text, TextCommon, TextSize};
use util::truncate_and_trailoff;
use workspace::{
    Item, ItemBufferKind, ItemEvent, ItemId, ProjectItem, SerializableItem, TabContentParams,
    Workspace, WorkspaceId, delete_unloaded_items, pane::Pane,
};

use crate::{
    RequestEditor, RequestEditorEvent, RequestEditorState, RequestSnapshot,
    persistence::{RequestEditorDb, SerializedRequestEditor},
};

const MAX_TAB_TITLE_LEN: usize = 24;

impl Item for RequestEditor {
    type Event = RequestEditorEvent;

    fn to_item_events(event: &Self::Event, emitter: &mut dyn FnMut(ItemEvent)) {
        match event {
            RequestEditorEvent::Saved
            | RequestEditorEvent::TitleChanged
            | RequestEditorEvent::DirtyChanged => {
                emitter(ItemEvent::UpdateTab);
            }
            RequestEditorEvent::RequestBufferEdited => emitter(ItemEvent::Edit),
            RequestEditorEvent::FileHandleChanged => {}
        }
    }

    fn tab_content_text(&self, detail: usize, cx: &App) -> SharedString {
        self.path_for_request(detail, true, cx)
            .unwrap_or_else(|| self.title(cx))
    }

    fn tab_content(&self, params: TabContentParams, _window: &Window, cx: &App) -> AnyElement {
        let request_method = match &self.request {
            RequestEditorState::Ready(request) => Some(project::request_method_short_name(
                request.http.method.as_str(),
            )),
            RequestEditorState::Invalid { .. } => None,
        };
        let git_settings = GitSettings::get_global(cx);
        let git_status_enabled =
            git_settings.is_git_status_enabled() && git_settings.status.tabs.colors;
        let text_color = if git_status_enabled {
            project::ProjectItem::project_path(self.buffer.read(cx), cx)
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
        let title = Text::new(truncate_and_trailoff(&self.title(cx), MAX_TAB_TITLE_LEN))
            .color(text_color)
            .when(params.preview, |this| this.italic());
        let description = params.detail.and_then(|detail| {
            let path = self.path_for_request(detail, false, cx)?;
            let description = path.trim();

            if description.is_empty() {
                return None;
            }

            Some(truncate_and_trailoff(description, MAX_TAB_TITLE_LEN))
        });

        gpui::div()
            .flex()
            .items_center()
            .min_w_0()
            .gap_2()
            .when(
                matches!(&self.request, RequestEditorState::Invalid { .. }),
                |this| {
                    this.child(
                        gpui::div().flex_none().flex().items_center().child(
                            Icon::new(IconAsset::WarningCircle)
                                .size(IconSize::Small)
                                .color(Color::Error),
                        ),
                    )
                },
            )
            .when_some(request_method, |this, request_method| {
                this.child(
                    gpui::div().flex_none().flex().items_center().child(
                        Text::new(request_method)
                            .size(TextSize::Small)
                            .weight(FontWeight::MEDIUM)
                            .color(Color::Muted)
                            .alpha(0.7)
                            .single_line(),
                    ),
                )
            })
            .child(title)
            .when_some(description, |this, description| {
                this.child(
                    Text::new(description)
                        .size(TextSize::XSmall)
                        .color(Color::Muted),
                )
            })
            .into_any_element()
    }

    fn tab_tooltip_text(&self, cx: &App) -> Option<SharedString> {
        let project_path = project::ProjectItem::project_path(self.buffer.read(cx), cx)?;
        self.project
            .read(cx)
            .absolute_path(&project_path, cx)
            .map(|path| path.compact().to_string_lossy().into_owned().into())
    }

    fn for_each_project_item(
        &self,
        cx: &App,
        visitor: &mut dyn FnMut(EntityId, &dyn project::ProjectItem),
    ) {
        visitor(Entity::entity_id(&self.buffer), self.buffer.read(cx));
    }

    fn buffer_kind(&self, _cx: &App) -> ItemBufferKind {
        ItemBufferKind::Singleton
    }

    fn is_dirty(&self, cx: &App) -> bool {
        self.buffer.read(cx).is_dirty()
    }

    fn can_save(&self, cx: &App) -> bool {
        matches!(&self.request, RequestEditorState::Ready(_)) && self.project_path(cx).is_some()
    }

    fn save(
        &mut self,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        let buffer = self.buffer.clone();
        let RequestEditorState::Ready(request) = &self.request else {
            return Task::ready(Err(anyhow!("Cannot save invalid request")));
        };
        let request_snapshot = RequestSnapshot::from_request(request, cx);
        buffer.update(cx, |buffer, cx| {
            buffer.set_request_file(RequestFileState::Parsed(request_snapshot.0.clone()), cx);
        });
        cx.spawn_in(window, async move |this, cx| {
            project
                .update(cx, |project, cx| project.save_request_buffer(&buffer, cx))
                .await?;
            this.update(cx, |request_editor, cx| {
                request_editor.request_snapshot = Some(request_snapshot.clone());
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
        let reload_task =
            project.update(cx, |project, cx| project.reload_request_buffer(&buffer, cx));

        cx.spawn_in(window, async move |_, _| {
            reload_task.await?;
            anyhow::Ok(())
        })
    }
}

impl ProjectItem for RequestEditor {
    type Item = RequestBuffer;

    fn for_project_item(
        project: Entity<Project>,
        pane: Option<&Pane>,
        item: Entity<Self::Item>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self
    where
        Self: Sized,
    {
        let workspace = pane.map_or_else(WeakEntity::new_invalid, Pane::workspace);
        Self::for_buffer(workspace, project, item, window, cx)
    }
}

impl SerializableItem for RequestEditor {
    fn serialized_item_kind() -> &'static str {
        "RequestEditor"
    }

    fn cleanup(
        workspace_id: WorkspaceId,
        alive_items: Vec<ItemId>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<()>> {
        delete_unloaded_items(
            alive_items,
            workspace_id,
            "request_editor",
            &RequestEditorDb::global(cx),
            cx,
        )
    }

    fn deserialize(
        project: Entity<Project>,
        workspace: WeakEntity<Workspace>,
        workspace_id: WorkspaceId,
        item_id: ItemId,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<Entity<Self>>> {
        let serialized_request_editor = match RequestEditorDb::global(cx)
            .load_serialized_request_editor(item_id, workspace_id)
        {
            Ok(Some(serialized_request_editor)) => serialized_request_editor,
            Ok(None) => {
                return Task::ready(Err(anyhow!(
                    "Unable to deserialize request editor: No entry in database for item_id: {item_id} and workspace_id {workspace_id:?}"
                )));
            }
            Err(error) => return Task::ready(Err(error)),
        };
        let path = serialized_request_editor.absolute_path;

        let Some(project_path) = project
            .read(cx)
            .project_path_for_absolute_path(path.as_path(), cx)
        else {
            return Task::ready(Err(anyhow!(
                "Unable to deserialize request editor: path is not in project: {}",
                path.display()
            )));
        };

        let Some(open_buffer) =
            <RequestBuffer as project::ProjectItem>::try_open(&project, &project_path, cx)
        else {
            return Task::ready(Err(anyhow!(
                "Unable to deserialize request editor: cannot open path: {}",
                path.display()
            )));
        };

        window.spawn(cx, async move |cx| {
            let buffer = open_buffer.await?;
            cx.update(|window, cx| {
                cx.new(|cx| RequestEditor::for_buffer(workspace, project, buffer, window, cx))
            })
        })
    }

    fn serialize(
        &mut self,
        workspace: &mut Workspace,
        item_id: ItemId,
        _closing: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<anyhow::Result<()>>> {
        let project_path = project::ProjectItem::project_path(self.buffer.read(cx), cx)?;
        let path = self.project.read(cx).absolute_path(&project_path, cx)?;
        let workspace_id = workspace.database_id()?;
        let request_editor_db = RequestEditorDb::global(cx);

        Some(cx.spawn_in(window, async move |_, _| {
            request_editor_db
                .save_serialized_request_editor(
                    item_id,
                    workspace_id,
                    SerializedRequestEditor {
                        absolute_path: path,
                    },
                )
                .await
        }))
    }

    fn should_serialize(&self, event: &Self::Event) -> bool {
        matches!(
            event,
            RequestEditorEvent::Saved
                | RequestEditorEvent::DirtyChanged
                | RequestEditorEvent::RequestBufferEdited
                | RequestEditorEvent::FileHandleChanged
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use gpui::TestAppContext;
    use indoc::indoc;
    use serde_json::json;
    use std::sync::Arc;

    use fs::TempFs;
    use path::rel_path;
    use settings::SettingsStore;
    use theme::LoadThemes;
    use util_macros::path;
    use workspace::{AppState, WorkspaceDb, build_workspace};

    fn init_test(app_state: Arc<AppState>, cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test_new(cx);
            cx.set_global(settings_store);
            theme::init(LoadThemes::JustBase, cx);
            workspace::init(app_state, cx);
            editor::init(cx);
            crate::init(cx);
            response_panel::init(cx);
        });
    }

    #[gpui::test]
    async fn test_deserialize(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
        init_test(app_state, cx);

        temp_fs.insert_tree(
            path!("project"),
            json!({
                "collection": {
                    "request.toml": indoc! {r#"
                        [meta]
                        version = 1

                        [http]
                        method = "POST"
                        url = "https://api.zaku.dev/create"
                    "#}
                }
            }),
        );

        let project_path = temp_fs.path().join(path!("project"));
        let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;
        let (workspace, cx) = build_workspace(&project, cx);
        let workspace_db = cx.update(|_, cx| WorkspaceDb::global(cx));
        let request_editor_db = cx.update(|_, cx| RequestEditorDb::global(cx));
        let workspace_id = workspace_db.next_id().await.unwrap();
        let item_id: ItemId = 1;
        let serialized_request_editor = SerializedRequestEditor {
            absolute_path: project_path.join(path!("collection/request.toml")),
        };

        request_editor_db
            .save_serialized_request_editor(item_id, workspace_id, serialized_request_editor)
            .await
            .unwrap();

        let weak_workspace = workspace.downgrade();
        let request_editor = workspace
            .update_in(cx, |_, window, cx| {
                RequestEditor::deserialize(
                    project.clone(),
                    weak_workspace.clone(),
                    workspace_id,
                    item_id,
                    window,
                    cx,
                )
            })
            .await
            .unwrap();

        request_editor.read_with(cx, |request_editor, cx| {
            let RequestEditorState::Ready(request) = &request_editor.request else {
                panic!("Expected request editor to be ready");
            };

            assert_eq!(request.http.method.as_str(), "POST");
            assert_eq!(
                request.http.url.read(cx).value(cx),
                "https://api.zaku.dev/create"
            );
            assert_eq!(
                request_editor.buffer.read(cx).file().path.as_ref(),
                rel_path("collection/request.toml")
            );
        });
    }
}
