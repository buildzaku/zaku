use gpui::{
    AnyElement, App, AppContext, Context, Entity, EntityId, FontWeight, SharedString, Task,
    WeakEntity, Window, prelude::*,
};

use path::PathExt;
use project::{Project, RequestBuffer, RequestFileState};
use ui::{Color, Icon, IconName, IconSize, Label, LabelCommon, LabelSize};
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

    fn to_item_events(event: &Self::Event, f: &mut dyn FnMut(ItemEvent)) {
        match event {
            RequestEditorEvent::Saved
            | RequestEditorEvent::TitleChanged
            | RequestEditorEvent::DirtyChanged => {
                f(ItemEvent::UpdateTab);
            }
            RequestEditorEvent::RequestBufferEdited => f(ItemEvent::Edit),
            RequestEditorEvent::FileHandleChanged => {}
        }
    }

    fn tab_content_text(&self, detail: usize, cx: &App) -> SharedString {
        self.path_for_request(detail, true, cx)
            .unwrap_or_else(|| self.title(cx))
    }

    fn tab_content(&self, params: TabContentParams, _window: &Window, cx: &App) -> AnyElement {
        let selected_method_label = match &self.request {
            RequestEditorState::Ready(request) => {
                Some(project::request_method_label(request.http.method.as_str()))
            }
            RequestEditorState::Invalid { .. } => None,
        };
        let title = Label::new(truncate_and_trailoff(&self.title(cx), MAX_TAB_TITLE_LEN))
            .color(params.text_color())
            .when(params.preview, |this| this.italic());
        let description = params.detail.and_then(|detail| {
            let path = self.path_for_request(detail, false, cx)?;
            let description = path.trim();

            if description.is_empty() {
                return None;
            }

            Some(truncate_and_trailoff(description, MAX_TAB_TITLE_LEN))
        });

        ui::h_flex()
            .min_w_0()
            .gap_2()
            .when(
                matches!(&self.request, RequestEditorState::Invalid { .. }),
                |this| {
                    this.child(
                        ui::h_flex().flex_none().items_center().child(
                            Icon::new(IconName::WarningCircle)
                                .size(IconSize::Small)
                                .color(Color::Error),
                        ),
                    )
                },
            )
            .when_some(selected_method_label, |this, method| {
                this.child(
                    ui::h_flex().flex_none().items_center().child(
                        Label::new(method)
                            .size(LabelSize::Small)
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
                    Label::new(description)
                        .size(LabelSize::XSmall)
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
        f: &mut dyn FnMut(EntityId, &dyn project::ProjectItem),
    ) {
        f(Entity::entity_id(&self.buffer), self.buffer.read(cx));
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
            return Task::ready(Err(anyhow::anyhow!("Cannot save invalid request")));
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
                return Task::ready(Err(anyhow::anyhow!(
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
            return Task::ready(Err(anyhow::anyhow!(
                "Unable to deserialize request editor: path is not in project: {}",
                path.display()
            )));
        };

        let Some(open_buffer) =
            <RequestBuffer as project::ProjectItem>::try_open(&project, &project_path, cx)
        else {
            return Task::ready(Err(anyhow::anyhow!(
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

    use path::rel_path;
    use settings::SettingsStore;
    use theme::LoadThemes;
    use util_macros::path;
    use workspace::{SharedState, WorkspaceDb, build_workspace};

    fn init_test(shared_state: Arc<SharedState>, cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
            theme::init(LoadThemes::JustBase, cx);
            workspace::init(shared_state, cx);
            editor::init(cx);
            crate::init(cx);
            response_panel::init(cx);
        });
    }

    #[gpui::test]
    async fn test_deserialize(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let shared_state = cx.update(SharedState::test);
        let temp_fs = shared_state.fs.as_temp();
        init_test(shared_state, cx);

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
                request.http.url.read(cx).text(cx),
                "https://api.zaku.dev/create"
            );
            assert_eq!(
                request_editor.buffer.read(cx).file().path.as_ref(),
                rel_path("collection/request.toml")
            );
        });
    }
}
