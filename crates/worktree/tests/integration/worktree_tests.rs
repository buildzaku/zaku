#[path = "../common.rs"]
mod common;

use gpui::TestAppContext;
use indoc::indoc;
use parking_lot::Mutex;
use serde_json::json;
use std::{
    mem,
    sync::{Arc, atomic::AtomicUsize},
};

use fs::{Fs, RenameOptions, TempFs};
use util::rel_path::{RelPath, rel_path};
use util_macros::path;
use worktree::{EntryKind, PathChange, Worktree, WorktreeEvent, WorktreeId};

#[gpui::test]
async fn test_traversal(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let temp_fs = Arc::new(TempFs::new(cx.executor()));
    temp_fs.insert_tree(
        "project",
        json!({
            "a.toml": indoc! {"
                [meta]
                version = 1
            "},
            "b": {
                "c.toml": indoc! {"
                    [meta]
                    version = 1
                "},
                "d.toml": indoc! {"
                    [meta]
                    version = 1
                "},
            },
        }),
    );

    let worktree = Worktree::local(
        temp_fs.path().join("project"),
        true,
        temp_fs.clone(),
        Arc::new(AtomicUsize::new(1)),
        true,
        WorktreeId::from_usize(1),
        &mut cx.to_async(),
    )
    .await
    .unwrap();

    cx.update(|cx| worktree.read(cx).as_local().unwrap().scan_complete())
        .await;

    worktree.read_with(cx, |worktree, _| {
        assert_eq!(
            worktree
                .entries(0)
                .map(|entry| entry.path.as_ref())
                .collect::<Vec<_>>(),
            vec![
                rel_path(""),
                rel_path("a.toml"),
                rel_path("b"),
                rel_path("b/c.toml"),
                rel_path("b/d.toml"),
            ]
        );
    });
}

#[gpui::test(iterations = 10)]
async fn test_circular_symlinks(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let temp_fs = Arc::new(TempFs::new(cx.executor()));
    temp_fs.insert_tree(
        "project",
        json!({
            "lib": {
                "a": {
                    "a.toml": indoc! {"
                        [meta]
                        version = 1
                    "},
                },
                "b": {
                    "b.toml": indoc! {"
                        [meta]
                        version = 1
                    "},
                },
            },
        }),
    );

    temp_fs
        .create_symlink(
            &temp_fs.path().join(path!("project/lib/a/lib")),
            "..".into(),
        )
        .await
        .unwrap();
    temp_fs
        .create_symlink(
            &temp_fs.path().join(path!("project/lib/b/lib")),
            "..".into(),
        )
        .await
        .unwrap();

    let worktree = Worktree::local(
        temp_fs.path().join("project"),
        true,
        temp_fs.clone(),
        Arc::new(AtomicUsize::new(1)),
        true,
        WorktreeId::from_usize(1),
        &mut cx.to_async(),
    )
    .await
    .unwrap();

    cx.update(|cx| worktree.read(cx).as_local().unwrap().scan_complete())
        .await;

    worktree.read_with(cx, |worktree, _| {
        assert_eq!(
            worktree
                .entries(0)
                .map(|entry| entry.path.as_ref())
                .collect::<Vec<_>>(),
            vec![
                rel_path(""),
                rel_path("lib"),
                rel_path("lib/a"),
                rel_path("lib/a/a.toml"),
                rel_path("lib/a/lib"),
                rel_path("lib/b"),
                rel_path("lib/b/b.toml"),
                rel_path("lib/b/lib"),
            ]
        );
    });

    temp_fs
        .rename(
            &temp_fs.path().join(path!("project/lib/a/lib")),
            &temp_fs.path().join(path!("project/lib/a/lib-2")),
            RenameOptions::default(),
        )
        .await
        .unwrap();
    common::flush_fs_events(&worktree, cx).await;

    worktree.read_with(cx, |worktree, _| {
        assert_eq!(
            worktree
                .entries(0)
                .map(|entry| entry.path.as_ref())
                .collect::<Vec<_>>(),
            vec![
                rel_path(""),
                rel_path("lib"),
                rel_path("lib/a"),
                rel_path("lib/a/a.toml"),
                rel_path("lib/a/lib-2"),
                rel_path("lib/b"),
                rel_path("lib/b/b.toml"),
                rel_path("lib/b/lib"),
            ]
        );
    });
}

#[gpui::test]
async fn test_symlinks_pointing_outside(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let temp_fs = Arc::new(TempFs::new(cx.executor()));
    temp_fs.insert_tree(
        "project",
        json!({
            "dir1": {
                "deps": {},
                "src": {
                    "local.toml": indoc! {"
                        [meta]
                        version = 1
                    "},
                },
            },
            "dir2": {
                "src": {
                    "c.toml": indoc! {"
                        [meta]
                        version = 1
                    "},
                    "d.toml": indoc! {"
                        [meta]
                        version = 1
                    "},
                },
            },
            "dir3": {
                "deps": {},
                "src": {
                    "e.toml": indoc! {"
                        [meta]
                        version = 1
                    "},
                    "f.toml": indoc! {"
                        [meta]
                        version = 1
                    "},
                },
            },
        }),
    );

    temp_fs
        .create_symlink(
            &temp_fs.path().join(path!("project/dir1/deps/dep-dir2")),
            "../../dir2".into(),
        )
        .await
        .unwrap();
    temp_fs
        .create_symlink(
            &temp_fs.path().join(path!("project/dir1/deps/dep-dir3")),
            "../../dir3".into(),
        )
        .await
        .unwrap();

    let worktree = Worktree::local(
        temp_fs.path().join(path!("project/dir1")),
        true,
        temp_fs.clone(),
        Arc::new(AtomicUsize::new(1)),
        true,
        WorktreeId::from_usize(1),
        &mut cx.to_async(),
    )
    .await
    .unwrap();

    cx.update(|cx| worktree.read(cx).as_local().unwrap().scan_complete())
        .await;

    let worktree_updates = Arc::new(Mutex::new(Vec::new()));
    worktree.update(cx, |_, cx| {
        let worktree_updates = worktree_updates.clone();
        cx.subscribe(&worktree, move |_, _, event, _| {
            let WorktreeEvent::UpdatedEntries(update) = event;
            worktree_updates.lock().extend(
                update
                    .iter()
                    .map(|(path, _, change)| (path.clone(), *change)),
            );
        })
        .detach();
    });

    worktree.read_with(cx, |worktree, _| {
        assert_eq!(
            worktree
                .entries(0)
                .map(|entry| entry.path.as_ref())
                .collect::<Vec<_>>(),
            vec![
                rel_path(""),
                rel_path("deps"),
                rel_path("deps/dep-dir2"),
                rel_path("deps/dep-dir3"),
                rel_path("src"),
                rel_path("src/local.toml"),
            ]
        );
        assert_eq!(
            worktree
                .entry_for_path(RelPath::unix("deps/dep-dir2").unwrap())
                .unwrap()
                .kind,
            EntryKind::UnloadedDir
        );
        assert!(
            worktree
                .entry_for_path(RelPath::unix("deps/dep-dir2").unwrap())
                .unwrap()
                .is_external
        );
        assert_eq!(
            worktree
                .entry_for_path(RelPath::unix("deps/dep-dir3").unwrap())
                .unwrap()
                .kind,
            EntryKind::UnloadedDir
        );
        assert!(
            worktree
                .entry_for_path(RelPath::unix("deps/dep-dir3").unwrap())
                .unwrap()
                .is_external
        );
    });

    cx.update(|cx| {
        worktree
            .read(cx)
            .as_local()
            .unwrap()
            .refresh_entries_for_paths(vec![Arc::from(RelPath::unix("deps/dep-dir3").unwrap())])
    })
    .await
    .unwrap();

    cx.update(|cx| worktree.read(cx).as_local().unwrap().scan_complete())
        .await;

    worktree.read_with(cx, |worktree, _| {
        assert_eq!(
            worktree
                .entries(0)
                .map(|entry| entry.path.as_ref())
                .collect::<Vec<_>>(),
            vec![
                rel_path(""),
                rel_path("deps"),
                rel_path("deps/dep-dir2"),
                rel_path("deps/dep-dir3"),
                rel_path("deps/dep-dir3/deps"),
                rel_path("deps/dep-dir3/src"),
                rel_path("src"),
                rel_path("src/local.toml"),
            ]
        );
        assert_eq!(
            worktree
                .entry_for_path(RelPath::unix("deps/dep-dir3/src").unwrap())
                .unwrap()
                .kind,
            EntryKind::UnloadedDir
        );
        assert!(
            worktree
                .entry_for_path(RelPath::unix("deps/dep-dir3/src").unwrap())
                .unwrap()
                .is_external
        );
    });
    assert_eq!(
        mem::take(&mut *worktree_updates.lock()),
        &[
            (
                Arc::from(RelPath::unix("deps/dep-dir3").unwrap()),
                PathChange::Loaded,
            ),
            (
                Arc::from(RelPath::unix("deps/dep-dir3/deps").unwrap()),
                PathChange::Loaded,
            ),
            (
                Arc::from(RelPath::unix("deps/dep-dir3/src").unwrap()),
                PathChange::Loaded,
            ),
        ]
    );

    cx.update(|cx| {
        worktree
            .read(cx)
            .as_local()
            .unwrap()
            .refresh_entries_for_paths(vec![Arc::from(RelPath::unix("deps/dep-dir3/src").unwrap())])
    })
    .await
    .unwrap();

    cx.update(|cx| worktree.read(cx).as_local().unwrap().scan_complete())
        .await;

    worktree.read_with(cx, |worktree, _| {
        assert_eq!(
            worktree
                .entries(0)
                .map(|entry| entry.path.as_ref())
                .collect::<Vec<_>>(),
            vec![
                rel_path(""),
                rel_path("deps"),
                rel_path("deps/dep-dir2"),
                rel_path("deps/dep-dir3"),
                rel_path("deps/dep-dir3/deps"),
                rel_path("deps/dep-dir3/src"),
                rel_path("deps/dep-dir3/src/e.toml"),
                rel_path("deps/dep-dir3/src/f.toml"),
                rel_path("src"),
                rel_path("src/local.toml"),
            ]
        );
        assert_eq!(
            worktree
                .entry_for_path(RelPath::unix("deps/dep-dir3/src").unwrap())
                .unwrap()
                .kind,
            EntryKind::Dir
        );
        assert!(
            worktree
                .entry_for_path(RelPath::unix("deps/dep-dir3/src/e.toml").unwrap())
                .unwrap()
                .is_external
        );
        assert!(
            worktree
                .entry_for_path(RelPath::unix("deps/dep-dir3/src/f.toml").unwrap())
                .unwrap()
                .is_external
        );
    });
    assert_eq!(
        mem::take(&mut *worktree_updates.lock()),
        &[
            (
                Arc::from(RelPath::unix("deps/dep-dir3/src").unwrap()),
                PathChange::Loaded,
            ),
            (
                Arc::from(RelPath::unix("deps/dep-dir3/src/e.toml").unwrap()),
                PathChange::Loaded,
            ),
            (
                Arc::from(RelPath::unix("deps/dep-dir3/src/f.toml").unwrap()),
                PathChange::Loaded,
            ),
        ]
    );
}

#[gpui::test]
async fn test_renaming_case_only(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    const OLD_NAME: &str = "aaa.toml";
    const NEW_NAME: &str = "AAA.toml";

    let temp_fs = Arc::new(TempFs::new(cx.executor()));
    if temp_fs.is_case_sensitive().await {
        return;
    }

    temp_fs.insert_tree(
        "project",
        json!({
            OLD_NAME: indoc! {"
                [meta]
                version = 1
            "},
        }),
    );

    let worktree = Worktree::local(
        temp_fs.path().join("project"),
        true,
        temp_fs.clone(),
        Arc::new(AtomicUsize::new(1)),
        true,
        WorktreeId::from_usize(1),
        &mut cx.to_async(),
    )
    .await
    .unwrap();

    cx.update(|cx| worktree.read(cx).as_local().unwrap().scan_complete())
        .await;

    worktree.read_with(cx, |worktree, _| {
        assert_eq!(
            worktree
                .entries(0)
                .map(|entry| entry.path.as_ref())
                .collect::<Vec<_>>(),
            vec![rel_path(""), rel_path(OLD_NAME)]
        );
    });

    let old_path = std::path::Path::new("project").join(OLD_NAME);
    let new_path = std::path::Path::new("project").join(NEW_NAME);

    temp_fs
        .rename(
            old_path.as_path(),
            new_path.as_path(),
            RenameOptions {
                overwrite: true,
                ignore_if_exists: true,
                create_parents: false,
            },
        )
        .await
        .unwrap();

    common::flush_fs_events(&worktree, cx).await;

    worktree.read_with(cx, |worktree, _| {
        assert_eq!(
            worktree
                .entries(0)
                .map(|entry| entry.path.as_ref())
                .collect::<Vec<_>>(),
            vec![rel_path(""), rel_path(NEW_NAME)]
        );
    });
}

#[gpui::test]
async fn test_refresh_entries_for_paths_creates_ancestors(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let temp_fs = Arc::new(TempFs::new(cx.executor()));
    temp_fs.insert_tree(
        "project",
        json!({
            "a": {
                "b": {
                    "c": {
                        "deep.toml": indoc! {"
                            [meta]
                            version = 1
                        "},
                        "sibling.toml": indoc! {"
                            [meta]
                            version = 1
                        "},
                    },
                    "d": {
                        "under-sibling-dir.toml": indoc! {"
                            [meta]
                            version = 1
                        "},
                    },
                },
            },
        }),
    );

    let worktree = Worktree::local(
        temp_fs.path().join("project"),
        true,
        temp_fs.clone(),
        Arc::new(AtomicUsize::new(1)),
        false,
        WorktreeId::from_usize(1),
        &mut cx.to_async(),
    )
    .await
    .unwrap();

    cx.update(|cx| worktree.read(cx).as_local().unwrap().scan_complete())
        .await;

    worktree.read_with(cx, |worktree, _| {
        assert_eq!(
            worktree
                .entries(0)
                .map(|entry| entry.path.as_ref())
                .collect::<Vec<_>>(),
            vec![rel_path("")]
        );
    });

    let refresh = cx.update(|cx| {
        worktree
            .read(cx)
            .as_local()
            .unwrap()
            .refresh_entries_for_paths(vec![Arc::from(RelPath::unix("a/b/c/deep.toml").unwrap())])
    });
    refresh
        .await
        .expect("refresh entries for paths should complete");

    worktree.read_with(cx, |worktree, _| {
        assert_eq!(
            worktree
                .entries(0)
                .map(|entry| entry.path.as_ref())
                .collect::<Vec<_>>(),
            vec![
                rel_path(""),
                rel_path("a"),
                rel_path("a/b"),
                rel_path("a/b/c"),
                rel_path("a/b/c/deep.toml"),
                rel_path("a/b/c/sibling.toml"),
                rel_path("a/b/d"),
            ]
        );
    });
}
