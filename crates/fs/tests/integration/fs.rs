use futures::{FutureExt, StreamExt};
use gpui::{BackgroundExecutor, TestAppContext};
use std::{collections::BTreeSet, time::Duration};
use tempfile::TempDir;

#[cfg(unix)]
use std::path::PathBuf;

use fs::{Fs, NativeFs, PathEventKind, RenameOptions};

#[cfg(target_os = "windows")]
use util::path::SanitizedPath;

#[gpui::test]
async fn test_native_fs_parallel_rename_without_overwrite_preserves_failed_source(
    executor: BackgroundExecutor,
) {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    let source_a = root.join("dir_a/shared.txt");
    let source_b = root.join("dir_b/shared.txt");
    let target = root.join("shared.txt");

    std::fs::create_dir_all(source_a.parent().unwrap()).unwrap();
    std::fs::create_dir_all(source_b.parent().unwrap()).unwrap();
    std::fs::write(&source_a, "from a").unwrap();
    std::fs::write(&source_b, "from b").unwrap();

    let fs = NativeFs::new(executor);
    let (first_result, second_result) = futures::future::join(
        fs.rename(&source_a, &target, RenameOptions::default()),
        fs.rename(&source_b, &target, RenameOptions::default()),
    )
    .await;

    assert_ne!(first_result.is_ok(), second_result.is_ok());
    assert!(target.exists());
    assert_eq!(source_a.exists() as u8 + source_b.exists() as u8, 1);
}

#[gpui::test]
async fn test_native_fs_rename_ignore_if_exists_leaves_source_and_target_unchanged(
    executor: BackgroundExecutor,
) {
    let fs = NativeFs::new(executor);
    let temp_dir = TempDir::new().unwrap();
    let source = temp_dir.path().join("source.txt");
    let target = temp_dir.path().join("target.txt");

    std::fs::write(&source, "from source").unwrap();
    std::fs::write(&target, "from target").unwrap();

    let result = fs
        .rename(
            &source,
            &target,
            RenameOptions {
                ignore_if_exists: true,
                ..Default::default()
            },
        )
        .await;

    assert!(result.is_ok());
    assert_eq!(std::fs::read_to_string(&source).unwrap(), "from source");
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "from target");
}

#[gpui::test]
async fn test_native_fs_rename_respects_create_parents(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();

    let fs = NativeFs::new(executor);
    let temp_dir = TempDir::new().unwrap();
    let source_a = temp_dir.path().join("src/file_a.txt");
    let source_b = temp_dir.path().join("src/file_b.txt");
    let created_target = temp_dir.path().join("src/new/renamed_a.txt");
    let missing_parent_target = temp_dir.path().join("src/old/renamed_b.txt");

    std::fs::create_dir_all(source_a.parent().unwrap()).unwrap();
    std::fs::write(&source_a, "content a").unwrap();
    std::fs::write(&source_b, "content b").unwrap();

    fs.rename(
        &source_a,
        &created_target,
        RenameOptions {
            create_parents: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    assert!(!source_a.exists());
    assert_eq!(
        std::fs::read_to_string(&created_target).unwrap(),
        "content a"
    );

    let result = fs
        .rename(
            &source_b,
            &missing_parent_target,
            RenameOptions {
                create_parents: false,
                ..Default::default()
            },
        )
        .await;

    assert!(result.is_err());
    assert_eq!(std::fs::read_to_string(&source_b).unwrap(), "content b");
    assert!(!missing_parent_target.exists());
}

#[gpui::test]
#[cfg(target_os = "windows")]
async fn test_native_fs_canonicalize(executor: BackgroundExecutor) {
    let fs = NativeFs::new(executor);
    let temp_dir = TempDir::new().unwrap();
    let file = temp_dir.path().join("test (1).txt");
    let file = SanitizedPath::new(&file);

    std::fs::write(&file, "test").unwrap();

    let canonicalized = fs.canonicalize(file.as_path()).await;
    assert!(canonicalized.is_ok());
}

#[gpui::test]
#[cfg(unix)]
async fn test_native_fs_broken_symlink_metadata(executor: BackgroundExecutor) {
    let tempdir = TempDir::new().unwrap();
    let path = tempdir.path();
    let fs = NativeFs::new(executor);
    let symlink_path = path.join("symlink");

    smol::block_on(fs.create_symlink(&symlink_path, PathBuf::from("file_a.txt"))).unwrap();

    let metadata = fs
        .metadata(&symlink_path)
        .await
        .expect("metadata call succeeds")
        .expect("metadata returned");

    assert!(metadata.is_symlink);
    assert!(!metadata.is_dir);
    assert!(!metadata.is_fifo);
    assert!(!metadata.is_executable);
}

#[gpui::test]
#[cfg(unix)]
async fn test_native_fs_self_referential_symlink_metadata(executor: BackgroundExecutor) {
    let tempdir = TempDir::new().unwrap();
    let path = tempdir.path();
    let fs = NativeFs::new(executor);
    let symlink_path = path.join("symlink");

    smol::block_on(fs.create_symlink(&symlink_path, PathBuf::from("symlink"))).unwrap();

    let metadata = fs
        .metadata(&symlink_path)
        .await
        .expect("metadata call succeeds")
        .expect("metadata returned");

    assert!(metadata.is_symlink);
    assert!(!metadata.is_dir);
    assert!(!metadata.is_fifo);
    assert!(!metadata.is_executable);
}

#[gpui::test]
async fn test_native_fs_watch_stress_reports_rescan_when_paths_are_missed(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    const FILE_COUNT: usize = 32000;
    cx.executor().allow_parking();

    let fs = NativeFs::new(executor.clone());
    let temp_dir = TempDir::new().expect("create temp dir");
    let root = temp_dir.path();

    let mut file_paths = Vec::with_capacity(FILE_COUNT);
    let mut expected_paths = BTreeSet::new();

    for index in 0..FILE_COUNT {
        let dir_path = root.join(format!("dir-{index:04}"));
        let file_path = dir_path.join("file.txt");
        fs.create_dir(&dir_path).await.expect("create watched dir");
        fs.write(&file_path, b"before")
            .await
            .expect("create initial file");
        expected_paths.insert(file_path.clone());
        file_paths.push(file_path);
    }

    let (mut events, watcher) = fs.watch(root, Duration::from_millis(10)).await;
    let watcher = watcher;

    for file_path in &expected_paths {
        watcher
            .add(file_path.parent().expect("file has parent"))
            .expect("add explicit directory watch");
    }

    for (index, file_path) in file_paths.iter().enumerate() {
        let content = format!("after-{index}");
        fs.write(file_path, content.as_bytes())
            .await
            .expect("modify watched file");
    }

    let mut changed_paths = BTreeSet::new();
    let mut rescan_count: u32 = 0;
    let timeout = executor.timer(Duration::from_secs(10)).fuse();

    futures::pin_mut!(timeout);

    let mut ticks = 0;
    while ticks < 1000 {
        if let Some(batch) = events.next().fuse().now_or_never().flatten() {
            for event in batch {
                if event.kind == Some(PathEventKind::Rescan) {
                    rescan_count += 1;
                }
                if expected_paths.contains(&event.path) {
                    changed_paths.insert(event.path);
                }
            }
            if changed_paths.len() == expected_paths.len() {
                break;
            }
            ticks = 0;
        } else {
            ticks += 1;
            executor.timer(Duration::from_millis(10)).await;
        }
    }

    let missed_paths: BTreeSet<_> = expected_paths.difference(&changed_paths).cloned().collect();

    eprintln!(
        "nativefs watch stress: expected={}, observed={}, missed={}, rescan={}",
        expected_paths.len(),
        changed_paths.len(),
        missed_paths.len(),
        rescan_count
    );

    assert!(
        missed_paths.is_empty() || rescan_count > 0,
        "missed {} paths without rescan being reported",
        missed_paths.len()
    );
}
