use futures::channel::oneshot;
use gpui::{Entity, ListOffset, TestAppContext};
use indoc::indoc;
use parking_lot::Mutex;
use serde_json::json;
use std::{path::PathBuf, sync::Arc, time::Duration};
use uuid::Uuid;

use db::{AppDatabase, kv::KeyValueStore};
use fs::TempFs;
use http_client::{AsyncBody, FakeHttpClient, Response, StatusCode};
use path::rel_path;
use project::ProjectPath;
use response_panel::ResponsePanel;
use session::Session;
use settings::SettingsStore;
use theme::LoadThemes;
use workspace::{AppState, ItemHandle, OpenMode, OpenResult, Root, Workspace, WorkspaceDb};
use worktree::{Worktree, WorktreeModelHandle};

fn init_test(app_state: Arc<AppState>, app_db: AppDatabase, cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(app_db);

        let settings_store = SettingsStore::test_new(cx);
        cx.set_global(settings_store);
        theme::init(LoadThemes::JustBase, cx);
        workspace::init(app_state, cx);
        project_panel::init(cx);
        editor::init(cx);
        request_editor::init(cx);
        response_panel::init(cx);
        zaku::init(cx);
    });
}

async fn open_workspace(
    project_path: PathBuf,
    app_state: Arc<AppState>,
    cx: &mut TestAppContext,
) -> (OpenResult, Entity<Worktree>) {
    let open_result = cx
        .update(|cx| Workspace::open(project_path, app_state, None, OpenMode::NewWindow, cx))
        .await
        .expect("workspace should open");

    open_result
        .workspace
        .read_with(cx, |workspace, cx| workspace.worktree_scan_complete(cx))
        .await;
    let worktree = open_result.workspace.read_with(cx, |workspace, cx| {
        workspace
            .project()
            .read(cx)
            .root_worktree(cx)
            .expect("workspace should have a root worktree")
    });
    worktree.flush_fs_events(cx).await;

    (open_result, worktree)
}

async fn open_path(open_result: &OpenResult, path: ProjectPath, cx: &mut TestAppContext) {
    open_result
        .window
        .update(cx, |root, window, cx| {
            root.workspace().update(cx, |workspace, cx| {
                workspace.open_path(path, None, true, window, cx)
            })
        })
        .expect("window should update to open path")
        .await
        .expect("path should open");
}

async fn open_path_preview(
    open_result: &OpenResult,
    path: ProjectPath,
    cx: &mut TestAppContext,
) -> Box<dyn ItemHandle> {
    open_result
        .window
        .update(cx, |root, window, cx| {
            root.workspace().update(cx, |workspace, cx| {
                workspace.open_path_preview(path, None, false, true, true, window, cx)
            })
        })
        .expect("window should update to open preview path")
        .await
        .expect("preview path should open")
}

fn activate_item_for_path(open_result: &OpenResult, path: &str, cx: &mut TestAppContext) {
    let path = rel_path(path);
    let pane = open_result
        .workspace
        .read_with(cx, |workspace, _| workspace.pane().clone());
    let item_index = pane.read_with(cx, |pane, cx| {
        pane.items()
            .position(|item| {
                item.project_path(cx)
                    .is_some_and(|project_path| project_path.path.as_ref() == path)
            })
            .expect("pane should contain item for path")
    });

    open_result
        .window
        .update(cx, |_, window, cx| {
            pane.update(cx, |pane, cx| {
                pane.activate_item(item_index, true, false, window, cx);
            });
        })
        .expect("window should update to activate item");
    cx.run_until_parked();
}

#[gpui::test]
async fn test_restore_last_session_with_multiple_workspaces(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let app_db = AppDatabase::test_new();
    let kv_store = KeyValueStore::open(&app_db);
    let session = Session::new(Uuid::new_v4().to_string(), kv_store.clone()).await;
    let temp_fs = TempFs::new(cx.executor());
    let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));

    cx.update(|cx| {
        app_state
            .session
            .update(cx, |app_session, _cx| app_session.replace_session(session));
    });
    init_test(app_state.clone(), app_db, cx);

    for project_path in ["first", "second", "third", "fourth"] {
        temp_fs.insert_tree(
            project_path,
            json!({
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
        let (result, _) = open_workspace(path, app_state.clone(), cx).await;
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

    let session_id = cx.update(|cx| app_state.session.read(cx).id().to_owned());
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
        app_state.session.update(cx, |app_session, _cx| {
            app_session.replace_session(restored_session);
        });
    });

    let mut async_cx = cx.to_async();
    zaku::restore_or_create_workspace(app_state.clone(), &mut async_cx)
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

#[gpui::test]
async fn test_send_request_opens_response_panel(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let app_db = AppDatabase::test_new();
    let temp_fs = TempFs::new(cx.executor());
    let http_client = FakeHttpClient::with_response(StatusCode::NOT_FOUND);
    let app_state =
        cx.update(|cx| AppState::test_new(temp_fs.clone(), Some(http_client.clone()), cx));
    let (tx, rx) = oneshot::channel();
    let rx = Arc::new(Mutex::new(Some(rx)));

    http_client.replace_handler({
        move |_, request| {
            assert_eq!(request.uri().path(), "/me");
            let rx = rx.lock().take().unwrap();

            async move { Ok(rx.await.unwrap()) }
        }
    });

    init_test(app_state.clone(), app_db, cx);

    temp_fs.insert_tree(
        "project",
        json!({
            "collection": {
                "request.toml": indoc! {r#"
                    [meta]
                    version = 1

                    [http]
                    method = "GET"
                    url = "https://api.zaku.dev/me"
                "#}
            }
        }),
    );

    let project_path = temp_fs.path().join("project");
    let (open_result, worktree) = open_workspace(project_path, app_state.clone(), cx).await;
    let worktree_id = worktree.read_with(cx, |worktree, _| worktree.id());

    open_path(
        &open_result,
        ProjectPath::from((worktree_id, rel_path("collection/request.toml"))),
        cx,
    )
    .await;

    cx.dispatch_action(open_result.window.into(), actions::workspace::SendRequest);
    cx.run_until_parked();

    let response_panel = open_result
        .workspace
        .read_with(cx, |workspace, cx| workspace.panel::<ResponsePanel>(cx))
        .expect("response panel should be registered");
    open_result.workspace.read_with(cx, |workspace, cx| {
        let response_panel_id = Entity::entity_id(&response_panel);
        let active_panel_id = workspace
            .bottom_dock()
            .read(cx)
            .active_panel()
            .map(|panel| panel.panel_id());

        assert!(workspace.bottom_dock().read(cx).is_open());
        assert_eq!(active_panel_id, Some(response_panel_id));
    });

    let response = Response::builder()
        .status(StatusCode::OK)
        .body(AsyncBody::from("response"))
        .unwrap();
    assert!(
        matches!(tx.send(response), Ok(())),
        "response receiver should be active"
    );
}

#[gpui::test]
async fn test_each_request_editor_has_its_own_response(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let app_db = AppDatabase::test_new();
    let temp_fs = TempFs::new(cx.executor());
    let http_client = FakeHttpClient::with_response(StatusCode::NOT_FOUND);
    let app_state =
        cx.update(|cx| AppState::test_new(temp_fs.clone(), Some(http_client.clone()), cx));
    let (first_tx, first_rx) = oneshot::channel();
    let (second_tx, second_rx) = oneshot::channel();
    let first_rx = Arc::new(Mutex::new(Some(first_rx)));
    let second_rx = Arc::new(Mutex::new(Some(second_rx)));
    let first_response_delay = Duration::from_secs(5);
    let second_response_delay = Duration::from_secs(3);
    let executor = cx.executor();

    http_client.replace_handler({
        move |_, request| {
            let (rx, response_delay) = match request.uri().path() {
                "/first" => (first_rx.lock().take().unwrap(), first_response_delay),
                "/second" => (second_rx.lock().take().unwrap(), second_response_delay),
                path => panic!("Unexpected request path: {path}"),
            };
            let executor = executor.clone();

            async move {
                let response = rx.await.unwrap();
                executor.timer(response_delay).await;
                Ok(response)
            }
        }
    });

    init_test(app_state.clone(), app_db, cx);

    temp_fs.insert_tree(
        "project",
        json!({
            "collection": {
                "first.toml": indoc! {r#"
                    [meta]
                    version = 1

                    [http]
                    method = "GET"
                    url = "https://api.zaku.dev/first"
                "#},
                "second.toml": indoc! {r#"
                    [meta]
                    version = 1

                    [http]
                    method = "GET"
                    url = "https://api.zaku.dev/second"
                "#}
            }
        }),
    );

    let project_path = temp_fs.path().join("project");
    let (open_result, worktree) = open_workspace(project_path, app_state.clone(), cx).await;
    let worktree_id = worktree.read_with(cx, |worktree, _| worktree.id());
    let response_panel = open_result
        .workspace
        .read_with(cx, |workspace, cx| workspace.panel::<ResponsePanel>(cx))
        .expect("response panel should be registered");

    open_path(
        &open_result,
        ProjectPath::from((worktree_id, rel_path("collection/first.toml"))),
        cx,
    )
    .await;
    cx.dispatch_action(open_result.window.into(), actions::workspace::SendRequest);
    cx.run_until_parked();

    open_path(
        &open_result,
        ProjectPath::from((worktree_id, rel_path("collection/second.toml"))),
        cx,
    )
    .await;
    cx.dispatch_action(open_result.window.into(), actions::workspace::SendRequest);
    cx.run_until_parked();

    let response = Response::builder()
        .status(StatusCode::OK)
        .body(AsyncBody::from("first response"))
        .unwrap();
    assert!(
        matches!(first_tx.send(response), Ok(())),
        "response receiver should be active"
    );

    cx.executor().advance_clock(first_response_delay);
    cx.run_until_parked();

    assert_eq!(
        response_panel.read_with(cx, |response_panel, cx| response_panel.text(cx)),
        ""
    );

    let response = Response::builder()
        .status(StatusCode::OK)
        .body(AsyncBody::from("second response"))
        .unwrap();
    assert!(
        matches!(second_tx.send(response), Ok(())),
        "response receiver should be active"
    );

    cx.executor().advance_clock(second_response_delay);
    cx.run_until_parked();

    assert_eq!(
        response_panel.read_with(cx, |response_panel, cx| response_panel.text(cx)),
        "second response"
    );

    activate_item_for_path(&open_result, "collection/first.toml", cx);

    assert_eq!(
        response_panel.read_with(cx, |response_panel, cx| response_panel.text(cx)),
        "first response"
    );
}

#[gpui::test]
async fn test_send_request_with_preview_request_editor(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let app_db = AppDatabase::test_new();
    let temp_fs = TempFs::new(cx.executor());
    let http_client = FakeHttpClient::with_response(StatusCode::NOT_FOUND);
    let app_state =
        cx.update(|cx| AppState::test_new(temp_fs.clone(), Some(http_client.clone()), cx));
    let (first_tx, first_rx) = oneshot::channel();
    let (second_tx, second_rx) = oneshot::channel();
    let first_rx = Arc::new(Mutex::new(Some(first_rx)));
    let second_rx = Arc::new(Mutex::new(Some(second_rx)));

    http_client.replace_handler({
        move |_, request| {
            let rx = match request.uri().path() {
                "/first" => first_rx.lock().take().unwrap(),
                "/second" => second_rx.lock().take().unwrap(),
                path => panic!("Unexpected request path: {path}"),
            };
            async move { Ok(rx.await.unwrap()) }
        }
    });

    init_test(app_state.clone(), app_db, cx);

    temp_fs.insert_tree(
        "project",
        json!({
            "collection": {
                "first.toml": indoc! {r#"
                    [meta]
                    version = 1

                    [http]
                    method = "GET"
                    url = "https://api.zaku.dev/first"
                "#},
                "second.toml": indoc! {r#"
                    [meta]
                    version = 1

                    [http]
                    method = "GET"
                    url = "https://api.zaku.dev/second"
                "#}
            }
        }),
    );

    let project_path = temp_fs.path().join("project");
    let (open_result, worktree) = open_workspace(project_path, app_state.clone(), cx).await;
    let worktree_id = worktree.read_with(cx, |worktree, _| worktree.id());
    let response_panel = open_result
        .workspace
        .read_with(cx, |workspace, cx| workspace.panel::<ResponsePanel>(cx))
        .expect("response panel should be registered");
    let pane = open_result
        .workspace
        .read_with(cx, |workspace, _| workspace.pane().clone());

    let first_item = open_path_preview(
        &open_result,
        ProjectPath::from((worktree_id, rel_path("collection/first.toml"))),
        cx,
    )
    .await;
    cx.dispatch_action(open_result.window.into(), actions::workspace::SendRequest);
    cx.run_until_parked();
    assert!(pane.read_with(cx, |pane, _| pane.preview_item_idx().is_none()));

    open_path_preview(
        &open_result,
        ProjectPath::from((worktree_id, rel_path("collection/second.toml"))),
        cx,
    )
    .await;
    cx.dispatch_action(open_result.window.into(), actions::workspace::SendRequest);
    cx.run_until_parked();

    let first_item_id = first_item.item_id();
    assert!(pane.read_with(cx, |pane, _| {
        pane.items().any(|item| item.item_id() == first_item_id)
    }));

    let response = Response::builder()
        .status(StatusCode::OK)
        .body(AsyncBody::from("first response"))
        .unwrap();
    assert!(
        matches!(first_tx.send(response), Ok(())),
        "response receiver should be active"
    );

    cx.run_until_parked();

    assert_eq!(
        response_panel.read_with(cx, |response_panel, cx| response_panel.text(cx)),
        ""
    );

    let response = Response::builder()
        .status(StatusCode::OK)
        .body(AsyncBody::from("second response"))
        .unwrap();
    assert!(
        matches!(second_tx.send(response), Ok(())),
        "response receiver should be active"
    );

    cx.run_until_parked();

    assert_eq!(
        response_panel.read_with(cx, |response_panel, cx| response_panel.text(cx)),
        "second response"
    );

    activate_item_for_path(&open_result, "collection/first.toml", cx);

    assert_eq!(
        response_panel.read_with(cx, |response_panel, cx| response_panel.text(cx)),
        "first response"
    );
}

#[gpui::test]
async fn test_switching_request_editor_tab_preserves_response_panel_scroll(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();

    let app_db = AppDatabase::test_new();
    let temp_fs = TempFs::new(cx.executor());
    let http_client = FakeHttpClient::with_response(StatusCode::NOT_FOUND);
    let app_state =
        cx.update(|cx| AppState::test_new(temp_fs.clone(), Some(http_client.clone()), cx));

    http_client.replace_handler({
        move |_, request| {
            let prefix = match request.uri().path() {
                "/first" => "first",
                "/second" => "second",
                path => panic!("Unexpected request path: {path}"),
            };

            async move {
                let mut response = Response::builder().status(StatusCode::OK);
                for header_index in 0..50 {
                    response = response.header(
                        format!("x-{prefix}-header-{header_index}"),
                        format!("{prefix} header {header_index}"),
                    );
                }
                for cookie_index in 0..25 {
                    response = response.header(
                        "set-cookie",
                        format!(
                            "{prefix}-cookie-{cookie_index}=value-{cookie_index}; \
                            Path=/; Domain=zaku.dev; Secure; HttpOnly; SameSite=Lax"
                        ),
                    );
                }

                Ok(response
                    .body(AsyncBody::from(format!("{prefix} response")))
                    .unwrap())
            }
        }
    });

    init_test(app_state.clone(), app_db, cx);

    temp_fs.insert_tree(
        "project",
        json!({
            "collection": {
                "first.toml": indoc! {r#"
                    [meta]
                    version = 1

                    [http]
                    method = "GET"
                    url = "https://api.zaku.dev/first"
                "#},
                "second.toml": indoc! {r#"
                    [meta]
                    version = 1

                    [http]
                    method = "GET"
                    url = "https://api.zaku.dev/second"
                "#}
            }
        }),
    );

    let project_path = temp_fs.path().join("project");
    let (open_result, worktree) = open_workspace(project_path, app_state.clone(), cx).await;

    let worktree_id = worktree.read_with(cx, |worktree, _| worktree.id());
    let response_panel = open_result
        .workspace
        .read_with(cx, |workspace, cx| workspace.panel::<ResponsePanel>(cx))
        .expect("response panel should be registered");

    open_path(
        &open_result,
        ProjectPath::from((worktree_id, rel_path("collection/first.toml"))),
        cx,
    )
    .await;

    cx.dispatch_action(open_result.window.into(), actions::workspace::SendRequest);
    cx.run_until_parked();

    assert!(open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));
    assert_eq!(
        response_panel.read_with(cx, |response_panel, cx| response_panel.text(cx)),
        "first response"
    );

    let first_headers_scroll_offset = ListOffset {
        item_ix: 7,
        offset_in_item: gpui::px(3.0),
    };
    let first_cookies_scroll_offset = ListOffset {
        item_ix: 12,
        offset_in_item: gpui::px(2.0),
    };
    let second_headers_scroll_offset = ListOffset {
        item_ix: 19,
        offset_in_item: gpui::px(4.0),
    };
    let second_cookies_scroll_offset = ListOffset {
        item_ix: 20,
        offset_in_item: gpui::px(5.0),
    };

    response_panel.update(cx, |response_panel, cx| {
        response_panel
            .headers_list_state(cx)
            .expect("response panel should have response")
            .scroll_to(first_headers_scroll_offset);
        response_panel
            .cookies_list_state(cx)
            .expect("response panel should have response")
            .scroll_to(first_cookies_scroll_offset);
    });

    let headers_scroll_offset = response_panel.read_with(cx, |response_panel, cx| {
        response_panel
            .headers_list_state(cx)
            .expect("response panel should have response")
            .logical_scroll_top()
    });
    assert_eq!(
        headers_scroll_offset.item_ix,
        first_headers_scroll_offset.item_ix,
    );
    assert_eq!(
        headers_scroll_offset.offset_in_item,
        first_headers_scroll_offset.offset_in_item,
    );

    let cookies_scroll_offset = response_panel.read_with(cx, |response_panel, cx| {
        response_panel
            .cookies_list_state(cx)
            .expect("response panel should have response")
            .logical_scroll_top()
    });
    assert_eq!(
        cookies_scroll_offset.item_ix,
        first_cookies_scroll_offset.item_ix,
    );
    assert_eq!(
        cookies_scroll_offset.offset_in_item,
        first_cookies_scroll_offset.offset_in_item,
    );

    open_path(
        &open_result,
        ProjectPath::from((worktree_id, rel_path("collection/second.toml"))),
        cx,
    )
    .await;

    cx.dispatch_action(open_result.window.into(), actions::workspace::SendRequest);
    cx.run_until_parked();

    assert!(open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));
    assert_eq!(
        response_panel.read_with(cx, |response_panel, cx| response_panel.text(cx)),
        "second response"
    );

    response_panel.update(cx, |response_panel, cx| {
        response_panel
            .headers_list_state(cx)
            .expect("response panel should have response")
            .scroll_to(second_headers_scroll_offset);
        response_panel
            .cookies_list_state(cx)
            .expect("response panel should have response")
            .scroll_to(second_cookies_scroll_offset);
    });

    let headers_scroll_offset = response_panel.read_with(cx, |response_panel, cx| {
        response_panel
            .headers_list_state(cx)
            .expect("response panel should have response")
            .logical_scroll_top()
    });
    assert_eq!(
        headers_scroll_offset.item_ix,
        second_headers_scroll_offset.item_ix,
    );
    assert_eq!(
        headers_scroll_offset.offset_in_item,
        second_headers_scroll_offset.offset_in_item,
    );

    let cookies_scroll_offset = response_panel.read_with(cx, |response_panel, cx| {
        response_panel
            .cookies_list_state(cx)
            .expect("response panel should have response")
            .logical_scroll_top()
    });
    assert_eq!(
        cookies_scroll_offset.item_ix,
        second_cookies_scroll_offset.item_ix,
    );
    assert_eq!(
        cookies_scroll_offset.offset_in_item,
        second_cookies_scroll_offset.offset_in_item,
    );

    activate_item_for_path(&open_result, "collection/first.toml", cx);

    assert!(open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));
    assert_eq!(
        response_panel.read_with(cx, |response_panel, cx| response_panel.text(cx)),
        "first response"
    );

    let headers_scroll_offset = response_panel.read_with(cx, |response_panel, cx| {
        response_panel
            .headers_list_state(cx)
            .expect("response panel should have response")
            .logical_scroll_top()
    });
    assert_eq!(
        headers_scroll_offset.item_ix,
        first_headers_scroll_offset.item_ix,
    );
    assert_eq!(
        headers_scroll_offset.offset_in_item,
        first_headers_scroll_offset.offset_in_item,
    );

    let cookies_scroll_offset = response_panel.read_with(cx, |response_panel, cx| {
        response_panel
            .cookies_list_state(cx)
            .expect("response panel should have response")
            .logical_scroll_top()
    });
    assert_eq!(
        cookies_scroll_offset.item_ix,
        first_cookies_scroll_offset.item_ix,
    );
    assert_eq!(
        cookies_scroll_offset.offset_in_item,
        first_cookies_scroll_offset.offset_in_item,
    );

    activate_item_for_path(&open_result, "collection/second.toml", cx);

    assert!(open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));
    assert_eq!(
        response_panel.read_with(cx, |response_panel, cx| response_panel.text(cx)),
        "second response"
    );

    let headers_scroll_offset = response_panel.read_with(cx, |response_panel, cx| {
        response_panel
            .headers_list_state(cx)
            .expect("response panel should have response")
            .logical_scroll_top()
    });
    assert_eq!(
        headers_scroll_offset.item_ix,
        second_headers_scroll_offset.item_ix,
    );
    assert_eq!(
        headers_scroll_offset.offset_in_item,
        second_headers_scroll_offset.offset_in_item,
    );

    let cookies_scroll_offset = response_panel.read_with(cx, |response_panel, cx| {
        response_panel
            .cookies_list_state(cx)
            .expect("response panel should have response")
            .logical_scroll_top()
    });
    assert_eq!(
        cookies_scroll_offset.item_ix,
        second_cookies_scroll_offset.item_ix,
    );
    assert_eq!(
        cookies_scroll_offset.offset_in_item,
        second_cookies_scroll_offset.offset_in_item,
    );
}

#[gpui::test]
async fn test_restored_request_editor_tabs_preserve_response_panel_context(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();

    let app_db = AppDatabase::test_new();
    let temp_fs = TempFs::new(cx.executor());
    let http_client = FakeHttpClient::with_response(StatusCode::NOT_FOUND);
    let app_state =
        cx.update(|cx| AppState::test_new(temp_fs.clone(), Some(http_client.clone()), cx));

    http_client.replace_handler({
        move |_, request| {
            let response = match request.uri().path() {
                "/first" => "first response",
                "/second" => "second response",
                path => panic!("Unexpected request path: {path}"),
            };

            async move {
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .body(AsyncBody::from(response))
                    .unwrap())
            }
        }
    });

    init_test(app_state.clone(), app_db, cx);

    temp_fs.insert_tree(
        "project",
        json!({
            "collection": {
                "first.toml": indoc! {r#"
                    [meta]
                    version = 1

                    [http]
                    method = "GET"
                    url = "https://api.zaku.dev/first"
                "#},
                "second.toml": indoc! {r#"
                    [meta]
                    version = 1

                    [http]
                    method = "GET"
                    url = "https://api.zaku.dev/second"
                "#}
            },
            "settings.json": "{}",
        }),
    );

    let project_path = temp_fs.path().join("project");
    let (open_result, worktree) = open_workspace(project_path.clone(), app_state.clone(), cx).await;

    let worktree_id = worktree.read_with(cx, |worktree, _| worktree.id());

    open_path(
        &open_result,
        ProjectPath::from((worktree_id, rel_path("settings.json"))),
        cx,
    )
    .await;
    open_path(
        &open_result,
        ProjectPath::from((worktree_id, rel_path("collection/first.toml"))),
        cx,
    )
    .await;
    open_path(
        &open_result,
        ProjectPath::from((worktree_id, rel_path("collection/second.toml"))),
        cx,
    )
    .await;
    cx.run_until_parked();
    cx.executor()
        .advance_clock(workspace::SERIALIZATION_THROTTLE_TIME);

    open_result
        .window
        .update(cx, |root, window, cx| {
            root.workspace().update(cx, |workspace, cx| {
                workspace.flush_serialization(window, cx)
            })
        })
        .unwrap()
        .await;
    open_result
        .window
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();

    let (open_result, _) = open_workspace(project_path, app_state.clone(), cx).await;
    cx.run_until_parked();

    let response_panel = open_result
        .workspace
        .read_with(cx, |workspace, cx| workspace.panel::<ResponsePanel>(cx))
        .expect("response panel should be registered");
    let pane = open_result
        .workspace
        .read_with(cx, |workspace, _| workspace.pane().clone());

    assert_eq!(pane.read_with(cx, |pane, _| pane.items_len()), 3);

    activate_item_for_path(&open_result, "settings.json", cx);

    cx.dispatch_action(
        open_result.window.into(),
        actions::response_panel::ToggleFocus,
    );
    cx.run_until_parked();

    assert!(open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));
    assert!(!response_panel.read_with(cx, |response_panel, _| {
        response_panel.has_response_context()
    }));

    activate_item_for_path(&open_result, "collection/second.toml", cx);

    cx.dispatch_action(open_result.window.into(), actions::workspace::SendRequest);
    cx.run_until_parked();

    assert!(open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));
    assert_eq!(
        response_panel.read_with(cx, |response_panel, cx| response_panel.text(cx)),
        "second response"
    );

    activate_item_for_path(&open_result, "settings.json", cx);

    assert!(!open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));
    assert!(!response_panel.read_with(cx, |response_panel, _| {
        response_panel.has_response_context()
    }));

    activate_item_for_path(&open_result, "collection/first.toml", cx);

    assert!(
        response_panel.read_with(cx, |response_panel, cx| {
            response_panel.text(cx).is_empty()
        }),
        "response panel should reflect first tab"
    );

    cx.dispatch_action(open_result.window.into(), actions::workspace::SendRequest);
    cx.run_until_parked();

    assert!(open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));
    assert_eq!(
        response_panel.read_with(cx, |response_panel, cx| response_panel.text(cx)),
        "first response"
    );

    activate_item_for_path(&open_result, "collection/second.toml", cx);

    assert!(open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));
    assert_eq!(
        response_panel.read_with(cx, |response_panel, cx| response_panel.text(cx)),
        "second response"
    );
}

#[gpui::test]
async fn test_response_panel_auto_hidden_without_context(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let app_db = AppDatabase::test_new();
    let temp_fs = TempFs::new(cx.executor());
    let http_client = FakeHttpClient::with_response(StatusCode::NOT_FOUND);
    let app_state =
        cx.update(|cx| AppState::test_new(temp_fs.clone(), Some(http_client.clone()), cx));

    http_client.replace_handler({
        move |_, request| {
            assert_eq!(request.uri().path(), "/valid");

            async move {
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .body(AsyncBody::from("valid response"))
                    .unwrap())
            }
        }
    });

    init_test(app_state.clone(), app_db, cx);

    temp_fs.insert_tree(
        "project",
        json!({
            "collection": {
                "valid.toml": indoc! {r#"
                    [meta]
                    version = 1

                    [http]
                    method = "GET"
                    url = "https://api.zaku.dev/valid"
                "#},
                "invalid.toml": "",
            },
            "settings.json": "{}",
        }),
    );

    let project_path = temp_fs.path().join("project");
    let (open_result, worktree) = open_workspace(project_path, app_state.clone(), cx).await;

    let worktree_id = worktree.read_with(cx, |worktree, _| worktree.id());
    let response_panel = open_result
        .workspace
        .read_with(cx, |workspace, cx| workspace.panel::<ResponsePanel>(cx))
        .expect("response panel should be registered");
    let pane = open_result
        .workspace
        .read_with(cx, |workspace, _| workspace.pane().clone());

    let valid_request_path = ProjectPath::from((worktree_id, rel_path("collection/valid.toml")));
    let invalid_request_path =
        ProjectPath::from((worktree_id, rel_path("collection/invalid.toml")));
    let settings_path = ProjectPath::from((worktree_id, rel_path("settings.json")));

    open_path(&open_result, valid_request_path.clone(), cx).await;
    cx.dispatch_action(open_result.window.into(), actions::workspace::SendRequest);
    cx.run_until_parked();

    assert!(open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));
    assert_eq!(
        response_panel.read_with(cx, |response_panel, cx| response_panel.text(cx)),
        "valid response"
    );

    open_path(&open_result, invalid_request_path, cx).await;
    cx.run_until_parked();

    assert!(!open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));

    open_path(&open_result, valid_request_path.clone(), cx).await;
    cx.run_until_parked();

    assert!(open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));
    assert_eq!(
        response_panel.read_with(cx, |response_panel, cx| response_panel.text(cx)),
        "valid response"
    );

    open_path(&open_result, settings_path.clone(), cx).await;
    cx.run_until_parked();

    assert_eq!(pane.read_with(cx, |pane, _| pane.items_len()), 3);
    assert!(!open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));

    cx.dispatch_action(
        open_result.window.into(),
        actions::response_panel::ToggleFocus,
    );
    cx.run_until_parked();

    assert!(open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));

    open_path(&open_result, valid_request_path, cx).await;
    cx.run_until_parked();

    assert!(open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));
    assert_eq!(
        response_panel.read_with(cx, |response_panel, cx| response_panel.text(cx)),
        "valid response"
    );

    open_path(&open_result, settings_path, cx).await;
    cx.run_until_parked();

    assert!(!open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));
}
