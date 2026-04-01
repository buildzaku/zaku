use fs::Fs;
use futures::{StreamExt, channel::mpsc::UnboundedReceiver};
use gpui::{BackgroundExecutor, Task};
use std::{path::PathBuf, sync::Arc, time::Duration};

const FILE_WATCH_LATENCY: Duration = Duration::from_millis(100);

pub fn watch_config_file(
    executor: &BackgroundExecutor,
    fs: Arc<dyn Fs>,
    path: PathBuf,
) -> (UnboundedReceiver<String>, Task<()>) {
    let (tx, rx) = futures::channel::mpsc::unbounded();
    let task = executor.spawn(async move {
        let (events, _) = fs.watch(&path, FILE_WATCH_LATENCY).await;
        futures::pin_mut!(events);

        let contents = match fs.load(&path).await {
            Ok(contents) => contents,
            Err(error) => {
                if let Some(io_error) = error.downcast_ref::<std::io::Error>()
                    && io_error.kind() == std::io::ErrorKind::NotFound
                {
                    crate::default_user_settings().into_owned()
                } else {
                    log::error!("Failed to load settings file: {error}");
                    String::new()
                }
            }
        };
        if tx.unbounded_send(contents).is_err() {
            return;
        }

        loop {
            if events.next().await.is_none() {
                break;
            }

            match fs.load(&path).await {
                Ok(contents) => {
                    if tx.unbounded_send(contents).is_err() {
                        break;
                    }
                }
                Err(error) => {
                    if let Some(io_error) = error.downcast_ref::<std::io::Error>()
                        && io_error.kind() == std::io::ErrorKind::NotFound
                    {
                        if tx
                            .unbounded_send(crate::default_user_settings().into_owned())
                            .is_err()
                        {
                            break;
                        }
                    } else {
                        log::error!("Failed to load settings file: {error}");
                    }
                }
            }
        }
    });

    (rx, task)
}

#[cfg(test)]
mod tests {
    use super::*;

    use fs::TempFs;
    use futures::FutureExt;
    use gpui::TestAppContext;
    use indoc::indoc;

    async fn next_settings_update(
        receiver: &mut UnboundedReceiver<String>,
        cx: &mut TestAppContext,
    ) -> String {
        let next_update = receiver.next().fuse();
        let timeout = cx.background_executor.timer(Duration::from_secs(5)).fuse();
        futures::pin_mut!(next_update, timeout);

        futures::select_biased! {
            update = next_update => update.unwrap_or_else(|| panic!("Settings watcher closed")),
            _ = timeout => panic!("Timed out waiting for settings watcher update"),
        }
    }

    #[gpui::test]
    async fn test_watch_config_file_uses_default_user_settings_when_file_is_missing(
        cx: &mut TestAppContext,
    ) {
        cx.executor().allow_parking();

        let temp_fs = Arc::new(TempFs::new(cx.executor()));
        let settings_path = temp_fs.path().join("settings.json");
        let (mut receiver, _watcher) =
            watch_config_file(&cx.background_executor, temp_fs, settings_path);

        assert_eq!(
            next_settings_update(&mut receiver, cx).await,
            crate::default_user_settings().into_owned()
        );
    }

    #[gpui::test]
    async fn test_watch_config_file_reloads_after_file_change(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = Arc::new(TempFs::new(cx.executor()));
        let settings_path = temp_fs.path().join("settings.json");
        temp_fs
            .write(
                &settings_path,
                indoc! {r#"
                    { "ui_font_size": 13 }
                "#}
                .as_bytes(),
            )
            .await
            .unwrap();

        let (mut receiver, _watcher) = watch_config_file(
            &cx.background_executor,
            temp_fs.clone(),
            settings_path.clone(),
        );

        assert_eq!(
            next_settings_update(&mut receiver, cx).await,
            indoc! {r#"
                { "ui_font_size": 13 }
            "#}
        );

        temp_fs
            .write(
                &settings_path,
                indoc! {r#"
                    { "ui_font_size": 14 }
                "#}
                .as_bytes(),
            )
            .await
            .unwrap();

        assert_eq!(
            next_settings_update(&mut receiver, cx).await,
            indoc! {r#"
                { "ui_font_size": 14 }
            "#}
        );
    }
}
