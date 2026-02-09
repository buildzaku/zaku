use futures::channel::mpsc;
use gpui::{BackgroundExecutor, Task};
use notify::{RecursiveMode, Watcher};
use std::path::{Path, PathBuf};

fn read_config_file(path: &Path) -> String {
    match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            crate::default_user_settings().into_owned()
        }
        Err(error) => {
            eprintln!("failed to load settings file: {error}");
            crate::default_user_settings().into_owned()
        }
    }
}

fn existing_watch_root(target_path: &Path) -> Option<(PathBuf, RecursiveMode)> {
    let file_parent = target_path.parent()?.to_path_buf();
    let mut watch_root = file_parent.clone();

    while !watch_root.is_dir() {
        if !watch_root.pop() {
            return None;
        }
    }

    let mode = if watch_root == file_parent {
        RecursiveMode::NonRecursive
    } else {
        RecursiveMode::Recursive
    };

    Some((watch_root, mode))
}

pub fn watch_config_file(
    executor: &BackgroundExecutor,
    path: PathBuf,
) -> (mpsc::UnboundedReceiver<String>, Task<()>) {
    let (tx, rx) = mpsc::unbounded();
    let task = executor.spawn(async move {
        let file_parent = path
            .parent()
            .map(|path| path.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));

        let (watch_root, mode) = match existing_watch_root(&path) {
            Some(result) => result,
            None => {
                eprintln!(
                    "failed to watch settings file: no existing parent directory for {path:?}"
                );
                let contents = read_config_file(&path);
                if tx.unbounded_send(contents).is_err() {
                    return;
                }
                return;
            }
        };

        let path_for_callback = path.clone();
        let file_parent_for_callback = file_parent.clone();
        let tx_for_callback = tx.clone();
        let mut watcher =
            match notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
                match result {
                    Ok(event) => {
                        let is_relevant = event.paths.iter().any(|changed_path| {
                            changed_path == &path_for_callback
                                || changed_path.parent() == Some(file_parent_for_callback.as_path())
                        });
                        if !is_relevant {
                            return;
                        }

                        let contents = read_config_file(&path_for_callback);
                        if tx_for_callback.unbounded_send(contents).is_err() {
                            return;
                        }
                    }
                    Err(error) => {
                        eprintln!("failed to watch settings file: {error}");
                    }
                }
            }) {
                Ok(watcher) => watcher,
                Err(error) => {
                    eprintln!("failed to watch settings file: {error}");
                    let contents = read_config_file(&path);
                    if tx.unbounded_send(contents).is_err() {
                        return;
                    }
                    return;
                }
            };

        if let Err(error) = watcher.watch(&watch_root, mode) {
            eprintln!("failed to watch settings file: {error}");
            let contents = read_config_file(&path);
            if tx.unbounded_send(contents).is_err() {
                return;
            }
            return;
        }

        let contents = read_config_file(&path);
        if tx.unbounded_send(contents).is_err() {
            return;
        }

        futures::future::pending::<()>().await;
    });

    (rx, task)
}
