use gpui::{ListOffset, TestAppContext};
use indoc::indoc;
use std::sync::Arc;
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
use workspace::{AppState, OpenMode, OpenResult, Root, Workspace, WorkspaceDb};
use worktree::WorktreeModelHandle;

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
            .update(|cx| Workspace::open(path, app_state.clone(), None, OpenMode::NewWindow, cx))
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
async fn test_switching_request_tab_preserves_response_panel_scroll(cx: &mut TestAppContext) {
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
                            Path=/; Domain=example.com; Secure; HttpOnly; SameSite=Lax"
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
        serde_json::json!({
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
    let open_result = cx
        .update(|cx| {
            Workspace::open(
                project_path,
                app_state.clone(),
                None,
                OpenMode::NewWindow,
                cx,
            )
        })
        .await
        .unwrap();
    open_result
        .workspace
        .read_with(cx, |workspace, cx| workspace.worktree_scan_complete(cx))
        .await;
    let worktree = open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.project().read(cx).root_worktree(cx).unwrap()
    });
    worktree.flush_fs_events(cx).await;

    let worktree_id = worktree.read_with(cx, |worktree, _| worktree.id());
    let response_panel = open_result
        .workspace
        .read_with(cx, |workspace, cx| workspace.panel::<ResponsePanel>(cx))
        .expect("response panel should be registered");
    let pane = open_result
        .workspace
        .read_with(cx, |workspace, _| workspace.pane().clone());

    let first_path = ProjectPath {
        worktree_id,
        path: Arc::from(rel_path("collection/first.toml")),
    };
    let second_path = ProjectPath {
        worktree_id,
        path: Arc::from(rel_path("collection/second.toml")),
    };
    let first_item = open_result
        .window
        .update(cx, |root, window, cx| {
            root.workspace().update(cx, |workspace, cx| {
                workspace.open_path(first_path, None, true, window, cx)
            })
        })
        .unwrap()
        .await
        .unwrap();
    let first_item_id = first_item.item_id();

    cx.dispatch_action(open_result.window.into(), actions::workspace::SendRequest);
    cx.run_until_parked();
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

    let second_item = open_result
        .window
        .update(cx, |root, window, cx| {
            root.workspace().update(cx, |workspace, cx| {
                workspace.open_path(second_path, None, true, window, cx)
            })
        })
        .unwrap()
        .await
        .unwrap();
    let second_item_id = second_item.item_id();
    cx.dispatch_action(open_result.window.into(), actions::workspace::SendRequest);
    cx.run_until_parked();

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

    let first_item_index = pane.read_with(cx, |pane, _| {
        pane.items()
            .position(|item| item.item_id() == first_item_id)
            .unwrap()
    });

    open_result
        .window
        .update(cx, |_, window, cx| {
            pane.update(cx, |pane, cx| {
                pane.activate_item(first_item_index, true, false, window, cx);
            });
        })
        .unwrap();
    cx.run_until_parked();

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

    let second_item_index = pane.read_with(cx, |pane, _| {
        pane.items()
            .position(|item| item.item_id() == second_item_id)
            .unwrap()
    });

    open_result
        .window
        .update(cx, |_, window, cx| {
            pane.update(cx, |pane, cx| {
                pane.activate_item(second_item_index, true, false, window, cx);
            });
        })
        .unwrap();
    cx.run_until_parked();

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
