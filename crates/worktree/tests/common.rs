use futures::StreamExt;
use gpui::{Entity, TestAppContext};
use std::time::Duration;

use util::rel_path::RelPath;
use worktree::Worktree;

pub async fn flush_fs_events(worktree: &Entity<Worktree>, cx: &mut TestAppContext) {
    let file_name = "marker.toml";
    let tree = worktree.clone();
    let (fs, root_path) = tree.read_with(cx, |tree, _| {
        let tree = tree.as_local().unwrap();
        (tree.fs().clone(), tree.abs_path().to_path_buf())
    });

    let mut events = cx.events(&tree);

    fs.write(&root_path.join(file_name), &[]).await.unwrap();

    let file_exists = || {
        tree.read_with(cx, |tree, _| {
            tree.entry_for_path(RelPath::unix(file_name).unwrap())
                .is_some()
        })
    };

    while !file_exists() {
        futures::select_biased! {
            _ = events.next() => {}
            _ = futures::FutureExt::fuse(cx.background_executor.timer(Duration::from_millis(10))) => {}
        }
    }

    fs.remove_file(&root_path.join(file_name), Default::default())
        .await
        .unwrap();

    let file_gone = || {
        tree.read_with(cx, |tree, _| {
            tree.entry_for_path(RelPath::unix(file_name).unwrap())
                .is_none()
        })
    };

    while !file_gone() {
        futures::select_biased! {
            _ = events.next() => {}
            _ = futures::FutureExt::fuse(cx.background_executor.timer(Duration::from_millis(10))) => {}
        }
    }

    cx.update(|cx| tree.read(cx).as_local().unwrap().scan_complete())
        .await;
}
