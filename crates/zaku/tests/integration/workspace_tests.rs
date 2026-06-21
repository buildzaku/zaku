use gpui::TestAppContext;
use indoc::indoc;
use std::sync::Arc;
use uuid::Uuid;

use db::{AppDatabase, kv::KeyValueStore};
use session::Session;
use settings::SettingsStore;
use theme::LoadThemes;
use workspace::{OpenMode, OpenResult, Root, SharedState, Workspace, WorkspaceDb};
use worktree::WorktreeModelHandle;

fn init_test(shared_state: Arc<SharedState>, app_db: AppDatabase, cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(app_db);

        let settings_store = SettingsStore::test_new(cx);
        cx.set_global(settings_store);
        theme::init(LoadThemes::JustBase, cx);
        workspace::init(shared_state, cx);
        project_panel::init(cx);
        editor::init(cx);
        request_editor::init(cx);
        response_panel::init(cx);
    });
}

#[gpui::test]
async fn test_restore_last_session_with_multiple_workspaces(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let app_db = AppDatabase::test_new();
    let kv_store = KeyValueStore::open(&app_db);
    let session = Session::new(Uuid::new_v4().to_string(), kv_store.clone()).await;

    let shared_state = cx.update(SharedState::test);
    let temp_fs = shared_state.fs.as_temp();
    cx.update(|cx| {
        shared_state
            .session
            .update(cx, |app_session, _cx| app_session.replace_session(session));
    });
    init_test(shared_state.clone(), app_db, cx);

    for project_path in ["first", "second", "third", "fourth"] {
        temp_fs.insert_tree(
            project_path,
            serde_json::json!({
                "collection": {
                    "request.toml": indoc! {"
                        [meta]
                        version = 1
                    "},
                }
            }),
        );
    }

    let first_path = temp_fs.path().join("first");
    let second_path = temp_fs.path().join("second");
    let third_path = temp_fs.path().join("third");
    let fourth_path = temp_fs.path().join("fourth");

    let mut open_results = Vec::new();
    for path in [
        first_path.clone(),
        second_path.clone(),
        third_path.clone(),
        fourth_path.clone(),
    ] {
        let result = cx
            .update(|cx| Workspace::open(path, shared_state.clone(), None, OpenMode::NewWindow, cx))
            .await
            .unwrap();

        result
            .workspace
            .read_with(cx, |workspace, cx| workspace.worktree_scan_complete(cx))
            .await;
        let worktree = result.workspace.read_with(cx, |workspace, cx| {
            workspace.project().read(cx).root_worktree(cx).unwrap()
        });
        worktree.flush_fs_events(cx).await;
        result
            .window
            .update(cx, |root, window, cx| {
                root.workspace().update(cx, |workspace, cx| {
                    workspace.flush_serialization(window, cx)
                })
            })
            .unwrap()
            .await;

        open_results.push(result);
    }
    let [first_result, second_result, third_result, fourth_result]: [OpenResult; 4] =
        open_results.try_into().ok().unwrap();

    let session_id = cx.update(|cx| shared_state.session.read(cx).id().to_owned());
    let workspace_db = cx.update(|cx| WorkspaceDb::global(cx));
    let session_workspaces = workspace::last_session_workspace_locations(
        &workspace_db,
        &session_id,
        None,
        temp_fs.as_ref(),
    )
    .await
    .unwrap();

    assert_eq!(session_workspaces.len(), 4);

    session::save_window_stack(
        kv_store.clone(),
        &[
            second_result.window.window_id().as_u64(),
            fourth_result.window.window_id().as_u64(),
            third_result.window.window_id().as_u64(),
            first_result.window.window_id().as_u64(),
        ],
    )
    .await;

    for window in [
        &first_result.window,
        &second_result.window,
        &third_result.window,
        &fourth_result.window,
    ] {
        window
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
    }
    cx.run_until_parked();

    let restored_session = Session::new(Uuid::new_v4().to_string(), kv_store).await;
    cx.update(|cx| {
        shared_state.session.update(cx, |app_session, _cx| {
            app_session.replace_session(restored_session);
        });
    });

    let mut async_cx = cx.to_async();
    zaku::restore_or_create_workspace(shared_state.clone(), &mut async_cx)
        .await
        .unwrap();

    let restored_windows = cx.read(|cx| {
        cx.windows()
            .into_iter()
            .filter_map(|window| window.downcast::<Root>())
            .collect::<Vec<_>>()
    });

    assert_eq!(restored_windows.len(), 4);

    for window in &restored_windows {
        let workspace = window
            .read_with(cx, |root, _| root.workspace().clone())
            .unwrap();
        workspace
            .read_with(cx, |workspace, cx| workspace.worktree_scan_complete(cx))
            .await;
        let worktree = workspace.read_with(cx, |workspace, cx| {
            workspace.project().read(cx).root_worktree(cx).unwrap()
        });
        worktree.flush_fs_events(cx).await;
        window
            .update(cx, |root, window, cx| {
                root.workspace().update(cx, |workspace, cx| {
                    workspace.flush_serialization(window, cx)
                })
            })
            .unwrap()
            .await;
    }

    let recent_workspace_paths = workspace_db
        .recent_workspaces_on_disk(temp_fs.as_ref())
        .await
        .unwrap()
        .into_iter()
        .map(|(_, location, _)| location)
        .collect::<Vec<_>>();

    for window in &restored_windows {
        window
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
    }
    cx.run_until_parked();

    assert_eq!(
        recent_workspace_paths,
        vec![second_path, fourth_path, third_path, first_path],
        "recent workspaces should preserve window stack order"
    );
}
