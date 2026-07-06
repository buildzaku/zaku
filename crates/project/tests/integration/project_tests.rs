use gpui::TestAppContext;
use indoc::indoc;
use language::LanguageRegistry;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc, sync::Arc};

#[cfg(any(target_os = "linux", target_os = "macos"))]
use fs::Fs;

use fs::TempFs;
use path::{RelPath, rel_path};
use project::{Project, ProjectItem, RequestBuffer, RequestBufferEvent};
use util_macros::path;
use worktree::WorktreeModelHandle;

#[gpui::test]
async fn test_newer_find_or_create_worktree_request_supersedes_previous_request(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();

    let temp_fs = TempFs::new(cx.executor());
    temp_fs.insert_tree(path!("first"), Value::default());
    temp_fs.insert_tree(path!("second"), Value::default());

    let first_path = temp_fs.path().join("first");
    let second_path = temp_fs.path().join("second");

    let languages = Arc::new(LanguageRegistry::test_new(cx.executor()));
    let project = cx
        .update(|cx| {
            Project::open(
                temp_fs.clone(),
                languages.clone(),
                temp_fs.path().join("project"),
                cx,
            )
        })
        .await
        .expect("project open should succeed");

    project
        .read_with(cx, |project, cx| project.wait_for_initial_scan(cx))
        .await;

    let first_open = project.update(cx, |project, cx| {
        project.find_or_create_worktree(&first_path, true, cx)
    });
    let second_open = project.update(cx, |project, cx| {
        project.find_or_create_worktree(&second_path, true, cx)
    });

    second_open
        .await
        .expect("newer project open should succeed");

    project
        .read_with(cx, |project, cx| project.wait_for_initial_scan(cx))
        .await;

    assert!(
        first_open.await.is_err(),
        "older project open should not report success once superseded"
    );
    assert_eq!(cx.update(|cx| project.read(cx).root(cx)), Some(second_path));
}

#[gpui::test]
async fn test_remove_worktree_invalidates_pending_find_or_create_worktree_request(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();

    let temp_fs = TempFs::new(cx.executor());
    temp_fs.insert_tree(path!("first"), Value::default());
    temp_fs.insert_tree(path!("second"), Value::default());

    let first_path = temp_fs.path().join("first");
    let second_path = temp_fs.path().join("second");

    let languages = Arc::new(LanguageRegistry::test_new(cx.executor()));
    let project = cx
        .update(|cx| Project::open(temp_fs.clone(), languages.clone(), first_path, cx))
        .await
        .expect("project open should succeed");

    project
        .read_with(cx, |project, cx| project.wait_for_initial_scan(cx))
        .await;

    let second_open = project.update(cx, |project, cx| {
        project.find_or_create_worktree(&second_path, true, cx)
    });

    project.update(cx, |project, cx| {
        project.remove_worktree(cx);
    });
    cx.run_until_parked();

    assert!(
        second_open.await.is_err(),
        "pending project open should be invalidated once the current worktree is removed"
    );
    assert!(cx.update(|cx| project.read(cx).root_worktree(cx)).is_none());
    assert!(cx.update(|cx| project.read(cx).root(cx)).is_none());
}

#[gpui::test]
async fn test_open_project_creates_worktree(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let temp_fs = TempFs::new(cx.executor());
    temp_fs.insert_tree(path!("project"), Value::default());
    let project_path = temp_fs.path().join("project");

    let languages = Arc::new(LanguageRegistry::test_new(cx.executor()));
    let project = cx
        .update(|cx| Project::open(temp_fs.clone(), languages.clone(), project_path.clone(), cx))
        .await
        .expect("project open should succeed");

    project
        .read_with(cx, |project, cx| project.wait_for_initial_scan(cx))
        .await;

    let (current_worktree, current_root) = cx.update(|cx| {
        let project = project.read(cx);
        (project.root_worktree(cx), project.root(cx))
    });

    assert!(current_worktree.is_some());
    assert_eq!(current_root, Some(project_path));
}

#[gpui::test]
async fn test_open_buffer_at_uses_hidden_worktree_for_external_file(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let temp_fs = TempFs::new(cx.executor());
    temp_fs.insert_tree(path!("project"), Value::default());
    let settings_content = indoc! {r#"
        {
          "ui": { "font_size": 14 },
          "editor": { "font_size": 12 }
        }
    "#};
    temp_fs.insert_tree(path!("settings.json"), json!(settings_content));

    let project_path = temp_fs.path().join("project");
    let settings_path = temp_fs.path().join("settings.json");

    let languages = Arc::new(LanguageRegistry::test_new(cx.executor()));
    let project = cx
        .update(|cx| Project::open(temp_fs.clone(), languages.clone(), project_path.clone(), cx))
        .await
        .expect("project open should succeed");

    project
        .read_with(cx, |project, cx| project.wait_for_initial_scan(cx))
        .await;

    let buffer = project
        .update(cx, |project, cx| project.open_buffer_at(&settings_path, cx))
        .await
        .expect("external file should open");

    let (buffer_text, opened_path) = buffer.read_with(cx, |buffer, cx| {
        (
            buffer.as_rope().to_string(),
            buffer.project_path(cx).expect("buffer should have a path"),
        )
    });
    let hidden_worktree = cx.update(|cx| {
        project
            .read(cx)
            .worktree_for_id(opened_path.worktree_id, cx)
            .expect("hidden worktree should exist")
    });

    assert_eq!(buffer_text, settings_content);
    assert_eq!(
        cx.update(|cx| project.read(cx).root(cx)),
        Some(project_path)
    );
    assert!(opened_path.path.is_empty());
    assert!(!hidden_worktree.read_with(cx, |worktree, _| worktree.is_visible()));
}

#[gpui::test]
async fn test_find_or_create_worktree_replaces_existing_worktree(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let temp_fs = TempFs::new(cx.executor());
    temp_fs.insert_tree(path!("first"), Value::default());
    temp_fs.insert_tree(path!("second"), Value::default());

    let first_path = temp_fs.path().join("first");
    let second_path = temp_fs.path().join("second");

    let languages = Arc::new(LanguageRegistry::test_new(cx.executor()));
    let project = cx
        .update(|cx| Project::open(temp_fs.clone(), languages.clone(), first_path.clone(), cx))
        .await
        .expect("project open should succeed");

    project
        .read_with(cx, |project, cx| project.wait_for_initial_scan(cx))
        .await;

    let first_worktree = cx.update(|cx| project.read(cx).root_worktree(cx)).unwrap();
    let first_worktree_id = first_worktree.read_with(cx, |worktree, _| worktree.id());

    let (second_worktree, _) = project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(&second_path, true, cx)
        })
        .await
        .expect("second project open should succeed");

    project
        .read_with(cx, |project, cx| project.wait_for_initial_scan(cx))
        .await;

    assert_ne!(first_worktree.entity_id(), second_worktree.entity_id());
    assert_eq!(cx.update(|cx| project.read(cx).root(cx)), Some(second_path));

    let current_worktree_id = cx.update(|cx| {
        project
            .read(cx)
            .root_worktree(cx)
            .map(|worktree| worktree.read(cx).id())
    });
    assert_ne!(current_worktree_id, Some(first_worktree_id));
}

#[gpui::test]
async fn test_find_or_create_worktree_reuses_existing_worktree_for_equivalent_canonicalized_path(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();

    let temp_fs = TempFs::new(cx.executor());
    temp_fs.insert_tree(path!("project"), Value::default());

    let canonical_project_path = temp_fs.path().join("project");
    let alternate_project_path = canonical_project_path.join("..").join("project");

    let languages = Arc::new(LanguageRegistry::test_new(cx.executor()));
    let project = cx
        .update(|cx| {
            Project::open(
                temp_fs.clone(),
                languages.clone(),
                canonical_project_path.clone(),
                cx,
            )
        })
        .await
        .expect("project open should succeed");

    project
        .read_with(cx, |project, cx| project.wait_for_initial_scan(cx))
        .await;

    let first_worktree = cx.update(|cx| project.read(cx).root_worktree(cx)).unwrap();
    let (second_worktree, _) = project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(&alternate_project_path, true, cx)
        })
        .await
        .expect("canonicalized project open should reuse the current worktree");

    assert_eq!(first_worktree.entity_id(), second_worktree.entity_id());
    assert_eq!(
        cx.update(|cx| project.read(cx).root(cx)),
        Some(canonical_project_path)
    );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[gpui::test]
async fn test_find_or_create_worktree_reuses_existing_worktree_for_equivalent_symlinked_path(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();

    let temp_fs = TempFs::new(cx.executor());
    temp_fs.insert_tree(path!("project"), Value::default());

    let project_path = temp_fs.path().join("project");
    let alias_project_path = temp_fs.path().join("project-alias");

    temp_fs
        .create_symlink(&alias_project_path, project_path.clone())
        .await
        .unwrap();

    let languages = Arc::new(LanguageRegistry::test_new(cx.executor()));
    let project = cx
        .update(|cx| {
            Project::open(
                temp_fs.clone(),
                languages.clone(),
                alias_project_path.clone(),
                cx,
            )
        })
        .await
        .expect("project open should succeed");

    project
        .read_with(cx, |project, cx| project.wait_for_initial_scan(cx))
        .await;

    let first_worktree = cx.update(|cx| project.read(cx).root_worktree(cx)).unwrap();

    assert_eq!(
        cx.update(|cx| project.read(cx).root(cx)),
        Some(project_path.clone())
    );

    let (second_worktree, _) = project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(&project_path, true, cx)
        })
        .await
        .expect("second project open should succeed");

    assert_eq!(first_worktree.entity_id(), second_worktree.entity_id());
    assert_eq!(
        cx.update(|cx| project.read(cx).root(cx)),
        Some(project_path)
    );
}

#[gpui::test]
async fn test_absolute_path_resolves_relative_paths_against_current_root(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let temp_fs = TempFs::new(cx.executor());
    temp_fs.insert_tree(
        path!("project"),
        json!({
            "nested": {
                "request.toml": indoc! {"
                    [meta]
                    version = 1
                "}
            }
        }),
    );

    let project_path = temp_fs.path().join("project");
    let request_path = project_path.join("nested").join("request.toml");
    let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;

    let (resolved_request_path, resolved_project_path) = cx.update(|cx| {
        let project = project.read(cx);
        (
            project.absolutize(rel_path("nested/request.toml"), cx),
            project.absolutize(RelPath::empty(), cx),
        )
    });

    assert_eq!(resolved_request_path, Some(request_path));
    assert_eq!(resolved_project_path, Some(project_path));
}

#[gpui::test]
async fn test_initial_scan_complete(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let temp_fs = TempFs::new(cx.executor());
    temp_fs.insert_tree(path!("project"), Value::default());

    let project_path = temp_fs.path().join("project");
    let project = Project::test_new(temp_fs.clone(), &project_path, cx).await;

    project
        .read_with(cx, |project, cx| project.wait_for_initial_scan(cx))
        .await;

    project.read_with(cx, |project, cx| {
        assert!(
            project.worktree_store().read(cx).initial_scan_completed(),
            "expected initial scan to be completed after awaiting wait_for_initial_scan"
        );
    });
}

#[gpui::test(iterations = 10)]
async fn test_buffer_identity_across_renames(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let temp_fs = TempFs::new(cx.executor());
    temp_fs.insert_tree(
        path!("project"),
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

    let project_path = temp_fs.path().join(path!("project"));
    let project = Project::test_new(temp_fs, &project_path, cx).await;
    let worktree = project.update(cx, |project, cx| project.root_worktree(cx).unwrap());
    let worktree_id = worktree.update(cx, |worktree, _| worktree.id());

    let entry_id_for_path = |path: &'static str, cx: &mut TestAppContext| {
        project.update(cx, |project, cx| {
            let worktree = project.root_worktree(cx).unwrap();
            worktree.read(cx).entry_for_path(rel_path(path)).unwrap().id
        })
    };

    let collection_id = entry_id_for_path("collection", cx);
    let request_entry_id = entry_id_for_path("collection/request.toml", cx);
    let buffer = cx
        .update(|cx| {
            <RequestBuffer as ProjectItem>::try_open(
                &project,
                &(worktree_id, rel_path("collection/request.toml")).into(),
                cx,
            )
            .unwrap()
        })
        .await
        .unwrap();
    buffer.update(cx, |buffer, _| assert!(!buffer.is_dirty()));

    let received_file_handle_changed = Rc::new(RefCell::new(false));
    buffer.update(cx, |_, cx| {
        let received_file_handle_changed = received_file_handle_changed.clone();
        cx.subscribe(&buffer, move |_, _, event, _| {
            if matches!(event, RequestBufferEvent::FileHandleChanged) {
                *received_file_handle_changed.borrow_mut() = true;
            }
        })
        .detach();
    });

    project
        .update(cx, |project, cx| {
            project.rename_entry(collection_id, (worktree_id, rel_path("renamed")).into(), cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();
    worktree.flush_fs_events(cx).await;

    assert_eq!(entry_id_for_path("renamed", cx), collection_id);
    assert_eq!(
        entry_id_for_path("renamed/request.toml", cx),
        request_entry_id
    );
    assert!(
        *received_file_handle_changed.borrow(),
        "RequestBufferEvent::FileHandleChanged must be emitted when the open request is moved by a parent rename"
    );
    buffer.update(cx, |buffer, _| {
        assert!(!buffer.is_dirty());
        assert_eq!(
            buffer.file().path.as_ref(),
            rel_path("renamed/request.toml")
        );
    });
}
