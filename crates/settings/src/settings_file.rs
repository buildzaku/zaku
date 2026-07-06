use futures::{StreamExt, channel::mpsc::UnboundedReceiver};
use gpui::{App, BackgroundExecutor, ReadGlobal, Task};
use std::{path::PathBuf, sync::Arc, time::Duration};

use fs::Fs;

use crate::{settings_content::SettingsContent, settings_store::SettingsStore};

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

pub fn update_settings_file(
    fs: Arc<dyn Fs>,
    cx: &App,
    update: impl 'static + Send + FnOnce(&mut SettingsContent, &App),
) {
    SettingsStore::global(cx).update_settings_file(fs, update);
}

#[cfg(test)]
mod tests {
    use super::*;

    use gpui::TestAppContext;
    use indoc::indoc;
    use serde_json::json;
    use std::path::Path;

    use fs::{Fs, TempFs};
    use util_macros::path;

    #[gpui::test]
    async fn test_watch_config_file_uses_empty_string_when_file_is_missing(
        cx: &mut TestAppContext,
    ) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let settings_path = temp_fs.path().join("settings.json");
        let (mut receiver, _watcher) =
            watch_config_file(&cx.background_executor, temp_fs, settings_path);

        assert_eq!(receiver.next().await.as_deref(), Some(""));
    }

    #[gpui::test]
    async fn test_watch_config_file_reloads_after_file_change(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let settings_path = temp_fs.path().join("settings.json");
        temp_fs
            .write(
                &settings_path,
                indoc! {r#"
                    { "ui": { "font_size": 14 } }
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
                { "ui": { "font_size": 14 } }
            "#})
        );

        temp_fs
            .write(
                &settings_path,
                indoc! {r#"
                    { "ui": { "font_size": 16 } }
                "#}
                .as_bytes(),
            )
            .await
            .unwrap();

        assert_eq!(
            receiver.next().await.as_deref(),
            Some(indoc! {r#"
                { "ui": { "font_size": 16 } }
            "#})
        );
    }

    #[gpui::test]
    async fn test_watch_config_file_reloads_when_parent_dir_is_symlink(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let config_dir_path = temp_fs.path().join(path!(".config/zaku"));
        let target_dir_path = temp_fs.path().join(path!("dotfiles/zaku"));

        temp_fs.insert_tree(
            Path::new(""),
            json!({
                ".config": {},
                "dotfiles": {
                    "zaku": {
                        "settings.json": "A",
                    },
                },
            }),
        );

        temp_fs
            .create_symlink(&config_dir_path, target_dir_path.clone())
            .await
            .unwrap();

        let (mut receiver, _watcher) = watch_config_file(
            &cx.background_executor,
            temp_fs.clone(),
            config_dir_path.join("settings.json"),
        );

        assert_eq!(receiver.next().await.as_deref(), Some("A"));
        temp_fs
            .write(&target_dir_path.join("settings.json"), b"B")
            .await
            .unwrap();
        assert_eq!(receiver.next().await.as_deref(), Some("B"));
    }
}
