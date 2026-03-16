use fs::Fs;
use futures::{StreamExt, channel::mpsc::UnboundedReceiver};
use gpui::{BackgroundExecutor, Task};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

const FILE_WATCH_LATENCY: Duration = Duration::from_millis(100);

async fn load_config_file(fs: &dyn Fs, path: &Path) -> String {
    if fs.metadata(path).await.ok().flatten().is_none() {
        return crate::default_user_settings().into_owned();
    }

    match fs.load(path).await {
        Ok(contents) => contents,
        Err(error) => {
            log::error!("Failed to load settings file: {error}");
            crate::default_user_settings().into_owned()
        }
    }
}

pub fn watch_config_file(
    executor: &BackgroundExecutor,
    fs: Arc<dyn Fs>,
    path: PathBuf,
) -> (UnboundedReceiver<String>, Task<()>) {
    let (tx, rx) = futures::channel::mpsc::unbounded();
    let task = executor.spawn(async move {
        let (events, _) = fs.watch(&path, FILE_WATCH_LATENCY).await;
        futures::pin_mut!(events);

        let contents = load_config_file(fs.as_ref(), &path).await;
        if tx.unbounded_send(contents).is_err() {
            return;
        }

        loop {
            if events.next().await.is_none() {
                break;
            }

            let contents = load_config_file(fs.as_ref(), &path).await;
            if tx.unbounded_send(contents).is_err() {
                break;
            }
        }
    });

    (rx, task)
}
