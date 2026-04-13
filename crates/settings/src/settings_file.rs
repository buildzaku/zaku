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

        let contents = fs.load(&path).await.unwrap_or_default();
        if tx.unbounded_send(contents).is_err() {
            return;
        }

        loop {
            if events.next().await.is_none() {
                break;
            }

            if let Ok(contents) = fs.load(&path).await
                && tx.unbounded_send(contents).is_err()
            {
                break;
            }
        }
    });

    (rx, task)
}

#[cfg(test)]
mod tests {
    use super::*;

    use fs::TempFs;
    use gpui::TestAppContext;
    use indoc::indoc;

    #[gpui::test]
    async fn test_watch_config_file_uses_empty_string_when_file_is_missing(
        cx: &mut TestAppContext,
    ) {
        cx.executor().allow_parking();

        let temp_fs = Arc::new(TempFs::new(cx.executor()));
        let settings_path = temp_fs.path().join("settings.json");
        let (mut receiver, _watcher) =
            watch_config_file(&cx.background_executor, temp_fs, settings_path);

        assert_eq!(receiver.next().await.as_deref(), Some(""));
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
            receiver.next().await.as_deref(),
            Some(indoc! {r#"
                { "ui_font_size": 13 }
            "#})
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
            receiver.next().await.as_deref(),
            Some(indoc! {r#"
                { "ui_font_size": 14 }
            "#})
        );
    }
}
