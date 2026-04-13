use gpui::TestAppContext;
use indoc::indoc;
use serde_json::json;
use std::sync::Arc;

#[cfg(any(target_os = "macos", target_os = "linux"))]
use fs::Fs;

use fs::TempFs;
use project::Project;
use util::rel_path::{RelPath, rel_path};
use util_macros::path;

#[gpui::test]
async fn test_newer_find_or_create_worktree_request_supersedes_previous_request(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();

    let temp_fs = Arc::new(TempFs::new(cx.executor()));
    temp_fs.insert_tree(path!("first"), Default::default());
    temp_fs.insert_tree(path!("second"), Default::default());

    let first_path = temp_fs.path().join("first");
    let second_path = temp_fs.path().join("second");

    let project = cx
        .update(|cx| Project::open_local(temp_fs.clone(), temp_fs.path().join("project"), cx))
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

    let temp_fs = Arc::new(TempFs::new(cx.executor()));
    temp_fs.insert_tree(path!("first"), Default::default());
    temp_fs.insert_tree(path!("second"), Default::default());

    let first_path = temp_fs.path().join("first");
    let second_path = temp_fs.path().join("second");

    let project = cx
        .update(|cx| Project::open_local(temp_fs.clone(), first_path, cx))
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
    assert!(cx.update(|cx| project.read(cx).worktree(cx)).is_none());
    assert!(cx.update(|cx| project.read(cx).root(cx)).is_none());
}

#[gpui::test]
async fn test_open_local_project_creates_worktree(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let temp_fs = Arc::new(TempFs::new(cx.executor()));
    temp_fs.insert_tree(path!("project"), Default::default());
    let project_path = temp_fs.path().join("project");

    let project = cx
        .update(|cx| Project::open_local(temp_fs.clone(), project_path.clone(), cx))
        .await
        .expect("project open should succeed");

    project
        .read_with(cx, |project, cx| project.wait_for_initial_scan(cx))
        .await;

    let (current_worktree, current_root) = cx.update(|cx| {
        let project = project.read(cx);
        (project.worktree(cx), project.root(cx))
    });

    assert!(current_worktree.is_some());
    assert_eq!(current_root, Some(project_path));
}

#[gpui::test]
async fn test_find_or_create_worktree_replaces_existing_worktree(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let temp_fs = Arc::new(TempFs::new(cx.executor()));
    temp_fs.insert_tree(path!("first"), Default::default());
    temp_fs.insert_tree(path!("second"), Default::default());

    let first_path = temp_fs.path().join("first");
    let second_path = temp_fs.path().join("second");

    let project = cx
        .update(|cx| Project::open_local(temp_fs.clone(), first_path.clone(), cx))
        .await
        .expect("project open should succeed");

    project
        .read_with(cx, |project, cx| project.wait_for_initial_scan(cx))
        .await;

    let first_worktree = cx
        .update(|cx| project.read(cx).worktree(cx))
        .expect("first project open should install a worktree");
    let first_worktree_id = first_worktree.read_with(cx, |worktree, _| worktree.id());

    let second_worktree = project
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
            .worktree(cx)
            .map(|worktree| worktree.read(cx).id())
    });
    assert_ne!(current_worktree_id, Some(first_worktree_id));
}

#[gpui::test]
async fn test_find_or_create_worktree_reuses_existing_worktree_for_equivalent_canonicalized_path(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();

    let temp_fs = Arc::new(TempFs::new(cx.executor()));
    temp_fs.insert_tree(path!("project"), Default::default());

    let canonical_project_path = temp_fs.path().join("project");
    let alternate_project_path = canonical_project_path.join("..").join("project");

    let project = cx
        .update(|cx| Project::open_local(temp_fs.clone(), canonical_project_path.clone(), cx))
        .await
        .expect("project open should succeed");

    project
        .read_with(cx, |project, cx| project.wait_for_initial_scan(cx))
        .await;

    let first_worktree = cx
        .update(|cx| project.read(cx).worktree(cx))
        .expect("first project open should create a worktree");
    let second_worktree = project
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

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[gpui::test]
async fn test_find_or_create_worktree_reuses_existing_worktree_for_equivalent_symlinked_path(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();

    let temp_fs = Arc::new(TempFs::new(cx.executor()));
    temp_fs.insert_tree(path!("project"), Default::default());

    let project_path = temp_fs.path().join("project");
    let alias_project_path = temp_fs.path().join("project-alias");

    temp_fs
        .create_symlink(&alias_project_path, project_path.clone())
        .await
        .unwrap();

    let project = cx
        .update(|cx| Project::open_local(temp_fs.clone(), alias_project_path.clone(), cx))
        .await
        .expect("project open should succeed");

    project
        .read_with(cx, |project, cx| project.wait_for_initial_scan(cx))
        .await;

    let first_worktree = cx
        .update(|cx| project.read(cx).worktree(cx))
        .expect("first project open should create a worktree");

    assert_eq!(
        cx.update(|cx| project.read(cx).root(cx)),
        Some(project_path.clone())
    );

    let second_worktree = project
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

    let temp_fs = Arc::new(TempFs::new(cx.executor()));
    temp_fs.insert_tree(
        path!("project"),
        json!({
            "nested": {
                "request.toml": indoc! {r#"
                    [meta]
                    version = 1
                "#}
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

    let temp_fs = Arc::new(TempFs::new(cx.executor()));
    temp_fs.insert_tree(path!("project"), Default::default());

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
