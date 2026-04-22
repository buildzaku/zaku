use futures::{FutureExt, select_biased};
use gpui::{Action, AnyWindowHandle, NoAction, TestAppContext, Unbind};
use std::{collections::BTreeSet, path::PathBuf, sync::Arc, time::Duration};

use actions::workspace::ToggleLeftDock;
use fs::{Fs, TempFs};
use settings::watch_config_file;
use theme::LoadThemes;
use workspace::{Root, SharedState, WORKSPACE_DB, Workspace};
use zaku::{handle_keymap_file_changes, handle_settings_file_changes};

gpui::actions!(test_only, [ActionA, ActionB]);

fn init_test(shared_state: Arc<SharedState>, cx: &mut TestAppContext) {
    cx.update(|cx| {
        settings::init(cx);
        settings::log_settings::init(cx);
        theme::init(LoadThemes::JustBase, cx);
        editor::init(cx);
        workspace::init(shared_state, cx);
        workspace::panel::project::init(cx);
        workspace::panel::response::init(cx);
    });
}

fn action_namespace(action_name: &str) -> &str {
    action_name
        .rsplit_once("::")
        .map(|(namespace, _)| namespace)
        .unwrap_or("")
}

#[track_caller]
fn assert_key_bindings_for(
    window: AnyWindowHandle,
    cx: &TestAppContext,
    actions: Vec<(&'static str, &dyn Action)>,
    line: u32,
) {
    let available_actions = cx
        .update(|cx| window.update(cx, |_, window, cx| window.available_actions(cx)))
        .unwrap();

    for (key, action) in actions {
        let bindings = cx
            .update(|cx| window.update(cx, |_, window, _| window.bindings_for_action(action)))
            .unwrap();

        assert!(
            available_actions
                .iter()
                .any(|bound_action| bound_action.partial_eq(action)),
            "On {} Failed to find {}",
            line,
            action.name(),
        );
        assert!(
            bindings.into_iter().any(|binding| binding
                .keystrokes()
                .iter()
                .any(|keystroke| keystroke.key() == key)),
            "On {} Failed to find {} with key binding {}",
            line,
            action.name(),
            key,
        );
    }
}

fn has_key_binding(
    window: AnyWindowHandle,
    cx: &TestAppContext,
    key: &str,
    action: &dyn Action,
) -> bool {
    let bindings = cx
        .update(|cx| window.update(cx, |_, window, _| window.bindings_for_action(action)))
        .unwrap();

    bindings.iter().any(|binding| {
        binding
            .keystrokes()
            .iter()
            .any(|keystroke| keystroke.key() == key)
    })
}

async fn wait_until(cx: &TestAppContext, condition: impl Fn(&TestAppContext) -> bool) {
    let timeout = cx.background_executor.timer(Duration::from_secs(2)).fuse();
    futures::pin_mut!(timeout);

    while !condition(cx) {
        select_biased! {
            _ = cx.background_executor.timer(Duration::from_millis(10)).fuse() => {}
            _ = timeout => panic!("timed out waiting for polled condition"),
        }
    }
}

#[gpui::test]
async fn test_basic_keymap(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let temp_fs = Arc::new(TempFs::new(cx.executor().clone()));
    let shared_state = cx.update(|cx| Arc::new(SharedState::test_new(temp_fs.clone(), cx)));
    init_test(shared_state.clone(), cx);

    let workspace_id = WORKSPACE_DB.next_id().await.unwrap();
    let window = cx.add_window(move |window, cx| {
        Root::new(Workspace::create(workspace_id, shared_state, window, cx))
    });
    let workspace = window
        .read_with(cx, |root, _| root.workspace().clone())
        .unwrap();

    let settings_path = PathBuf::from("settings.json");
    let keymap_path = PathBuf::from("keymap.json");

    temp_fs.write(&settings_path, br#"{}"#).await.unwrap();
    temp_fs
        .write(
            &keymap_path,
            br#"[{"bindings":{"backspace":"test_only::ActionA"}}]"#,
        )
        .await
        .unwrap();
    cx.executor().run_until_parked();

    let (settings_rx, settings_watcher) = watch_config_file(
        &cx.background_executor,
        temp_fs.clone(),
        settings_path.clone(),
    );
    let (keymap_rx, keymap_watcher) = watch_config_file(
        &cx.background_executor,
        temp_fs.clone(),
        keymap_path.clone(),
    );
    cx.update(|cx| {
        handle_settings_file_changes(settings_rx, settings_watcher, cx);
        handle_keymap_file_changes(keymap_rx, keymap_watcher, cx);
    });
    window
        .update(cx, |_, _, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.register_action(|_, _: &ActionA, _, _| {});
                workspace.register_action(|_, _: &ActionB, _, _| {});
                cx.notify();
            });
        })
        .unwrap();
    cx.executor().run_until_parked();

    assert_key_bindings_for(
        window.into(),
        cx,
        vec![("backspace", &ActionA), ("b", &ToggleLeftDock)],
        line!(),
    );

    temp_fs
        .write(
            &keymap_path,
            br#"[{"bindings":{"backspace":"test_only::ActionB"}}]"#,
        )
        .await
        .unwrap();
    wait_until(cx, |cx| {
        has_key_binding(window.into(), cx, "backspace", &ActionB)
    })
    .await;

    assert_key_bindings_for(
        window.into(),
        cx,
        vec![("backspace", &ActionB), ("b", &ToggleLeftDock)],
        line!(),
    );
}

#[gpui::test]
async fn test_disabled_keymap_binding(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let temp_fs = Arc::new(TempFs::new(cx.executor().clone()));
    let shared_state = cx.update(|cx| Arc::new(SharedState::test_new(temp_fs.clone(), cx)));
    init_test(shared_state.clone(), cx);

    let workspace_id = WORKSPACE_DB.next_id().await.unwrap();
    let window = cx.add_window(move |window, cx| {
        Root::new(Workspace::create(workspace_id, shared_state, window, cx))
    });
    let workspace = window
        .read_with(cx, |root, _| root.workspace().clone())
        .unwrap();

    window
        .update(cx, |_, _, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.register_action(|_, _: &ActionA, _, _| {});
                workspace.register_action(|_, _: &ActionB, _, _| {});
                cx.notify();
            });
        })
        .unwrap();

    let settings_path = PathBuf::from("settings.json");
    let keymap_path = PathBuf::from("keymap.json");
    temp_fs.write(&settings_path, br#"{}"#).await.unwrap();
    temp_fs
        .write(
            &keymap_path,
            br#"[{"bindings":{"backspace":"test_only::ActionA"}}]"#,
        )
        .await
        .unwrap();

    let (settings_rx, settings_watcher) = watch_config_file(
        &cx.background_executor,
        temp_fs.clone(),
        settings_path.clone(),
    );
    let (keymap_rx, keymap_watcher) = watch_config_file(
        &cx.background_executor,
        temp_fs.clone(),
        keymap_path.clone(),
    );
    cx.update(|cx| {
        handle_settings_file_changes(settings_rx, settings_watcher, cx);
        handle_keymap_file_changes(keymap_rx, keymap_watcher, cx);
    });
    cx.executor().run_until_parked();

    assert_key_bindings_for(
        window.into(),
        cx,
        vec![("backspace", &ActionA), ("b", &ToggleLeftDock)],
        line!(),
    );

    temp_fs
        .write(&keymap_path, br#"[{"bindings":{"backspace":null}}]"#)
        .await
        .unwrap();
    wait_until(cx, |cx| {
        !has_key_binding(window.into(), cx, "backspace", &ActionA)
    })
    .await;

    assert_key_bindings_for(window.into(), cx, vec![("b", &ToggleLeftDock)], line!());
}

#[gpui::test]
async fn test_action_namespaces(cx: &mut TestAppContext) {
    let temp_fs = Arc::new(TempFs::new(cx.executor()));
    let shared_state = cx.update(|cx| Arc::new(SharedState::test_new(temp_fs.clone(), cx)));
    init_test(shared_state, cx);

    cx.update(|cx| {
        let all_actions = cx.all_action_names();
        let mut actions_without_namespace = Vec::new();
        let mut all_namespaces = BTreeSet::new();
        let ignored_namespaces = BTreeSet::from([
            action_namespace(NoAction.name()),
            action_namespace(Unbind::name_for_type()),
            "test_only",
        ]);

        for action_name in all_actions.iter() {
            let namespace = action_namespace(action_name);
            if namespace.is_empty() {
                actions_without_namespace.push(*action_name);
                continue;
            }

            if !ignored_namespaces.contains(namespace) {
                all_namespaces.insert(namespace.to_string());
            }
        }

        assert_eq!(actions_without_namespace, Vec::<&str>::new());
        assert_eq!(
            all_namespaces,
            BTreeSet::from([
                "action".to_string(),
                "editor".to_string(),
                "menu".to_string(),
                "project_panel".to_string(),
                "response_panel".to_string(),
                "welcome".to_string(),
                "workspace".to_string(),
                "zaku".to_string(),
            ])
        );
    });
}
