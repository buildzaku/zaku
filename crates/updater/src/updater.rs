mod update_version;

use anyhow::Context as _;
use futures::{AsyncReadExt, AsyncWriteExt, StreamExt};
#[cfg(target_os = "macos")]
use gpui::BackgroundExecutor;
#[cfg(target_os = "windows")]
use gpui::Subscription;
use gpui::{
    App, AppContext, AsyncApp, Context, Entity, Global, PromptLevel, Task, TaskExt, Window,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use smol::fs::File;
#[cfg(target_os = "windows")]
use std::io;
#[cfg(target_os = "macos")]
use std::mem;
use std::{
    env::consts::{ARCH, OS},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};
#[cfg(target_os = "linux")]
use std::{error, fmt};

use app_version::{AppVersion, ReleaseChannel};
use db::kv::KeyValueStore;
use http_client::{AsyncBody, HttpClient, http::StatusCode};
use metadata::ZAKU_SERVER_URL;
use settings::{RegisterSetting, Settings, SettingsStore};
use workspace::Workspace;

use crate::update_version::UpdateVersion;

#[cfg(target_os = "linux")]
#[derive(Debug)]
struct MissingDependencyError(String);

#[cfg(target_os = "linux")]
impl fmt::Display for MissingDependencyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

#[cfg(target_os = "linux")]
impl error::Error for MissingDependencyError {}

const SHOULD_SHOW_UPDATE_NOTIFICATION_KEY: &str = "updater-should-show-updated-notification";
const POLL_INTERVAL: Duration = Duration::from_hours(1);
const INSTALLER_DIR_PREFIX: &str = "zaku-updater";

#[cfg(target_os = "linux")]
fn linux_rsync_install_hint() -> &'static str {
    let Ok(os_release) = std::fs::read_to_string("/etc/os-release") else {
        return "Please install rsync using your package manager";
    };

    let mut distribution_ids = Vec::new();
    for line in os_release.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("ID=") {
            distribution_ids.push(value.trim_matches('"').to_ascii_lowercase());
        } else if let Some(value) = line.strip_prefix("ID_LIKE=") {
            for distribution_id in value.trim_matches('"').split_whitespace() {
                distribution_ids.push(distribution_id.to_ascii_lowercase());
            }
        }
    }

    let package_manager_hint = if distribution_ids
        .iter()
        .any(|distribution_id| distribution_id == "arch")
    {
        Some("Install it with: sudo pacman -S rsync")
    } else if distribution_ids
        .iter()
        .any(|distribution_id| distribution_id == "debian" || distribution_id == "ubuntu")
    {
        Some("Install it with: sudo apt install rsync")
    } else if distribution_ids.iter().any(|distribution_id| {
        distribution_id == "fedora"
            || distribution_id == "rhel"
            || distribution_id == "centos"
            || distribution_id == "rocky"
            || distribution_id == "almalinux"
    }) {
        Some("Install it with: sudo dnf install rsync")
    } else if distribution_ids
        .iter()
        .any(|distribution_id| distribution_id == "nixos")
    {
        Some("Install pkgs.rsync from nixpkgs")
    } else {
        None
    };

    package_manager_hint.unwrap_or("Please install rsync using your package manager")
}

#[derive(Debug, Clone)]
pub enum UpdateStatus {
    Idle,
    Checking,
    Downloading {
        version: AppVersion,
        /// Download progress in `0.0..=1.0`, or `None` when the size is unknown.
        progress: Option<f32>,
    },
    Installing {
        version: AppVersion,
    },
    Updated {
        version: AppVersion,
    },
    Failed {
        error: Arc<anyhow::Error>,
    },
}

impl UpdateStatus {
    pub fn is_updated(&self) -> bool {
        matches!(self, Self::Updated { .. })
    }
}

impl PartialEq for UpdateStatus {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (UpdateStatus::Idle, UpdateStatus::Idle)
            | (UpdateStatus::Checking, UpdateStatus::Checking) => true,
            (
                UpdateStatus::Downloading { version: v1, .. },
                UpdateStatus::Downloading { version: v2, .. },
            )
            | (
                UpdateStatus::Installing { version: v1 },
                UpdateStatus::Installing { version: v2 },
            )
            | (UpdateStatus::Updated { version: v1 }, UpdateStatus::Updated { version: v2 }) => {
                v1 == v2
            }
            (UpdateStatus::Failed { error: error1 }, UpdateStatus::Failed { error: error2 }) => {
                error1.to_string() == error2.to_string()
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseArtifact {
    pub version: AppVersion,
    pub size: u64,
    pub sha256: String,
    pub download_url: String,
}

#[cfg(target_os = "macos")]
struct MacOsUnmounter<'a> {
    mount_path: PathBuf,
    background_executor: &'a BackgroundExecutor,
}

#[cfg(target_os = "macos")]
impl MacOsUnmounter<'_> {
    async fn unmount(mut self) {
        let mount_path = mem::take(&mut self.mount_path);
        unmount_disk_image(&mount_path).await;
    }
}

#[cfg(target_os = "macos")]
impl Drop for MacOsUnmounter<'_> {
    fn drop(&mut self) {
        let mount_path = mem::take(&mut self.mount_path);
        if mount_path.as_os_str().is_empty() {
            return;
        }
        self.background_executor
            .spawn(async move { unmount_disk_image(&mount_path).await })
            .detach();
    }
}

#[cfg(target_os = "macos")]
async fn unmount_disk_image(mount_path: &Path) {
    let unmount_output = util::command::new_command("hdiutil")
        .args(["detach", "-force"])
        .arg(mount_path)
        .output()
        .await;
    match unmount_output {
        Ok(output) if output.status.success() => {
            log::info!("Successfully unmounted the disk image");
        }
        Ok(output) => {
            log::error!(
                "Failed to unmount disk image: {:?}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Err(error) => {
            log::error!("Error while trying to unmount disk image: {error:?}");
        }
    }
}

#[derive(Debug, Clone, Copy, RegisterSetting)]
struct UpdateSettings {
    automatic: bool,
    beta: bool,
}

impl Settings for UpdateSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let update = content.update.as_ref();

        Self {
            automatic: update
                .and_then(|update| update.automatic)
                .expect("update automatic should be defaulted"),
            beta: update
                .and_then(|update| update.beta)
                .expect("update beta should be defaulted"),
        }
    }
}

#[derive(Default)]
struct GlobalUpdater(Option<Entity<Updater>>);

impl Global for GlobalUpdater {}

pub fn init(client: Arc<dyn HttpClient>, cache_dir: PathBuf, cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, window, cx| {
        let Some(window) = window else {
            return;
        };

        let update_version = cx.new(|cx| UpdateVersion::new(window, cx));
        workspace.register_action({
            let update_version = update_version.clone();
            move |_, action, window, cx| {
                update_version.update(cx, |update_version, _| {
                    update_version.start_manual_check();
                });
                check_for_updates(action, window, cx);
            }
        });
        workspace.register_action({
            let update_version = update_version.clone();
            move |_, _: &actions::updater::SimulateUpdateAvailable, _, cx| {
                update_version.update(cx, |update_version, cx| {
                    update_version.update_simulation(cx);
                });
            }
        });
        workspace.status_bar().update(cx, |status_bar, cx| {
            status_bar.add_left_item(update_version, window, cx);
        });
    })
    .detach();

    let version = metadata::version(cx);
    let settings = *UpdateSettings::get_global(cx);
    let should_poll_for_updates =
        !Updater::eligible_channels_for(&version, settings.beta).is_empty();
    let updater = cx.new(|cx| {
        let updater = Updater::new(
            version,
            client,
            cache_dir,
            Arc::new(PlatformReleaseInstaller),
            cx,
        );
        if should_poll_for_updates {
            let mut beta_updates_enabled = settings.beta;
            let mut update_subscription = settings.automatic.then(|| updater.start_polling(cx));

            cx.observe_global::<SettingsStore>(move |updater, cx| {
                let settings = *UpdateSettings::get_global(cx);
                if settings.automatic {
                    if update_subscription.is_none() {
                        update_subscription = Some(updater.start_polling(cx));
                    } else if beta_updates_enabled != settings.beta {
                        updater.poll(UpdateCheckType::Automatic, cx);
                    }
                } else {
                    update_subscription.take();
                }
                beta_updates_enabled = settings.beta;
            })
            .detach();
        }

        updater
    });
    cx.set_global(GlobalUpdater(Some(updater)));
    update_version::notify_if_app_was_updated(cx);
}

pub fn check_for_updates(_: &actions::updater::Check, window: &mut Window, cx: &mut App) {
    if let Some(updater) = Updater::get(cx) {
        let settings = *UpdateSettings::get_global(cx);
        let should_poll_for_updates =
            !Updater::eligible_channels_for(&updater.read(cx).current_version, settings.beta)
                .is_empty();
        if should_poll_for_updates {
            updater.update(cx, |updater, cx| {
                updater.poll(UpdateCheckType::Manual, cx);
            });
        }
    } else {
        log::error!("Cannot check for updates because updater is not initialized");
        let prompt = window.prompt(
            PromptLevel::Warning,
            "Couldn't check for updates",
            Some("Check the logs for details or try again later."),
            &["Open Logs", "OK"],
            cx,
        );
        window
            .spawn(cx, async move |cx| {
                if prompt.await == Ok(0) {
                    cx.update(|window, cx| {
                        window.dispatch_action(Box::new(actions::zaku::OpenLogs), cx);
                    })?;
                }
                anyhow::Ok(())
            })
            .detach_and_log_err(cx);
    }
}

struct InstallerDir(tempfile::TempDir);

impl InstallerDir {
    fn new(cache_dir: &Path) -> anyhow::Result<Self> {
        Ok(Self(
            tempfile::Builder::new()
                .prefix(INSTALLER_DIR_PREFIX)
                .tempdir_in(cache_dir)?,
        ))
    }

    fn path(&self) -> &Path {
        self.0.path()
    }
}

trait ReleaseInstaller: Send + Sync {
    fn install(
        &self,
        installer_dir: InstallerDir,
        target_path: PathBuf,
        cx: &mut AsyncApp,
    ) -> anyhow::Result<Task<anyhow::Result<Option<PathBuf>>>>;
}

struct PlatformReleaseInstaller;

impl ReleaseInstaller for PlatformReleaseInstaller {
    fn install(
        &self,
        installer_dir: InstallerDir,
        target_path: PathBuf,
        cx: &mut AsyncApp,
    ) -> anyhow::Result<Task<anyhow::Result<Option<PathBuf>>>> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        let running_app_path = cx.update(|cx| cx.app_path())?;
        let background_executor = cx.background_executor().clone();
        #[cfg(target_os = "macos")]
        let install_background_executor = background_executor.clone();

        Ok(background_executor.spawn(async move {
            match OS {
                #[cfg(target_os = "linux")]
                "linux" => {
                    install_release_linux(&installer_dir, &target_path, running_app_path).await
                }
                #[cfg(target_os = "macos")]
                "macos" => {
                    install_release_macos(
                        &installer_dir,
                        &target_path,
                        running_app_path,
                        &install_background_executor,
                    )
                    .await
                }
                #[cfg(target_os = "windows")]
                "windows" => {
                    let result = install_release_windows(&target_path).await;
                    drop(installer_dir);
                    result
                }
                unsupported_os => anyhow::bail!("not supported: {unsupported_os}"),
            }
        }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UpdateCheckType {
    Automatic,
    Manual,
}

impl UpdateCheckType {
    pub fn is_manual(self) -> bool {
        self == Self::Manual
    }
}

pub struct Updater {
    status: UpdateStatus,
    current_version: AppVersion,
    client: Arc<dyn HttpClient>,
    cache_dir: PathBuf,
    installer: Arc<dyn ReleaseInstaller>,
    pending_poll: Option<Task<Option<()>>>,
    // Windows cannot replace the running executable, so this keeps the quit callback
    // subscribed to launch the updater helper after Zaku exits. On restart, the
    // subscription is removed because the restart path launches the helper instead.
    #[cfg(target_os = "windows")]
    quit_subscription: Option<Subscription>,
    update_check_type: UpdateCheckType,
    dismissed_status: Option<UpdateStatus>,
}

impl Updater {
    pub fn get(cx: &mut App) -> Option<Entity<Self>> {
        cx.default_global::<GlobalUpdater>().0.clone()
    }

    fn new(
        current_version: AppVersion,
        client: Arc<dyn HttpClient>,
        cache_dir: PathBuf,
        installer: Arc<dyn ReleaseInstaller>,
        #[cfg(any(target_os = "linux", target_os = "macos"))] _: &mut Context<Self>,
        #[cfg(target_os = "windows")] cx: &mut Context<Self>,
    ) -> Self {
        #[cfg(target_os = "windows")]
        let quit_subscription = Some(cx.on_app_quit(|_, _| finalize_update_on_quit()));

        #[cfg(target_os = "windows")]
        cx.on_app_restart(|this, _| {
            this.quit_subscription.take();
        })
        .detach();

        Self {
            status: UpdateStatus::Idle,
            current_version,
            client,
            cache_dir,
            installer,
            pending_poll: None,
            #[cfg(target_os = "windows")]
            quit_subscription,
            update_check_type: UpdateCheckType::Automatic,
            dismissed_status: None,
        }
    }

    pub fn start_polling(&self, cx: &mut Context<Self>) -> Task<anyhow::Result<()>> {
        cx.background_spawn(cleanup_stale_installer_dirs(self.cache_dir.clone()))
            .detach();

        cx.spawn(async move |this, cx| {
            #[cfg(target_os = "windows")]
            if let Err(error) = cleanup_windows().await {
                log::warn!("Failed to clean up old update directories: {error:#}");
            }

            loop {
                this.update(cx, |this, cx| {
                    this.poll(UpdateCheckType::Automatic, cx);
                })?;
                cx.background_executor().timer(POLL_INTERVAL).await;
            }
        })
    }

    pub fn update_check_type(&self) -> UpdateCheckType {
        self.update_check_type
    }

    pub fn poll(&mut self, check_type: UpdateCheckType, cx: &mut Context<Self>) {
        if check_type.is_manual() {
            self.dismissed_status = None;
        }
        if self.pending_poll.is_some() {
            if self.update_check_type == UpdateCheckType::Automatic {
                self.update_check_type = check_type;
                cx.notify();
            }
            return;
        }
        self.update_check_type = check_type;

        cx.notify();

        self.pending_poll = Some(cx.spawn(async move |this, cx| {
            let result = Self::update(this.upgrade()?, cx).await;
            match this.update(cx, |this, cx| {
                this.pending_poll = None;
                if let Err(error) = result {
                    #[cfg(target_os = "linux")]
                    let is_missing_dependency =
                        error.downcast_ref::<MissingDependencyError>().is_some();
                    this.status = match this.update_check_type {
                        #[cfg(target_os = "linux")]
                        UpdateCheckType::Automatic if is_missing_dependency => {
                            log::warn!("Updater: {error}");
                            UpdateStatus::Failed {
                                error: Arc::new(error),
                            }
                        }
                        UpdateCheckType::Automatic => {
                            log::info!("Updater check failed: {error:?}");
                            UpdateStatus::Idle
                        }
                        UpdateCheckType::Manual => {
                            log::error!("Updater failed: {error:?}");
                            UpdateStatus::Failed {
                                error: Arc::new(error),
                            }
                        }
                    };
                    cx.notify();
                }
            }) {
                Ok(()) => Some(()),
                Err(_) => None,
            }
        }));
    }

    pub fn current_version(&self) -> AppVersion {
        self.current_version.clone()
    }

    pub fn status(&self) -> UpdateStatus {
        self.status.clone()
    }

    pub fn dismissed_status(&self) -> Option<UpdateStatus> {
        self.dismissed_status.clone()
    }

    pub fn dismiss_status(&mut self, status: UpdateStatus, cx: &mut Context<Self>) {
        self.dismissed_status = Some(status);
        cx.notify();
    }

    pub fn dismiss(&mut self, cx: &mut Context<Self>) -> bool {
        if let UpdateStatus::Idle = self.status {
            return false;
        }
        self.status = UpdateStatus::Idle;
        cx.notify();
        true
    }

    async fn get_release_artifact(
        this: &Entity<Self>,
        channel: ReleaseChannel,
        os: &str,
        arch: &str,
        cx: &mut AsyncApp,
    ) -> anyhow::Result<Option<ReleaseArtifact>> {
        let client = this.read_with(cx, |this, _| this.client.clone());
        let channel = channel.as_str();
        let url = format!("{ZAKU_SERVER_URL}/releases/{channel}/latest/{os}-{arch}");

        let mut response = client.get(&url, AsyncBody::default(), true).await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }

        let mut body = Vec::new();
        response.body_mut().read_to_end(&mut body).await?;

        anyhow::ensure!(
            response.status().is_success(),
            "failed to fetch release: {:?}",
            String::from_utf8_lossy(&body),
        );

        let release_artifact = serde_json::from_slice(&body).with_context(|| {
            format!(
                "error deserializing release: {}",
                String::from_utf8_lossy(&body),
            )
        })?;
        Ok(Some(release_artifact))
    }

    fn eligible_channels_for(
        version: &AppVersion,
        beta_updates_enabled: bool,
    ) -> &'static [ReleaseChannel] {
        if !version.is_beta() && !version.is_stable() {
            return &[];
        }

        if beta_updates_enabled {
            &[ReleaseChannel::Beta, ReleaseChannel::Stable]
        } else {
            &[ReleaseChannel::Stable]
        }
    }

    async fn update(this: Entity<Self>, cx: &mut AsyncApp) -> anyhow::Result<()> {
        let (client, installed_version, previous_status, installer) =
            this.read_with(cx, |this, _| {
                (
                    this.client.clone(),
                    this.current_version.clone(),
                    this.status.clone(),
                    this.installer.clone(),
                )
            });
        let cache_dir = this.read_with(cx, |this, _| this.cache_dir.clone());
        let current_version = if let UpdateStatus::Updated { version } = &previous_status {
            version
        } else {
            &installed_version
        };
        let beta_updates_enabled = cx.update(|cx| UpdateSettings::get_global(cx).beta);
        let channels = Self::eligible_channels_for(current_version, beta_updates_enabled);
        if channels.is_empty() {
            return Ok(());
        }

        #[cfg(any(target_os = "linux", target_os = "macos"))]
        Self::check_dependencies()?;

        this.update(cx, |this, cx| {
            this.status = UpdateStatus::Checking;
            log::info!("Updater: checking for updates");
            cx.notify();
        });

        let mut release_artifact: Option<ReleaseArtifact> = None;
        for channel in channels {
            let Some(candidate) = Self::get_release_artifact(&this, *channel, OS, ARCH, cx).await?
            else {
                continue;
            };
            if release_artifact
                .as_ref()
                .is_none_or(|release| candidate.version > release.version)
            {
                release_artifact = Some(candidate);
            }
        }
        let release_artifact =
            release_artifact.context("no latest release for eligible update channels")?;
        let newer_version = Self::check_if_fetched_version_is_newer(
            installed_version,
            release_artifact.version.clone(),
            previous_status.clone(),
            beta_updates_enabled,
        )?;

        let Some(newer_version) = newer_version else {
            this.update(cx, |this, cx| {
                let status = match previous_status {
                    UpdateStatus::Updated { .. } => previous_status,
                    _ => UpdateStatus::Idle,
                };
                this.status = status;
                cx.notify();
            });
            return Ok(());
        };

        this.update(cx, |this, cx| {
            this.status = UpdateStatus::Downloading {
                version: newer_version.clone(),
                progress: None,
            };
            cx.notify();
        });

        let installer_dir =
            InstallerDir::new(&cache_dir).context("failed to create installer dir")?;
        let target_path = Self::target_path(&installer_dir)?;
        let progress_entity = this.clone();
        let mut progress_cx = cx.clone();
        download_release(&target_path, release_artifact, client, move |progress| {
            progress_entity.update(&mut progress_cx, |this, cx| {
                if let UpdateStatus::Downloading {
                    progress: current_progress,
                    ..
                } = &mut this.status
                {
                    *current_progress = progress;
                    cx.notify();
                }
            });
        })
        .await
        .with_context(|| format!("failed to download update to {}", target_path.display()))?;

        this.update(cx, |this, cx| {
            this.status = UpdateStatus::Installing {
                version: newer_version.clone(),
            };
            cx.notify();
        });

        let install_result = installer
            .install(installer_dir, target_path.clone(), cx)?
            .await;
        let new_binary_path = install_result
            .with_context(|| format!("failed to install update at: {}", target_path.display()))?;
        if let Some(new_binary_path) = new_binary_path {
            cx.update(|cx| cx.set_restart_path(new_binary_path));
        }

        this.update(cx, |this, cx| {
            this.set_should_show_update_notification(true, cx)
                .detach_and_log_err(cx);
            this.status = UpdateStatus::Updated {
                version: newer_version,
            };
            cx.notify();
        });
        Ok(())
    }

    fn check_if_fetched_version_is_newer(
        installed_version: AppVersion,
        fetched_version: AppVersion,
        status: UpdateStatus,
        beta_updates_enabled: bool,
    ) -> anyhow::Result<Option<AppVersion>> {
        let current_version = if let UpdateStatus::Updated { version } = status {
            version
        } else {
            installed_version
        };
        anyhow::ensure!(
            fetched_version.is_stable() || beta_updates_enabled && fetched_version.is_beta(),
            "{fetched_version} is not an eligible update for Zaku {current_version}",
        );
        Ok(Self::check_if_fetched_version_is_newer_stable(
            &current_version,
            fetched_version,
        ))
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn check_dependencies() -> anyhow::Result<()> {
        #[cfg(target_os = "linux")]
        if which::which("rsync").is_err() {
            let install_hint = linux_rsync_install_hint();
            return Err(MissingDependencyError(format!(
                "rsync is required for auto-updates but is not installed. {install_hint}"
            ))
            .into());
        }

        #[cfg(target_os = "macos")]
        anyhow::ensure!(
            which::which("rsync").is_ok(),
            "could not auto-update because the required rsync utility was not found"
        );

        Ok(())
    }

    fn target_path(installer_dir: &InstallerDir) -> anyhow::Result<PathBuf> {
        let filename = match OS {
            "linux" => "Zaku.tar.gz",
            "macos" => "Zaku.dmg",
            "windows" => "Zaku.exe",
            unsupported_os => anyhow::bail!("not supported: {unsupported_os}"),
        };

        Ok(installer_dir.path().join(filename))
    }

    fn check_if_fetched_version_is_newer_stable(
        installed_version: &AppVersion,
        fetched_version: AppVersion,
    ) -> Option<AppVersion> {
        (fetched_version > *installed_version).then_some(fetched_version)
    }

    pub fn set_should_show_update_notification(
        &self,
        should_show: bool,
        cx: &App,
    ) -> Task<anyhow::Result<()>> {
        let kv_store = KeyValueStore::global(cx);
        cx.background_spawn(async move {
            if should_show {
                kv_store
                    .write_kv(
                        SHOULD_SHOW_UPDATE_NOTIFICATION_KEY.to_string(),
                        String::new(),
                    )
                    .await?;
            } else {
                kv_store
                    .delete_kv(SHOULD_SHOW_UPDATE_NOTIFICATION_KEY.to_string())
                    .await?;
            }
            Ok(())
        })
    }

    pub fn should_show_update_notification(&self, cx: &App) -> Task<anyhow::Result<bool>> {
        let kv_store = KeyValueStore::global(cx);
        cx.background_spawn(async move {
            Ok(kv_store
                .read_kv(SHOULD_SHOW_UPDATE_NOTIFICATION_KEY)?
                .is_some())
        })
    }
}

async fn download_release(
    target_path: &Path,
    release: ReleaseArtifact,
    client: Arc<dyn HttpClient>,
    mut on_progress: impl FnMut(Option<f32>),
) -> anyhow::Result<()> {
    const PERCENTAGE_SCALE: u8 = 100;
    let mut target_file = File::create(target_path).await?;

    let mut response = client
        .get(&release.download_url, AsyncBody::default(), true)
        .await?;
    anyhow::ensure!(
        response.status().is_success(),
        "failed to download update: {:?}",
        response.status()
    );

    let mut downloaded_bytes = 0_u64;
    let mut last_reported_percent = None;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];
    let body = response.body_mut();
    loop {
        let bytes_read = body.read(&mut buffer).await?;
        if bytes_read == 0 {
            break;
        }
        let bytes = buffer
            .get(..bytes_read)
            .context("downloaded byte count exceeded the buffer")?;
        let bytes_read =
            u64::try_from(bytes_read).context("downloaded byte count should fit in u64")?;
        downloaded_bytes += bytes_read;

        hasher.update(bytes);
        target_file.write_all(bytes).await?;

        let percentage_scale = u128::from(PERCENTAGE_SCALE);
        let percent = u128::from(downloaded_bytes) * percentage_scale / u128::from(release.size);
        let percent = percent.min(percentage_scale);
        let percent = u8::try_from(percent).context("download percentage should fit in u8")?;
        if last_reported_percent != Some(percent) {
            last_reported_percent = Some(percent);
            let fraction = f32::from(percent) / f32::from(PERCENTAGE_SCALE);
            on_progress(Some(fraction));
        }
    }
    target_file.flush().await?;

    let sha256 = hex::encode(hasher.finalize());
    anyhow::ensure!(
        sha256.eq_ignore_ascii_case(&release.sha256),
        "downloaded update SHA-256 does not match: expected {}, received {sha256}",
        release.sha256,
    );
    log::info!("Downloaded update to {}", target_path.display());

    Ok(())
}

#[cfg(target_os = "linux")]
async fn install_release_linux(
    installer_dir: &InstallerDir,
    downloaded_tar_gz: &Path,
    running_app_path: PathBuf,
) -> anyhow::Result<Option<PathBuf>> {
    let home_dir =
        PathBuf::from(std::env::var("HOME").context("no HOME environment variable set")?);

    let extracted = installer_dir.path().join("zaku");
    smol::fs::create_dir_all(&extracted)
        .await
        .context("failed to create directory into which to extract update")?;

    let mut command = util::command::new_command("tar");
    command
        .arg("-xzf")
        .arg(downloaded_tar_gz)
        .arg("-C")
        .arg(&extracted);
    let output = command
        .output()
        .await
        .with_context(|| format!("failed to extract: {command:?}"))?;

    anyhow::ensure!(
        output.status.success(),
        "failed to extract {} to {}: {:?}",
        downloaded_tar_gz.display(),
        extracted.display(),
        String::from_utf8_lossy(&output.stderr)
    );

    let app_folder_name = "zaku.app";
    let from = extracted.join(app_folder_name);
    let mut to = home_dir.join(".local");
    let expected_suffix = format!("{app_folder_name}/libexec/zaku");

    if let Some(prefix) = running_app_path
        .to_str()
        .and_then(|path| path.strip_suffix(&expected_suffix))
    {
        to = PathBuf::from(prefix);
    }
    smol::fs::create_dir_all(&to)
        .await
        .with_context(|| format!("failed to create installation prefix {}", to.display()))?;

    let mut command = util::command::new_command("rsync");
    command.args(["-av", "--delete"]).arg(&from).arg(&to);
    let output = command
        .output()
        .await
        .with_context(|| format!("failed to rsync: {command:?}"))?;

    anyhow::ensure!(
        output.status.success(),
        "failed to copy Zaku update from {} to {}: {:?}",
        from.display(),
        to.display(),
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(Some(to.join(expected_suffix)))
}

#[cfg(target_os = "macos")]
async fn install_release_macos(
    temp_dir: &InstallerDir,
    downloaded_dmg: &Path,
    running_app_path: PathBuf,
    background_executor: &BackgroundExecutor,
) -> anyhow::Result<Option<PathBuf>> {
    let running_app_filename = running_app_path
        .file_name()
        .with_context(|| format!("invalid running app path {}", running_app_path.display()))?;

    let mount_path = temp_dir.path().join("Zaku");
    let mut mounted_app_path = mount_path.join(running_app_filename).into_os_string();

    mounted_app_path.push("/");
    let mut command = util::command::new_command("hdiutil");
    command
        .args(["attach", "-nobrowse"])
        .arg(downloaded_dmg)
        .arg("-mountroot")
        .arg(temp_dir.path());
    let output = command
        .output()
        .await
        .with_context(|| format!("failed to mount: {command:?}"))?;

    anyhow::ensure!(
        output.status.success(),
        "failed to mount: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );

    let unmounter = MacOsUnmounter {
        mount_path,
        background_executor,
    };

    let mut command = util::command::new_command("rsync");
    command
        .args(["-av", "--delete", "--exclude", "Icon?"])
        .arg(&mounted_app_path)
        .arg(&running_app_path);
    let rsync_output = command.output().await;

    // Await unmount even if rsync failed so the installer directory can be deleted.
    unmounter.unmount().await;

    let output = rsync_output.with_context(|| format!("failed to rsync: {command:?}"))?;

    anyhow::ensure!(
        output.status.success(),
        "failed to copy app: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(None)
}

#[cfg(target_os = "windows")]
async fn cleanup_windows() -> anyhow::Result<()> {
    let app_dir = std::env::current_exe()?
        .parent()
        .context("no parent directory for Zaku.exe")?
        .to_path_buf();

    for directory in ["updates", "install", "old"] {
        let directory = app_dir.join(directory);
        match smol::fs::remove_dir_all(&directory).await {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to remove update directory {}", directory.display())
                });
            }
        }
    }

    Ok(())
}

#[cfg(target_os = "windows")]
async fn install_release_windows(downloaded_installer: &Path) -> anyhow::Result<Option<PathBuf>> {
    let mut command = util::command::new_command(downloaded_installer);
    command
        .arg("/verysilent")
        .arg("/update=true")
        .arg("/MERGETASKS=!desktopicon")
        .arg("/NORESTART");
    let output = command.output().await?;
    anyhow::ensure!(
        output.status.success(),
        "failed to start installer: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );

    let helper_path = std::env::current_exe()?
        .parent()
        .context("no parent directory for Zaku.exe")?
        .join("tools")
        .join("updater_windows.exe");
    Ok(Some(helper_path))
}

#[cfg(target_os = "windows")]
pub async fn finalize_update_on_quit() {
    let current_exe = match std::env::current_exe() {
        Ok(current_exe) => current_exe,
        Err(error) => {
            log::error!("Failed to locate current executable while finalizing update: {error}");
            return;
        }
    };
    let Some(application_dir) = current_exe.parent() else {
        log::error!("Failed to locate application directory while finalizing update");
        return;
    };
    let versions_path = application_dir.join("updates").join("versions.txt");
    if !versions_path.exists() {
        return;
    }

    let helper_path = application_dir.join("tools").join("updater_windows.exe");
    let mut command = util::command::new_command(helper_path);
    command.args(["--launch", "false"]);
    match command.spawn() {
        Ok(mut child) => {
            if let Err(error) = child.status().await {
                log::error!("Failed to wait for Windows update helper: {error}");
            }
        }
        Err(error) => log::error!("Failed to start Windows update helper: {error}"),
    }
}

async fn cleanup_stale_installer_dirs(cache_dir: PathBuf) {
    const STALE_INSTALLER_DIR_AGE: Duration = Duration::from_hours(24);

    let Ok(mut entries) = smol::fs::read_dir(&cache_dir).await else {
        log::warn!(
            "Failed to read cache directory {} while cleaning up installer directories",
            cache_dir.display()
        );
        return;
    };
    while let Some(entry) = entries.next().await {
        let Ok(entry) = entry else {
            continue;
        };
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with(INSTALLER_DIR_PREFIX)
        {
            continue;
        }

        // A recent directory may belong to an update in another process.
        let is_stale = entry.metadata().await.ok().is_some_and(|metadata| {
            metadata.is_dir()
                && metadata.modified().ok().is_some_and(|modified| {
                    SystemTime::now()
                        .duration_since(modified)
                        .is_ok_and(|age| age > STALE_INSTALLER_DIR_AGE)
                })
        });
        if is_stale {
            let entry_path = entry.path();
            if let Err(error) = smol::fs::remove_dir_all(&entry_path).await {
                log::warn!(
                    "Failed to remove stale installer directory {}: {error}",
                    entry_path.display()
                );
            } else {
                log::info!("Removed stale installer directory {}", entry_path.display());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use futures::channel::{mpsc, oneshot};
    use gpui::{BorrowAppContext, TestAppContext};
    use parking_lot::Mutex;
    use serde_json::json;
    use std::{
        cell::RefCell,
        rc::Rc,
        sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    use tempfile::{TempDir, tempdir};

    use http_client::{FakeHttpClient, Response};

    struct TestReleaseInstaller {
        installed_dir: Arc<TempDir>,
    }

    impl ReleaseInstaller for TestReleaseInstaller {
        fn install(
            &self,
            installer_dir: InstallerDir,
            target_path: PathBuf,
            cx: &mut AsyncApp,
        ) -> anyhow::Result<Task<anyhow::Result<Option<PathBuf>>>> {
            let installed_dir = self.installed_dir.clone();
            let background_executor = cx.background_executor().clone();

            Ok(background_executor.spawn(async move {
                let installed_path = installed_dir.path().join("zaku");
                smol::fs::copy(target_path, &installed_path).await?;
                drop(installer_dir);

                Ok(Some(installed_path))
            }))
        }
    }

    #[gpui::test]
    fn test_updater_settings_defaults(cx: &mut TestAppContext) {
        cx.update(|cx| {
            settings::init(cx);
            let settings = UpdateSettings::get_global(cx);
            assert!(
                settings.automatic,
                "automatic updates should default to true"
            );
            assert!(!settings.beta, "beta updates should default to false");
        });
    }

    #[gpui::test]
    async fn test_updater(cx: &mut TestAppContext) {
        cx.background_executor.allow_parking();

        let release_available = Arc::new(AtomicBool::new(false));
        let (tx, rx) = oneshot::channel::<Vec<u8>>();
        let cache_dir = tempdir().unwrap();
        let installed_dir = Arc::new(tempdir().unwrap());
        let update_contents = b"test-zaku-update".to_vec();
        let update_size = u64::try_from(update_contents.len()).unwrap();
        let update_sha256 = hex::encode(Sha256::digest(&update_contents));

        let (updater, _polling) = cx.update(|cx| {
            settings::init(cx);
            cx.update_global::<SettingsStore, _>(|store, cx| {
                store
                    .set_user_settings(r#"{ "update": { "beta": true } }"#, cx)
                    .result()
                    .unwrap();
            });
            let release_available = Arc::clone(&release_available);
            let rx = Arc::new(Mutex::new(Some(rx)));
            let beta_discovery_path = format!("/releases/beta/latest/{OS}-{ARCH}");
            let stable_discovery_path = format!("/releases/stable/latest/{OS}-{ARCH}");
            let artifact_path = format!("/releases/stable/26.2/{OS}-{ARCH}/download");
            let http_client = FakeHttpClient::create(move |request| {
                let rx = rx.clone();
                let beta_discovery_path = beta_discovery_path.clone();
                let stable_discovery_path = stable_discovery_path.clone();
                let artifact_path = artifact_path.clone();
                let release_available = release_available.load(Ordering::Relaxed);
                let update_sha256 = update_sha256.clone();
                async move {
                    let path = request.uri().path();
                    if path == beta_discovery_path {
                        let version = "26.0-beta.1";
                        let download_url = format!(
                            "https://api.zaku.dev/releases/beta/{version}/{OS}-{ARCH}/download"
                        );
                        Ok(Response::builder()
                            .status(200)
                            .body(
                                json!({
                                    "version": version,
                                    "size": update_size,
                                    "sha256": update_sha256,
                                    "download_url": download_url,
                                })
                                .to_string()
                                .into(),
                            )
                            .unwrap())
                    } else if path == stable_discovery_path {
                        if !release_available {
                            return Ok(Response::builder()
                                .status(StatusCode::NOT_FOUND)
                                .body(AsyncBody::default())
                                .unwrap());
                        }
                        let version = "26.2";
                        let download_url = format!(
                            "https://api.zaku.dev/releases/stable/{version}/{OS}-{ARCH}/download"
                        );
                        Ok(Response::builder()
                            .status(200)
                            .body(
                                json!({
                                    "version": version,
                                    "size": update_size,
                                    "sha256": update_sha256,
                                    "download_url": download_url,
                                })
                                .to_string()
                                .into(),
                            )
                            .unwrap())
                    } else if path == artifact_path {
                        let rx = rx.lock().take().unwrap();
                        Ok(Response::builder()
                            .status(200)
                            .body(rx.await.unwrap().into())
                            .unwrap())
                    } else {
                        panic!("unexpected update request path: {path}");
                    }
                }
            });
            let updater = cx.new(|cx| {
                Updater::new(
                    "26.0-beta.1".parse().unwrap(),
                    http_client,
                    cache_dir.path().to_path_buf(),
                    Arc::new(TestReleaseInstaller {
                        installed_dir: installed_dir.clone(),
                    }),
                    cx,
                )
            });
            let polling = updater.update(cx, |updater, cx| updater.start_polling(cx));

            (updater, polling)
        });
        cx.background_executor.run_until_parked();

        updater.read_with(cx, |updater, _| {
            assert_eq!(updater.status(), UpdateStatus::Idle);
            assert_eq!(
                updater.current_version(),
                "26.0-beta.1".parse::<AppVersion>().unwrap()
            );
        });

        release_available.store(true, Ordering::SeqCst);
        cx.background_executor.advance_clock(POLL_INTERVAL);
        cx.condition(&updater, |updater, _| {
            matches!(updater.status(), UpdateStatus::Downloading { .. })
        })
        .await;

        let status = updater.read_with(cx, |updater, _| updater.status());
        assert!(
            matches!(
                &status,
                UpdateStatus::Downloading {
                    version,
                    progress: None,
                } if version == &"26.2".parse::<AppVersion>().unwrap()
            ),
            "status should be downloading without progress, got {status:?}"
        );

        tx.send(update_contents.clone()).unwrap();

        loop {
            cx.run_until_parked();
            let status = updater.read_with(cx, |updater, _| updater.status());
            if !matches!(
                status,
                UpdateStatus::Downloading { .. } | UpdateStatus::Installing { .. }
            ) {
                break;
            }
        }

        assert_eq!(
            updater.read_with(cx, |updater, _| updater.status()),
            UpdateStatus::Updated {
                version: "26.2".parse().unwrap(),
            }
        );

        let will_restart = cx.expect_restart();
        cx.update(|cx| cx.restart());
        let installed_path = will_restart.await.unwrap().unwrap();
        assert_eq!(installed_path, installed_dir.path().join("zaku"));
        assert_eq!(std::fs::read(installed_path).unwrap(), update_contents);
    }

    #[gpui::test]
    async fn test_updater_watches_automatic_setting(cx: &mut TestAppContext) {
        cx.background_executor.allow_parking();

        let request_count = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = oneshot::channel::<()>();
        let cache_dir = tempdir().unwrap();

        cx.update(|cx| {
            settings::init(cx);
            cx.update_global::<SettingsStore, _>(|store, cx| {
                store
                    .set_user_settings(r#"{ "update": { "automatic": false } }"#, cx)
                    .result()
                    .unwrap();
            });
            metadata::init_test("26.0".parse().unwrap(), cx);

            let rx = Arc::new(Mutex::new(Some(rx)));
            let request_count = Arc::clone(&request_count);
            let discovery_path = format!("/releases/stable/latest/{OS}-{ARCH}");
            let http_client = FakeHttpClient::create(move |request| {
                let rx = rx.clone();
                let discovery_path = discovery_path.clone();
                let request_count = request_count.clone();
                async move {
                    let path = request.uri().path();
                    assert_eq!(path, discovery_path, "update request path should match");
                    request_count.fetch_add(1, Ordering::SeqCst);
                    let rx = rx.lock().take().unwrap();
                    rx.await.unwrap();
                    let download_url =
                        format!("https://api.zaku.dev/releases/stable/26.0/{OS}-{ARCH}/download");
                    Ok(Response::builder()
                        .status(200)
                        .body(
                            json!({
                                "version": "26.0",
                                "size": 1,
                                "sha256": hex::encode(Sha256::digest([0_u8])),
                                "download_url": download_url,
                            })
                            .to_string()
                            .into(),
                        )
                        .unwrap())
                }
            });
            crate::init(http_client, cache_dir.path().to_path_buf(), cx);
        });

        let updater = cx.update(|cx| Updater::get(cx).unwrap());
        cx.background_executor.run_until_parked();
        assert_eq!(
            request_count.load(Ordering::SeqCst),
            0,
            "automatic updates should not poll when disabled"
        );

        cx.update(|cx| {
            cx.update_global::<SettingsStore, _>(|store, cx| {
                store
                    .set_user_settings(r#"{ "update": { "automatic": true } }"#, cx)
                    .result()
                    .unwrap();
            });
        });
        cx.condition(&updater, |updater, _| {
            updater.status() == UpdateStatus::Checking
        })
        .await;
        assert_eq!(
            updater.read_with(cx, |updater, _| updater.status()),
            UpdateStatus::Checking
        );
        assert_eq!(
            request_count.load(Ordering::SeqCst),
            1,
            "enabling automatic updates should poll immediately"
        );

        cx.update(|cx| {
            cx.update_global::<SettingsStore, _>(|store, cx| {
                store
                    .set_user_settings(r#"{ "update": { "automatic": false } }"#, cx)
                    .result()
                    .unwrap();
            });
        });
        cx.run_until_parked();
        tx.send(()).unwrap();

        loop {
            cx.run_until_parked();
            let status = updater.read_with(cx, |updater, _| updater.status());
            if !matches!(status, UpdateStatus::Checking) {
                break;
            }
        }
        assert_eq!(
            updater.read_with(cx, |updater, _| updater.status()),
            UpdateStatus::Idle,
            "disabling automatic updates should not cancel an active check"
        );

        cx.background_executor.advance_clock(POLL_INTERVAL);
        cx.background_executor.run_until_parked();
        assert_eq!(
            request_count.load(Ordering::SeqCst),
            1,
            "automatic updates should stop polling when disabled"
        );
    }

    #[gpui::test]
    async fn test_updater_watches_beta_setting(cx: &mut TestAppContext) {
        cx.background_executor.allow_parking();

        let (tx, mut rx) = mpsc::unbounded();
        let cache_dir = tempdir().unwrap();

        let updater = cx.update(|cx| {
            settings::init(cx);
            metadata::init_test("26.0".parse().unwrap(), cx);

            let beta_discovery_path = format!("/releases/beta/latest/{OS}-{ARCH}");
            let stable_discovery_path = format!("/releases/stable/latest/{OS}-{ARCH}");
            let http_client = FakeHttpClient::create(move |request| {
                let tx = tx.clone();
                let beta_discovery_path = beta_discovery_path.clone();
                let stable_discovery_path = stable_discovery_path.clone();
                async move {
                    let path = request.uri().path().to_string();
                    tx.unbounded_send(path.clone()).unwrap();
                    let (channel, version) = if path == beta_discovery_path {
                        ("beta", "26.0-beta.1")
                    } else if path == stable_discovery_path {
                        ("stable", "26.0")
                    } else {
                        panic!("unexpected update request path: {path}");
                    };
                    let download_url = format!(
                        "https://api.zaku.dev/releases/{channel}/{version}/{OS}-{ARCH}/download"
                    );
                    Ok(Response::builder()
                        .status(200)
                        .body(
                            json!({
                                "version": version,
                                "size": 1,
                                "sha256": hex::encode(Sha256::digest([0_u8])),
                                "download_url": download_url,
                            })
                            .to_string()
                            .into(),
                        )
                        .unwrap())
                }
            });
            crate::init(http_client, cache_dir.path().to_path_buf(), cx);
            Updater::get(cx).unwrap()
        });

        assert_eq!(
            futures::StreamExt::next(&mut rx).await.unwrap(),
            format!("/releases/stable/latest/{OS}-{ARCH}"),
            "default update settings should check only stable releases"
        );
        cx.condition(&updater, |updater, _| {
            matches!(updater.status(), UpdateStatus::Idle)
        })
        .await;

        cx.update(|cx| {
            cx.update_global::<SettingsStore, _>(|store, cx| {
                store
                    .set_user_settings(r#"{ "update": { "beta": true } }"#, cx)
                    .result()
                    .unwrap();
            });
        });

        assert_eq!(
            futures::StreamExt::next(&mut rx).await.unwrap(),
            format!("/releases/beta/latest/{OS}-{ARCH}"),
            "enabling beta updates should immediately check beta releases"
        );
        assert_eq!(
            futures::StreamExt::next(&mut rx).await.unwrap(),
            format!("/releases/stable/latest/{OS}-{ARCH}"),
            "enabling beta updates should immediately check beta and stable releases"
        );
        cx.condition(&updater, |updater, _| {
            matches!(updater.status(), UpdateStatus::Idle)
        })
        .await;

        cx.update(|cx| {
            cx.update_global::<SettingsStore, _>(|store, cx| {
                store
                    .set_user_settings(r#"{ "update": { "beta": false } }"#, cx)
                    .result()
                    .unwrap();
            });
        });

        assert_eq!(
            futures::StreamExt::next(&mut rx).await.unwrap(),
            format!("/releases/stable/latest/{OS}-{ARCH}"),
            "disabling beta updates should immediately check only stable releases"
        );
        cx.condition(&updater, |updater, _| {
            matches!(updater.status(), UpdateStatus::Idle)
        })
        .await;
    }

    #[test]
    fn test_eligible_channels_for_version() {
        for (version, beta_updates_enabled, expected_channels) in [
            (
                "26.0-beta.1",
                true,
                &[ReleaseChannel::Beta, ReleaseChannel::Stable][..],
            ),
            ("26.1", false, &[ReleaseChannel::Stable][..]),
            (
                "26.1",
                true,
                &[ReleaseChannel::Beta, ReleaseChannel::Stable][..],
            ),
            ("26.1-beta.1", false, &[ReleaseChannel::Stable][..]),
            ("26.1-nightly.2026-08-02", true, &[][..]),
            ("26.1-dev.1000.aaaaaaaa", false, &[][..]),
        ] {
            let version = version.parse().unwrap();
            assert_eq!(
                Updater::eligible_channels_for(&version, beta_updates_enabled),
                expected_channels
            );
        }
    }

    #[test]
    fn test_fetched_version_selection() {
        for (
            installed_version,
            fetched_version,
            updated_version,
            beta_updates_enabled,
            expected_version,
        ) in [
            (
                "26.0-beta.1",
                "26.0-beta.2",
                None,
                true,
                Some("26.0-beta.2"),
            ),
            ("26.0-beta.1", "26.0", None, true, Some("26.0")),
            ("26.1", "26.2-beta.1", None, true, Some("26.2-beta.1")),
            ("26.1", "26.0", None, false, None),
            ("26.1", "26.1", None, false, None),
            ("26.0", "26.1", None, false, Some("26.1")),
            ("26.0", "26.1", Some("26.1"), false, None),
            ("26.0", "26.1.1", Some("26.1"), false, Some("26.1.1")),
        ] {
            let status = match updated_version {
                Some(version) => UpdateStatus::Updated {
                    version: version.parse().unwrap(),
                },
                None => UpdateStatus::Idle,
            };
            let selected_version = Updater::check_if_fetched_version_is_newer(
                installed_version.parse().unwrap(),
                fetched_version.parse().unwrap(),
                status,
                beta_updates_enabled,
            );
            let expected_version =
                expected_version.map(|version| version.parse::<AppVersion>().unwrap());

            assert_eq!(selected_version.unwrap(), expected_version);
        }
    }

    #[test]
    fn test_fetched_prerelease_rejection() {
        for fetched_version in [
            "26.1-beta.1",
            "26.1-nightly.2026-07-19",
            "26.1-dev.1000.aaaaaaaa",
        ] {
            Updater::check_if_fetched_version_is_newer(
                "26.0".parse().unwrap(),
                fetched_version.parse().unwrap(),
                UpdateStatus::Idle,
                false,
            )
            .unwrap_err();
        }
    }

    #[gpui::test]
    async fn test_download_release_reports_progress(cx: &mut TestAppContext) {
        cx.background_executor.allow_parking();

        let body = vec![0_u8; 20_000];
        let release = ReleaseArtifact {
            version: "26.1".parse().unwrap(),
            size: u64::try_from(body.len()).unwrap(),
            sha256: hex::encode(Sha256::digest(&body)),
            download_url: format!("{ZAKU_SERVER_URL}/releases/stable/26.1/{OS}-{ARCH}/download"),
        };
        let expected_size = release.size;
        let http_client = FakeHttpClient::create(move |_| {
            let body = body.clone();
            async move { Ok(Response::builder().status(200).body(body.into()).unwrap()) }
        });
        let temp_dir = tempdir().unwrap();
        let target_path = temp_dir.path().join("zaku-download");
        let reported = Rc::new(RefCell::new(Vec::new()));

        download_release(&target_path, release, http_client, {
            let reported = reported.clone();
            move |fraction| {
                if let Some(fraction) = fraction {
                    reported.borrow_mut().push(fraction);
                }
            }
        })
        .await
        .unwrap();

        let reported = reported.borrow();
        assert!(
            reported.len() >= 2,
            "progress should be reported across multiple reads, got {reported:?}"
        );
        assert_eq!(
            reported.last().copied(),
            Some(1.0),
            "download should finish at 100%"
        );
        for fraction in reported.iter() {
            assert!(
                (0.0..=1.0).contains(fraction),
                "progress should be within range: {fraction}"
            );
        }
        for pair in reported.windows(2) {
            assert!(pair[0] <= pair[1], "progress should not decrease");
        }

        let downloaded_length = std::fs::metadata(&target_path).unwrap().len();
        assert_eq!(
            downloaded_length, expected_size,
            "file size should match release metadata"
        );
    }

    #[gpui::test]
    async fn test_download_release_rejects_sha256_mismatch(cx: &mut TestAppContext) {
        cx.background_executor.allow_parking();

        let body = b"test-zaku-update".to_vec();
        let release = ReleaseArtifact {
            version: "26.1".parse().unwrap(),
            size: u64::try_from(body.len()).unwrap(),
            sha256: hex::encode(Sha256::digest(b"different-update")),
            download_url: format!("{ZAKU_SERVER_URL}/releases/stable/26.1/{OS}-{ARCH}/download"),
        };
        let http_client = FakeHttpClient::create(move |_| {
            let body = body.clone();
            async move { Ok(Response::builder().status(200).body(body.into()).unwrap()) }
        });
        let temp_dir = tempdir().unwrap();
        let target_path = temp_dir.path().join("zaku-download");

        let error = download_release(&target_path, release, http_client, |_| {})
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .starts_with("downloaded update SHA-256 does not match"),
            "unexpected error: {error:#}"
        );
    }
}
