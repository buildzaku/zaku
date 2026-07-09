use anyhow::Context;
use gpui::{App, Entity, EventEmitter, Global, SharedString, Task, WeakEntity, Window, prelude::*};
use std::sync::Arc;

use fs::Fs;
use migrator::{migrate_keymap, migrate_settings};
use settings::{KeymapFile, SettingsStore};
use theme::ActiveTheme;
use ui::{
    Button, ButtonCommon, ButtonVariant, Clickable, Color, DynamicSpacing, Icon, IconAsset,
    IconSize, Text, TextCommon,
};
use workspace::{
    AppState, ItemHandle, ToolbarItemEvent, ToolbarItemLocation, ToolbarItemView, Workspace,
    notifications::DetachAndPromptErr,
};

pub(crate) enum MigrationEvent {
    ContentChanged {
        migration_type: MigrationType,
        using_in_memory_migration: bool,
    },
}

pub(crate) struct MigrationNotification;

impl MigrationNotification {
    pub(crate) fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalMigrationNotification>()
            .map(|notifier| notifier.0.clone())
    }

    pub(crate) fn set_global(notifier: Entity<Self>, cx: &mut App) {
        cx.set_global(GlobalMigrationNotification(notifier));
    }
}

impl EventEmitter<MigrationEvent> for MigrationNotification {}

struct GlobalMigrationNotification(Entity<MigrationNotification>);

impl Global for GlobalMigrationNotification {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MigrationType {
    Keymap,
    Settings,
}

pub(crate) struct MigrationBanner {
    workspace: WeakEntity<Workspace>,
    migration_type: Option<MigrationType>,
    should_migrate_task: Option<Task<()>>,
    message: Option<SharedString>,
}

impl MigrationBanner {
    pub(crate) fn new(workspace: WeakEntity<Workspace>, cx: &mut gpui::Context<Self>) -> Self {
        if let Some(notifier) = MigrationNotification::try_global(cx) {
            cx.subscribe(
                &notifier,
                move |migration_banner, _, event: &MigrationEvent, cx| {
                    migration_banner.handle_notification(event, cx);
                },
            )
            .detach();
        }
        Self {
            workspace,
            migration_type: None,
            should_migrate_task: None,
            message: None,
        }
    }

    fn handle_notification(&mut self, event: &MigrationEvent, cx: &mut gpui::Context<Self>) {
        match event {
            MigrationEvent::ContentChanged {
                migration_type,
                using_in_memory_migration,
            } => {
                if *using_in_memory_migration {
                    self.migration_type = Some(*migration_type);
                    self.show(cx);
                } else {
                    cx.emit(ToolbarItemEvent::ChangeLocation(
                        ToolbarItemLocation::Hidden,
                    ));
                    self.reset(cx);
                }
            }
        }
    }

    fn show(&mut self, cx: &mut gpui::Context<Self>) {
        let (file_type, backup_file_name) = match self.migration_type {
            Some(MigrationType::Keymap) => (
                "keymap",
                path::keymap_backup_file()
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            ),
            Some(MigrationType::Settings) => (
                "settings",
                path::settings_backup_file()
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            ),
            None => return,
        };

        self.message = Some(
            format!(
                "Your {file_type} file uses deprecated settings which can be automatically \
                updated. A backup will be saved to {backup_file_name}"
            )
            .into(),
        );

        cx.emit(ToolbarItemEvent::ChangeLocation(
            ToolbarItemLocation::Secondary,
        ));
        cx.notify();
    }

    fn reset(&mut self, cx: &mut gpui::Context<Self>) {
        self.should_migrate_task.take();
        self.migration_type.take();
        self.message.take();
        cx.notify();
    }
}

impl EventEmitter<ToolbarItemEvent> for MigrationBanner {}

impl ToolbarItemView for MigrationBanner {
    fn set_active_pane_item(
        &mut self,
        active_pane_item: Option<&dyn ItemHandle>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> ToolbarItemLocation {
        self.reset(cx);

        let Some(project_path) = active_pane_item.and_then(|item| item.project_path(cx)) else {
            return ToolbarItemLocation::Hidden;
        };
        let Some(workspace) = self.workspace.upgrade() else {
            return ToolbarItemLocation::Hidden;
        };
        let project = workspace.read(cx).project().clone();
        let Some(target) = project.read(cx).absolute_path(&project_path, cx) else {
            return ToolbarItemLocation::Hidden;
        };

        if target.as_path() == path::keymap_file().as_path() {
            self.migration_type = Some(MigrationType::Keymap);
            let fs = AppState::global(cx).fs.clone();
            let should_migrate = cx.background_spawn(should_migrate_keymap(fs));
            self.should_migrate_task = Some(cx.spawn_in(window, async move |this, cx| {
                if let Ok(true) = should_migrate.await
                    && let Err(error) = this.update(cx, |this, cx| {
                        this.show(cx);
                    })
                {
                    log::trace!("Failed to show migration banner: {error:?}");
                }
            }));
        } else if target.as_path() == path::settings_file().as_path() {
            self.migration_type = Some(MigrationType::Settings);
            let fs = AppState::global(cx).fs.clone();
            let should_migrate = cx.background_spawn(should_migrate_settings(fs));
            self.should_migrate_task = Some(cx.spawn_in(window, async move |this, cx| {
                if let Ok(true) = should_migrate.await
                    && let Err(error) = this.update(cx, |this, cx| {
                        this.show(cx);
                    })
                {
                    log::trace!("Failed to show migration banner: {error:?}");
                }
            }));
        }

        ToolbarItemLocation::Hidden
    }
}

impl Render for MigrationBanner {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let migration_type = self.migration_type;

        gpui::div()
            .flex()
            .items_center()
            .justify_between()
            .gap(DynamicSpacing::Base08.rems(cx))
            .py_1()
            .pl_2()
            .pr_1()
            .bg(cx.theme().status().info_background.opacity(0.6))
            .border_1()
            .border_color(cx.theme().colors().border_variant)
            .rounded_sm()
            .child(
                gpui::div()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base08.rems(cx))
                    .min_w_0()
                    .overflow_hidden()
                    .child(
                        Icon::new(IconAsset::Warning)
                            .size(IconSize::XSmall)
                            .color(Color::Warning),
                    )
                    .when_some(self.message.clone(), |this, message| {
                        this.child(Text::new(message).truncate())
                    }),
            )
            .child(
                Button::new("backup-and-migrate", "Backup and Update")
                    .variant(ButtonVariant::Solid)
                    .on_click(move |_, window, cx| {
                        let Some(migration_type) = migration_type else {
                            return;
                        };
                        let fs = AppState::global(cx).fs.clone();
                        let (task, message) = match migration_type {
                            MigrationType::Keymap => (
                                cx.background_spawn(write_keymap_migration(fs)),
                                "Failed to update keymap file",
                            ),
                            MigrationType::Settings => (
                                cx.background_spawn(write_settings_migration(fs)),
                                "Failed to update settings file",
                            ),
                        };
                        task.detach_and_prompt_err(message, window, cx, |_, _, _| None);
                    }),
            )
            .into_any_element()
    }
}

async fn should_migrate_keymap(fs: Arc<dyn Fs>) -> anyhow::Result<bool> {
    let old_text = KeymapFile::load_keymap_file(&fs).await?;
    if let Ok(Some(_)) = migrate_keymap(&old_text) {
        return Ok(true);
    }
    Ok(false)
}

async fn should_migrate_settings(fs: Arc<dyn Fs>) -> anyhow::Result<bool> {
    let old_text = SettingsStore::load_settings(&fs).await?;
    if let Ok(Some(_)) = migrate_settings(&old_text) {
        return Ok(true);
    }
    Ok(false)
}

async fn write_keymap_migration(fs: Arc<dyn Fs>) -> anyhow::Result<()> {
    let old_text = KeymapFile::load_keymap_file(&fs).await?;
    let Ok(Some(new_text)) = migrate_keymap(&old_text) else {
        return Ok(());
    };
    let keymap_path = path::keymap_file().as_path();
    if fs
        .metadata(keymap_path)
        .await?
        .is_some_and(|metadata| !metadata.is_dir)
    {
        fs.atomic_write(path::keymap_backup_file().clone(), old_text)
            .await
            .context("failed to create keymap backup in config directory")?;
        let resolved_path = fs.canonicalize(keymap_path).await.with_context(|| {
            format!(
                "failed to canonicalize keymap path {}",
                keymap_path.display()
            )
        })?;
        fs.atomic_write(resolved_path.clone(), new_text)
            .await
            .with_context(|| {
                format!("failed to write keymap to file {}", resolved_path.display())
            })?;
    } else {
        fs.atomic_write(keymap_path.to_path_buf(), new_text)
            .await
            .with_context(|| format!("failed to write keymap to file {}", keymap_path.display()))?;
    }
    Ok(())
}

async fn write_settings_migration(fs: Arc<dyn Fs>) -> anyhow::Result<()> {
    let old_text = SettingsStore::load_settings(&fs).await?;
    let Ok(Some(new_text)) = migrate_settings(&old_text) else {
        return Ok(());
    };
    let settings_path = path::settings_file().as_path();
    if fs
        .metadata(settings_path)
        .await?
        .is_some_and(|metadata| !metadata.is_dir)
    {
        fs.atomic_write(path::settings_backup_file().clone(), old_text)
            .await
            .context("failed to create settings backup in config directory")?;
        let resolved_path = fs.canonicalize(settings_path).await.with_context(|| {
            format!(
                "failed to canonicalize settings path {}",
                settings_path.display()
            )
        })?;
        fs.atomic_write(resolved_path.clone(), new_text)
            .await
            .with_context(|| {
                format!(
                    "failed to write settings to file {}",
                    resolved_path.display()
                )
            })?;
    } else {
        fs.atomic_write(settings_path.to_path_buf(), new_text)
            .await
            .with_context(|| {
                format!(
                    "failed to write settings to file {}",
                    settings_path.display()
                )
            })?;
    }
    Ok(())
}
