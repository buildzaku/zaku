use futures::{FutureExt, channel::oneshot};
use gpui::{Entity, Focusable, ListOffset, TestAppContext, WindowHandle};
use indoc::{formatdoc, indoc};
use parking_lot::Mutex;
use serde_json::json;
use std::{path::PathBuf, sync::Arc, time::Duration};
use uuid::Uuid;

use db::{AppDatabase, kv::KeyValueStore};
use environment_selector::EnvironmentSelector;
use fs::{Fs, RemoveOptions, TempFs};
use http_client::{AsyncBody, FakeHttpClient, Response, StatusCode};
use path::rel_path;
use picker::PickerDelegate;
use project::ProjectPath;
use recent_projects::RecentProjects;
use response_panel::ResponsePanel;
use session::Session;
use settings::SettingsStore;
use theme::LoadThemes;
use util_macros::path;
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
        recent_projects::init(cx);
        environment_selector::init(cx);
        zaku::init(cx);
    });
}

#[cfg(test)]
async fn open_workspace(
    project_path: PathBuf,
    app_state: Arc<AppState>,
    cx: &mut TestAppContext,
) -> (OpenResult, Entity<Worktree>) {
    let open_result = cx
        .update(|cx| Workspace::open(project_path, app_state, None, OpenMode::NewWindow, cx))
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

    (open_result, worktree)
}

#[cfg(test)]
async fn open_workspaces_in_window(
    project_paths: &[PathBuf],
    app_state: Arc<AppState>,
    cx: &mut TestAppContext,
) -> OpenResult {
    let workspace_db = cx.update(|cx| WorkspaceDb::global(cx));
    let workspace_id = workspace_db.next_id().await.unwrap();
    let window = cx.add_window({
        let app_state = app_state.clone();
        move |window, cx| {
            window.activate_window();
            Root::new(Workspace::create(workspace_id, app_state, window, cx))
        }
    });

    for project_path in project_paths {
        cx.update(|cx| {
            Workspace::open(
                project_path.clone(),
                app_state.clone(),
                Some(window),
                OpenMode::Activate,
                cx,
            )
        })
        .await
        .unwrap();
        window
            .update(cx, |root, window, cx| {
                root.workspace().update(cx, |workspace, cx| {
                    workspace.flush_serialization(window, cx)
                })
            })
            .unwrap()
            .await;
    }

    let workspace = window
        .read_with(cx, |root, _| root.workspace().clone())
        .unwrap();
    OpenResult { window, workspace }
}

#[cfg(test)]
async fn open_path(window: WindowHandle<Root>, path: ProjectPath, cx: &mut TestAppContext) {
    window
        .update(cx, |root, window, cx| {
            root.workspace().update(cx, |workspace, cx| {
                workspace.open_path(path, None, true, window, cx)
            })
        })
        .unwrap()
        .await
        .unwrap();
}

#[cfg(test)]
async fn open_path_preview(
    window: WindowHandle<Root>,
    path: ProjectPath,
    cx: &mut TestAppContext,
) -> Box<dyn ItemHandle> {
    window
        .update(cx, |root, window, cx| {
            root.workspace().update(cx, |workspace, cx| {
                workspace.open_path_preview(path, None, false, true, true, window, cx)
            })
        })
        .unwrap()
        .await
        .unwrap()
}

#[cfg(test)]
fn activate_item_for_path(window: WindowHandle<Root>, path: &str, cx: &mut TestAppContext) {
    let path = rel_path(path);
    window
        .update(cx, |root, window, cx| {
            let pane = root.workspace().read(cx).pane().clone();
            let item_index = pane
                .read(cx)
                .items()
                .position(|item| {
                    item.project_path(cx)
                        .is_some_and(|project_path| project_path.path.as_ref() == path)
                })
                .unwrap();
            pane.update(cx, |pane, cx| {
                pane.activate_item(item_index, true, false, window, cx);
            });
        })
        .unwrap();
}

async fn wait_until(cx: &TestAppContext, condition: impl Fn(&TestAppContext) -> bool) {
    let timeout = cx.background_executor.timer(Duration::from_secs(2)).fuse();
    futures::pin_mut!(timeout);

    while !condition(cx) {
        futures::select_biased! {
            () = cx.background_executor.timer(Duration::from_millis(10)).fuse() => {}
            () = timeout => panic!("timed out waiting for polled condition"),
        }
    }
}

#[gpui::test]
async fn test_open_recent_projects_action(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let app_db = AppDatabase::test_new();
    let temp_fs = TempFs::new(cx.executor());
    let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
    init_test(app_state.clone(), app_db, cx);

    temp_fs.insert_tree("first", json!({}));
    temp_fs.insert_tree("second", json!({}));

    let first_path = temp_fs.path().join("first");
    let second_path = temp_fs.path().join("second");
    let open_result =
        open_workspaces_in_window(&[first_path.clone(), second_path], app_state, cx).await;

    cx.dispatch_action(open_result.window.into(), actions::projects::OpenRecent);

    let picker = open_result.workspace.read_with(cx, |workspace, cx| {
        workspace
            .active_modal::<RecentProjects>(cx)
            .unwrap()
            .read(cx)
            .picker
            .clone()
    });
    // Recent projects exclude the current project.
    cx.condition(&picker, |picker, _| {
        picker.delegate.matched_locations() == [first_path.clone()]
    })
    .await;
    open_result
        .window
        .update(cx, |_, window, cx| {
            assert!(picker.focus_handle(cx).is_focused(window));
        })
        .unwrap();

    cx.dispatch_action(open_result.window.into(), actions::menu::Confirm);

    wait_until(cx, |cx| {
        open_result
            .window
            .read_with(cx, |root, cx| {
                root.workspace().read(cx).project().read(cx).root(cx) == Some(first_path.clone())
            })
            .unwrap()
    })
    .await;
}

#[gpui::test]
async fn test_open_recent_projects_action_in_new_window(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let app_db = AppDatabase::test_new();
    let temp_fs = TempFs::new(cx.executor());
    let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
    init_test(app_state.clone(), app_db, cx);

    temp_fs.insert_tree("first", json!({}));
    temp_fs.insert_tree("second", json!({}));

    let first_path = temp_fs.path().join("first");
    let second_path = temp_fs.path().join("second");
    let open_result =
        open_workspaces_in_window(&[first_path.clone(), second_path.clone()], app_state, cx).await;

    cx.dispatch_action(open_result.window.into(), actions::projects::OpenRecent);

    let picker = open_result.workspace.read_with(cx, |workspace, cx| {
        workspace
            .active_modal::<RecentProjects>(cx)
            .unwrap()
            .read(cx)
            .picker
            .clone()
    });
    // Recent projects exclude the current project.
    cx.condition(&picker, |picker, _| {
        picker.delegate.matched_locations() == [first_path.clone()]
    })
    .await;

    cx.dispatch_action(open_result.window.into(), actions::menu::SecondaryConfirm);

    wait_until(cx, |cx| cx.windows().len() == 2).await;
    let new_window = cx.windows()[1].downcast::<Root>().unwrap();
    new_window
        .read_with(cx, |root, cx| {
            assert_eq!(
                root.workspace().read(cx).project().read(cx).root(cx),
                Some(first_path),
            );
        })
        .unwrap();
    open_result
        .window
        .read_with(cx, |root, cx| {
            let workspace = root.workspace().read(cx);
            assert_eq!(workspace.project().read(cx).root(cx), Some(second_path));
            assert!(workspace.active_modal::<RecentProjects>(cx).is_none());
        })
        .unwrap();
}

#[gpui::test]
async fn test_environment_selector(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let app_db = AppDatabase::test_new();
    let temp_fs = TempFs::new(cx.executor());
    let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));
    init_test(app_state.clone(), app_db, cx);

    temp_fs.insert_tree(
        "project",
        json!({
            ".zaku": {
                "environments": {
                    "dev.toml": indoc! {r#"
                        [meta]
                        version = 1

                        [environment]
                        color = "green"
                        variables = [{ name = "base_url", value = "http://localhost:8000" }]
                    "#},
                    "staging-9.toml": indoc! {r#"
                        [meta]
                        version = 1

                        [environment]
                        color = "blue"
                        variables = [{ name = "base_url", value = "https://staging-9.api.zaku.dev" }]
                    "#},
                    "staging-10.toml": indoc! {r#"
                        [meta]
                        version = 1

                        [environment]
                        color = "purple"
                        variables = [{ name = "base_url", value = "https://staging-10.api.zaku.dev" }]
                    "#},
                },
            },
        }),
    );

    let project_path = temp_fs.path().join("project");
    let (open_result, worktree) = open_workspace(project_path.clone(), app_state.clone(), cx).await;
    cx.run_until_parked();
    open_result.workspace.update(cx, |workspace, cx| {
        workspace
            .project()
            .read(cx)
            .project_config_store()
            .clone()
            .update(cx, |project_config_store, cx| {
                project_config_store.activate_environment(Some("dev".to_string()), cx);
            });
    });

    cx.dispatch_action(
        open_result.window.into(),
        actions::environment_selector::Toggle,
    );

    let picker = open_result.workspace.read_with(cx, |workspace, cx| {
        workspace
            .active_modal::<EnvironmentSelector>(cx)
            .unwrap()
            .read(cx)
            .picker
            .clone()
    });
    picker.read_with(cx, |picker, _| {
        assert_eq!(
            picker.delegate.matched_environments(),
            vec![
                (None, false),
                (Some("dev".to_string()), false),
                (Some("staging-9".to_string()), false),
                (Some("staging-10".to_string()), false),
            ]
        );
        assert_eq!(picker.delegate.selected_index(), 1);
    });

    cx.dispatch_action(
        open_result.window.into(),
        actions::environment_selector::Toggle,
    );
    temp_fs
        .remove_file(
            &project_path.join(path!(".zaku/environments/dev.toml")),
            RemoveOptions::default(),
        )
        .await
        .unwrap();
    worktree.flush_fs_events(cx).await;
    cx.run_until_parked();

    cx.dispatch_action(
        open_result.window.into(),
        actions::environment_selector::Toggle,
    );

    let picker = open_result.workspace.read_with(cx, |workspace, cx| {
        workspace
            .active_modal::<EnvironmentSelector>(cx)
            .unwrap()
            .read(cx)
            .picker
            .clone()
    });
    picker.read_with(cx, |picker, _| {
        assert_eq!(
            picker.delegate.matched_environments(),
            vec![
                (Some("dev".to_string()), true),
                (None, false),
                (Some("staging-9".to_string()), false),
                (Some("staging-10".to_string()), false),
            ]
        );
        assert_eq!(picker.delegate.selected_index(), 0);
    });

    cx.simulate_input(open_result.window.into(), "staging-10");
    cx.dispatch_action(open_result.window.into(), actions::menu::Confirm);
    cx.run_until_parked();

    open_result.workspace.read_with(cx, |workspace, cx| {
        assert_eq!(
            workspace
                .project()
                .read(cx)
                .project_config_store()
                .read(cx)
                .active_environment(),
            Some("staging-10")
        );
        assert!(workspace.active_modal::<EnvironmentSelector>(cx).is_none());
    });

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

    let (reopened_result, _) = open_workspace(project_path, app_state, cx).await;
    reopened_result.workspace.read_with(cx, |workspace, cx| {
        assert_eq!(
            workspace
                .project()
                .read(cx)
                .project_config_store()
                .read(cx)
                .active_environment(),
            Some("staging-10")
        );
    });
}

#[gpui::test]
async fn test_reload_restores_project_windows_and_tabs(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let app_db = AppDatabase::test_new();
    let kv_store = KeyValueStore::open(&app_db);
    let session = Session::new(Uuid::new_v4().to_string(), kv_store.clone()).await;
    let temp_fs = TempFs::new(cx.executor());
    let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));

    cx.update(|cx| {
        app_state
            .session
            .update(cx, |app_session, _| app_session.replace_session(session));
    });
    init_test(app_state.clone(), app_db, cx);

    let mut windows = Vec::new();
    for (project, request, preview_request, settings_file) in [
        ("first", "request1", "preview1", "settings.jsonc"),
        ("second", "request2", "preview2", "keymap.jsonc"),
    ] {
        temp_fs.insert_tree(
            project,
            json!({
                "folder": {
                    format!("{request}.toml"): formatdoc! {r#"
                        [meta]
                        version = 1

                        [http]
                        method = "GET"
                        url = "https://api.zaku.dev/{request}"
                    "#},
                    format!("{preview_request}.toml"): formatdoc! {r#"
                        [meta]
                        version = 1

                        [http]
                        method = "GET"
                        url = "https://api.zaku.dev/{preview_request}"
                    "#}
                },
                (settings_file): "{}",
            }),
        );

        let project_path = temp_fs.path().join(project);
        let (open_result, worktree) = open_workspace(project_path, app_state.clone(), cx).await;
        let worktree_id = worktree.read_with(cx, |worktree, _| worktree.id());
        let request_path = format!("folder/{request}.toml");
        let preview_path = format!("folder/{preview_request}.toml");

        open_path(
            open_result.window,
            ProjectPath::from((worktree_id, rel_path(&request_path))),
            cx,
        )
        .await;
        open_path(
            open_result.window,
            ProjectPath::from((worktree_id, rel_path(settings_file))),
            cx,
        )
        .await;
        open_path_preview(
            open_result.window,
            ProjectPath::from((worktree_id, rel_path(&preview_path))),
            cx,
        )
        .await;

        activate_item_for_path(open_result.window, settings_file, cx);
        windows.push(open_result.window);
    }

    let restart = cx.expect_restart();
    cx.update(workspace::reload);
    let (restart_path, restart_arguments) = restart.await.expect("restart was not requested");
    assert_eq!(restart_path, None);
    assert!(restart_arguments.is_empty());

    let session_id = cx.read(|cx| app_state.session.read(cx).id().to_owned());
    let workspace_db = cx.update(|cx| WorkspaceDb::global(cx));
    let locations = workspace::last_session_workspace_locations(
        &workspace_db,
        &session_id,
        None,
        temp_fs.as_ref(),
    )
    .await
    .unwrap();

    assert_eq!(locations.len(), 2);

    for window in windows {
        window
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
    }
    cx.run_until_parked();

    let restored_session = Session::new(Uuid::new_v4().to_string(), kv_store).await;

    cx.update(|cx| {
        app_state.session.update(cx, |app_session, _| {
            app_session.replace_session(restored_session);
        });
    });

    let mut async_cx = cx.to_async();
    zaku::restore_or_create_workspace(app_state, &mut async_cx)
        .await
        .unwrap();

    let mut restored_tabs = cx.read(|cx| {
        cx.windows()
            .into_iter()
            .filter_map(|window| window.downcast::<Root>())
            .map(|window| {
                window
                    .read_with(cx, |root, cx| {
                        let workspace = root.workspace().read(cx);
                        let root_path = workspace
                            .project()
                            .read(cx)
                            .root_worktree(cx)
                            .unwrap()
                            .read(cx)
                            .abs_path()
                            .to_path_buf();
                        let pane = workspace.pane().read(cx);
                        let tab_paths = pane
                            .items()
                            .map(|item| item.project_path(cx).unwrap().path)
                            .collect::<Vec<_>>();
                        let preview_path =
                            pane.preview_item().unwrap().project_path(cx).unwrap().path;

                        assert_eq!(pane.active_item_index(), 1);

                        (root_path, tab_paths, preview_path)
                    })
                    .unwrap()
            })
            .collect::<Vec<_>>()
    });
    restored_tabs.sort_by(|left, right| left.0.cmp(&right.0));

    assert_eq!(
        restored_tabs,
        vec![
            (
                temp_fs.path().join("first"),
                vec![
                    rel_path("folder/request1.toml").into(),
                    rel_path("settings.jsonc").into(),
                    rel_path("folder/preview1.toml").into(),
                ],
                rel_path("folder/preview1.toml").into(),
            ),
            (
                temp_fs.path().join("second"),
                vec![
                    rel_path("folder/request2.toml").into(),
                    rel_path("keymap.jsonc").into(),
                    rel_path("folder/preview2.toml").into(),
                ],
                rel_path("folder/preview2.toml").into(),
            ),
        ]
    );
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
                "folder": {
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
    let (tx, rx) = oneshot::channel();
    let rx = Arc::new(Mutex::new(Some(rx)));

    let http_client = FakeHttpClient::create(move |request| {
        assert_eq!(request.uri().path(), "/me");
        let rx = rx.lock().take().unwrap();

        async move { Ok(rx.await.unwrap()) }
    });
    let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), Some(http_client), cx));

    init_test(app_state.clone(), app_db, cx);

    temp_fs.insert_tree(
        "project",
        json!({
            "folder": {
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
        open_result.window,
        ProjectPath::from((worktree_id, rel_path("folder/request.toml"))),
        cx,
    )
    .await;

    cx.dispatch_action(open_result.window.into(), actions::workspace::SendRequest);
    cx.run_until_parked();

    let response_panel = open_result
        .workspace
        .read_with(cx, |workspace, cx| workspace.panel::<ResponsePanel>(cx))
        .unwrap();
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
        tx.send(response).is_ok(),
        "response receiver should be active"
    );
}

#[gpui::test]
async fn test_each_request_editor_has_its_own_response(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let app_db = AppDatabase::test_new();
    let temp_fs = TempFs::new(cx.executor());
    let (first_tx, first_rx) = oneshot::channel();
    let (second_tx, second_rx) = oneshot::channel();
    let first_rx = Arc::new(Mutex::new(Some(first_rx)));
    let second_rx = Arc::new(Mutex::new(Some(second_rx)));
    let first_response_delay = Duration::from_secs(5);
    let second_response_delay = Duration::from_secs(3);
    let executor = cx.executor();

    let http_client = FakeHttpClient::create(move |request| {
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
    });
    let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), Some(http_client), cx));

    init_test(app_state.clone(), app_db, cx);

    temp_fs.insert_tree(
        "project",
        json!({
            "folder": {
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
        .unwrap();

    open_path(
        open_result.window,
        ProjectPath::from((worktree_id, rel_path("folder/first.toml"))),
        cx,
    )
    .await;
    cx.dispatch_action(open_result.window.into(), actions::workspace::SendRequest);
    cx.run_until_parked();

    open_path(
        open_result.window,
        ProjectPath::from((worktree_id, rel_path("folder/second.toml"))),
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
        first_tx.send(response).is_ok(),
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
        second_tx.send(response).is_ok(),
        "response receiver should be active"
    );

    cx.executor().advance_clock(second_response_delay);
    cx.run_until_parked();

    assert_eq!(
        response_panel.read_with(cx, |response_panel, cx| response_panel.text(cx)),
        "second response"
    );

    activate_item_for_path(open_result.window, "folder/first.toml", cx);

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
    let (first_tx, first_rx) = oneshot::channel();
    let (second_tx, second_rx) = oneshot::channel();
    let first_rx = Arc::new(Mutex::new(Some(first_rx)));
    let second_rx = Arc::new(Mutex::new(Some(second_rx)));

    let http_client = FakeHttpClient::create(move |request| {
        let rx = match request.uri().path() {
            "/first" => first_rx.lock().take().unwrap(),
            "/second" => second_rx.lock().take().unwrap(),
            path => panic!("Unexpected request path: {path}"),
        };
        async move { Ok(rx.await.unwrap()) }
    });
    let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), Some(http_client), cx));

    init_test(app_state.clone(), app_db, cx);

    temp_fs.insert_tree(
        "project",
        json!({
            "folder": {
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
        .unwrap();
    let pane = open_result
        .workspace
        .read_with(cx, |workspace, _| workspace.pane().clone());

    let first_item = open_path_preview(
        open_result.window,
        ProjectPath::from((worktree_id, rel_path("folder/first.toml"))),
        cx,
    )
    .await;
    cx.dispatch_action(open_result.window.into(), actions::workspace::SendRequest);
    cx.run_until_parked();
    assert!(pane.read_with(cx, |pane, _| pane.preview_item_idx().is_none()));

    open_path_preview(
        open_result.window,
        ProjectPath::from((worktree_id, rel_path("folder/second.toml"))),
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
        first_tx.send(response).is_ok(),
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
        second_tx.send(response).is_ok(),
        "response receiver should be active"
    );

    cx.run_until_parked();

    assert_eq!(
        response_panel.read_with(cx, |response_panel, cx| response_panel.text(cx)),
        "second response"
    );

    activate_item_for_path(open_result.window, "folder/first.toml", cx);

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

    let http_client = FakeHttpClient::create(move |request| {
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
    });
    let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), Some(http_client), cx));

    init_test(app_state.clone(), app_db, cx);

    temp_fs.insert_tree(
        "project",
        json!({
            "folder": {
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
        .unwrap();

    open_path(
        open_result.window,
        ProjectPath::from((worktree_id, rel_path("folder/first.toml"))),
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
            .unwrap()
            .scroll_to(first_headers_scroll_offset);
        response_panel
            .cookies_list_state(cx)
            .unwrap()
            .scroll_to(first_cookies_scroll_offset);
    });

    let headers_scroll_offset = response_panel.read_with(cx, |response_panel, cx| {
        response_panel
            .headers_list_state(cx)
            .unwrap()
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
            .unwrap()
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
        open_result.window,
        ProjectPath::from((worktree_id, rel_path("folder/second.toml"))),
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
            .unwrap()
            .scroll_to(second_headers_scroll_offset);
        response_panel
            .cookies_list_state(cx)
            .unwrap()
            .scroll_to(second_cookies_scroll_offset);
    });

    let headers_scroll_offset = response_panel.read_with(cx, |response_panel, cx| {
        response_panel
            .headers_list_state(cx)
            .unwrap()
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
            .unwrap()
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

    activate_item_for_path(open_result.window, "folder/first.toml", cx);

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
            .unwrap()
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
            .unwrap()
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

    activate_item_for_path(open_result.window, "folder/second.toml", cx);

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
            .unwrap()
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
            .unwrap()
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

    let http_client = FakeHttpClient::create(move |request| {
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
    });
    let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), Some(http_client), cx));

    init_test(app_state.clone(), app_db, cx);

    temp_fs.insert_tree(
        "project",
        json!({
            "folder": {
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
            "settings.jsonc": "{}",
        }),
    );

    let project_path = temp_fs.path().join("project");
    let (open_result, worktree) = open_workspace(project_path.clone(), app_state.clone(), cx).await;
    let worktree_id = worktree.read_with(cx, |worktree, _| worktree.id());

    open_path(
        open_result.window,
        ProjectPath::from((worktree_id, rel_path("settings.jsonc"))),
        cx,
    )
    .await;
    open_path(
        open_result.window,
        ProjectPath::from((worktree_id, rel_path("folder/first.toml"))),
        cx,
    )
    .await;
    open_path(
        open_result.window,
        ProjectPath::from((worktree_id, rel_path("folder/second.toml"))),
        cx,
    )
    .await;

    let response_panel = open_result
        .workspace
        .read_with(cx, |workspace, cx| workspace.panel::<ResponsePanel>(cx))
        .unwrap();

    assert!(open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));
    assert!(response_panel.read_with(cx, |response_panel, _| {
        response_panel.has_response_context()
    }));

    activate_item_for_path(open_result.window, "settings.jsonc", cx);

    assert!(!open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));

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

    let (open_result, worktree) = open_workspace(project_path, app_state.clone(), cx).await;
    let worktree_id = worktree.read_with(cx, |worktree, _| worktree.id());
    let response_panel = open_result
        .workspace
        .read_with(cx, |workspace, cx| workspace.panel::<ResponsePanel>(cx))
        .unwrap();
    let pane = open_result
        .workspace
        .read_with(cx, |workspace, _| workspace.pane().clone());

    assert_eq!(pane.read_with(cx, |pane, _| pane.items_len()), 3);
    assert_eq!(
        pane.read_with(cx, |pane, cx| {
            pane.active_item().and_then(|item| item.project_path(cx))
        }),
        Some(ProjectPath::from((worktree_id, rel_path("settings.jsonc"))))
    );
    assert!(!open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));

    activate_item_for_path(open_result.window, "folder/second.toml", cx);

    assert!(open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));
    assert!(response_panel.read_with(cx, |response_panel, _| {
        response_panel.has_response_context()
    }));
    assert!(
        response_panel.read_with(cx, |response_panel, cx| {
            response_panel.text(cx).is_empty()
        }),
        "response panel should reflect restored request tab"
    );

    activate_item_for_path(open_result.window, "settings.jsonc", cx);

    cx.dispatch_action(
        open_result.window.into(),
        actions::response_panel::ToggleFocus,
    );

    assert!(open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));
    assert!(!response_panel.read_with(cx, |response_panel, _| {
        response_panel.has_response_context()
    }));

    activate_item_for_path(open_result.window, "folder/second.toml", cx);

    cx.dispatch_action(open_result.window.into(), actions::workspace::SendRequest);
    cx.run_until_parked();

    assert!(open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));
    assert_eq!(
        response_panel.read_with(cx, |response_panel, cx| response_panel.text(cx)),
        "second response"
    );

    activate_item_for_path(open_result.window, "settings.jsonc", cx);

    assert!(!open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));
    assert!(!response_panel.read_with(cx, |response_panel, _| {
        response_panel.has_response_context()
    }));

    activate_item_for_path(open_result.window, "folder/first.toml", cx);

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

    activate_item_for_path(open_result.window, "folder/second.toml", cx);

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

    let http_client = FakeHttpClient::create(move |request| {
        assert_eq!(request.uri().path(), "/valid");

        async move {
            Ok(Response::builder()
                .status(StatusCode::OK)
                .body(AsyncBody::from("valid response"))
                .unwrap())
        }
    });
    let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), Some(http_client), cx));

    init_test(app_state.clone(), app_db, cx);

    temp_fs.insert_tree(
        "project",
        json!({
            "folder": {
                "valid.toml": indoc! {r#"
                    [meta]
                    version = 1

                    [http]
                    method = "GET"
                    url = "https://api.zaku.dev/valid"
                "#},
                "invalid.toml": "",
            },
            "settings.jsonc": "{}",
        }),
    );

    let project_path = temp_fs.path().join("project");
    let (open_result, worktree) = open_workspace(project_path, app_state.clone(), cx).await;
    let worktree_id = worktree.read_with(cx, |worktree, _| worktree.id());
    let response_panel = open_result
        .workspace
        .read_with(cx, |workspace, cx| workspace.panel::<ResponsePanel>(cx))
        .unwrap();
    let pane = open_result
        .workspace
        .read_with(cx, |workspace, _| workspace.pane().clone());

    let valid_request_path = ProjectPath::from((worktree_id, rel_path("folder/valid.toml")));
    let invalid_request_path = ProjectPath::from((worktree_id, rel_path("folder/invalid.toml")));
    let settings_path = ProjectPath::from((worktree_id, rel_path("settings.jsonc")));

    open_path(open_result.window, valid_request_path.clone(), cx).await;

    assert!(open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));
    assert!(response_panel.read_with(cx, |response_panel, _| {
        response_panel.has_response_context()
    }));

    cx.dispatch_action(open_result.window.into(), actions::workspace::SendRequest);
    cx.run_until_parked();

    assert!(open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));
    assert_eq!(
        response_panel.read_with(cx, |response_panel, cx| response_panel.text(cx)),
        "valid response"
    );

    open_path(open_result.window, invalid_request_path, cx).await;

    assert!(!open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));

    open_path(open_result.window, valid_request_path.clone(), cx).await;

    assert!(open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));
    assert_eq!(
        response_panel.read_with(cx, |response_panel, cx| response_panel.text(cx)),
        "valid response"
    );

    open_path(open_result.window, settings_path.clone(), cx).await;

    assert_eq!(pane.read_with(cx, |pane, _| pane.items_len()), 3);
    assert!(!open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));

    cx.dispatch_action(
        open_result.window.into(),
        actions::response_panel::ToggleFocus,
    );

    assert!(open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));

    open_path(open_result.window, valid_request_path, cx).await;

    assert!(open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));
    assert_eq!(
        response_panel.read_with(cx, |response_panel, cx| response_panel.text(cx)),
        "valid response"
    );

    open_path(open_result.window, settings_path, cx).await;

    assert!(!open_result.workspace.read_with(cx, |workspace, cx| {
        workspace.is_panel_open::<ResponsePanel>(cx)
    }));
}

#[gpui::test]
async fn test_trash_delete_with_active_pane_item(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let app_db = AppDatabase::test_new();
    let temp_fs = TempFs::new(cx.executor());
    let app_state = cx.update(|cx| AppState::test_new(temp_fs.clone(), None, cx));

    init_test(app_state.clone(), app_db, cx);

    temp_fs.insert_tree(
        "project",
        json!({
            "folder": {
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
                "#},
            },
            "settings.jsonc": "{}",
        }),
    );

    let project_path = temp_fs.path().join("project");
    let (open_result, worktree) = open_workspace(project_path, app_state, cx).await;
    let worktree_id = worktree.read_with(cx, |worktree, _| worktree.id());
    let first_request_path = ProjectPath::from((worktree_id, rel_path("folder/first.toml")));
    let second_request_path = ProjectPath::from((worktree_id, rel_path("folder/second.toml")));
    let settings_path = ProjectPath::from((worktree_id, rel_path("settings.jsonc")));

    open_path(open_result.window, first_request_path.clone(), cx).await;
    open_path(open_result.window, second_request_path.clone(), cx).await;
    open_path(open_result.window, settings_path.clone(), cx).await;
    activate_item_for_path(open_result.window, "folder/first.toml", cx);

    let pane = open_result
        .workspace
        .read_with(cx, |workspace, _| workspace.pane().clone());
    assert_eq!(pane.read_with(cx, |pane, _| pane.items_len()), 3);
    assert_eq!(
        pane.read_with(cx, |pane, cx| {
            pane.active_item().and_then(|item| item.project_path(cx))
        }),
        Some(first_request_path.clone())
    );

    let project = open_result
        .workspace
        .read_with(cx, |workspace, _| workspace.project().clone());
    let entry_id = project.read_with(cx, |project, cx| {
        project.entry_for_path(&first_request_path, cx).unwrap().id
    });
    project
        .update(cx, |project, cx| {
            project.delete_entry(entry_id, false, cx).unwrap()
        })
        .await
        .unwrap();

    assert_eq!(pane.read_with(cx, |pane, _| pane.items_len()), 2);
    assert_eq!(
        pane.read_with(cx, |pane, cx| {
            pane.active_item().and_then(|item| item.project_path(cx))
        }),
        Some(second_request_path.clone())
    );
    assert!(
        temp_fs
            .metadata("project/folder/first.toml".as_ref())
            .await
            .unwrap()
            .is_none()
    );

    let entry_id = project.read_with(cx, |project, cx| {
        project.entry_for_path(&second_request_path, cx).unwrap().id
    });
    project
        .update(cx, |project, cx| {
            project.delete_entry(entry_id, true, cx).unwrap()
        })
        .await
        .unwrap();

    assert_eq!(pane.read_with(cx, |pane, _| pane.items_len()), 1);
    assert_eq!(
        pane.read_with(cx, |pane, cx| {
            pane.active_item().and_then(|item| item.project_path(cx))
        }),
        Some(settings_path)
    );
    assert!(
        temp_fs
            .metadata("project/folder/second.toml".as_ref())
            .await
            .unwrap()
            .is_none()
    );
}
