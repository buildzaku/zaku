mod update_version;

use anyhow::Context as _;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use futures::StreamExt;
use futures::{AsyncReadExt, AsyncWriteExt};
#[cfg(target_os = "windows")]
use gpui::Subscription;
use gpui::{
    App, AppContext, AsyncApp, BackgroundExecutor, Context, Entity, Global, PromptLevel, Task,
    TaskExt, Window,
};
use semver::Version;
use serde::{Deserialize, Serialize};
use smol::fs::File;
#[cfg(target_os = "windows")]
use std::io;
#[cfg(target_os = "macos")]
use std::mem;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::time::SystemTime;
use std::{
    env::consts::{ARCH, OS},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
#[cfg(target_os = "linux")]
use std::{error, fmt};

use db::kv::KeyValueStore;
use http_client::{AsyncBody, HttpClient};
use metadata::{AppVersion, ZAKU_SERVER_URL};
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
#[cfg(any(target_os = "linux", target_os = "macos"))]
const INSTALLER_DIR_PREFIX: &str = "zaku-updater";

#[cfg(target_os = "linux")]
fn linux_rsync_install_hint() -> &'static str {
    let os_release = match std::fs::read_to_string("/etc/os-release") {
        Ok(os_release) => os_release,
        Err(_) => return "Please install rsync using your package manager",
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
        version: Version,
        /// Download progress in `0.0..=1.0`, or `None` when the size is unknown.
        progress: Option<f32>,
    },
    Installing {
        version: Version,
    },
    Updated {
        version: Version,
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
pub struct ReleaseAsset {
    pub version: String,
    pub url: String,
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
}

impl Settings for UpdateSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let update = content.update.as_ref();

        Self {
            automatic: update
                .and_then(|update| update.automatic)
                .expect("update automatic should be defaulted"),
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

    let version = AppVersion::global(cx);
    let updater = cx.new(|cx| {
        let updater = Updater::new(version, client, cache_dir, cx);
        let mut update_subscription = UpdateSettings::get_global(cx)
            .automatic
            .then(|| updater.start_polling(cx));

        cx.observe_global::<SettingsStore>(move |updater, cx| {
            if UpdateSettings::get_global(cx).automatic {
                if update_subscription.is_none() {
                    update_subscription = Some(updater.start_polling(cx));
                }
            } else {
                update_subscription.take();
            }
        })
        .detach();

        updater
    });
    cx.set_global(GlobalUpdater(Some(updater)));
    update_version::notify_if_app_was_updated(cx);
}

pub fn check_for_updates(_: &actions::updater::Check, window: &mut Window, cx: &mut App) {
    if let Some(updater) = Updater::get(cx) {
        updater.update(cx, |updater, cx| {
            updater.poll(UpdateCheckType::Manual, cx);
        });
    } else {
        drop(window.prompt(
            PromptLevel::Warning,
            "Couldn't check for updates",
            Some("Zaku couldn't check for updates. Check your internet connection and try again."),
            &["OK"],
            cx,
        ));
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
struct InstallerDir(tempfile::TempDir);

#[cfg(any(target_os = "linux", target_os = "macos"))]
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

#[cfg(target_os = "windows")]
struct InstallerDir(PathBuf);

#[cfg(target_os = "windows")]
impl InstallerDir {
    async fn new() -> anyhow::Result<Self> {
        let installer_dir = std::env::current_exe()?
            .parent()
            .context("no parent directory for Zaku.exe")?
            .join("updates");
        match smol::fs::metadata(&installer_dir).await {
            Ok(_) => smol::fs::remove_dir_all(&installer_dir).await?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to read update directory metadata {}",
                        installer_dir.display()
                    )
                });
            }
        }
        smol::fs::create_dir(&installer_dir).await?;
        Ok(Self(installer_dir))
    }

    fn path(&self) -> &Path {
        self.0.as_path()
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
    current_version: Version,
    client: Arc<dyn HttpClient>,
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    cache_dir: PathBuf,
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
        current_version: Version,
        client: Arc<dyn HttpClient>,
        #[cfg(any(target_os = "linux", target_os = "macos"))] cache_dir: PathBuf,
        #[cfg(target_os = "windows")] _: PathBuf,
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
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            cache_dir,
            pending_poll: None,
            #[cfg(target_os = "windows")]
            quit_subscription,
            update_check_type: UpdateCheckType::Automatic,
            dismissed_status: None,
        }
    }

    pub fn start_polling(&self, cx: &mut Context<Self>) -> Task<anyhow::Result<()>> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
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

    pub fn current_version(&self) -> Version {
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

    async fn get_release_asset(
        this: &Entity<Self>,
        os: &str,
        arch: &str,
        cx: &mut AsyncApp,
    ) -> anyhow::Result<ReleaseAsset> {
        let client = this.read_with(cx, |this, _| this.client.clone());
        let url = format!("{ZAKU_SERVER_URL}/releases/stable/latest/{os}-{arch}");

        let mut response = client.get(&url, AsyncBody::default(), true).await?;
        let mut body = Vec::new();
        response.body_mut().read_to_end(&mut body).await?;

        anyhow::ensure!(
            response.status().is_success(),
            "failed to fetch release: {:?}",
            String::from_utf8_lossy(&body),
        );

        serde_json::from_slice(body.as_slice()).with_context(|| {
            format!(
                "error deserializing release {:?}",
                String::from_utf8_lossy(&body),
            )
        })
    }

    async fn update(this: Entity<Self>, cx: &mut AsyncApp) -> anyhow::Result<()> {
        Self::check_dependencies()?;

        let (client, installed_version, previous_status) = this.read_with(cx, |this, _| {
            (
                this.client.clone(),
                this.current_version.clone(),
                this.status.clone(),
            )
        });
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        let cache_dir = this.read_with(cx, |this, _| this.cache_dir.clone());

        this.update(cx, |this, cx| {
            this.status = UpdateStatus::Checking;
            log::info!("Updater: checking for updates");
            cx.notify();
        });

        let fetched_release_data = Self::get_release_asset(&this, OS, ARCH, cx).await?;
        let newer_version = Self::check_if_fetched_version_is_newer(
            installed_version,
            &fetched_release_data.version,
            previous_status.clone(),
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

        #[cfg(any(target_os = "linux", target_os = "macos"))]
        let installer_dir =
            InstallerDir::new(&cache_dir).context("failed to create installer dir")?;
        #[cfg(target_os = "windows")]
        let installer_dir = InstallerDir::new()
            .await
            .context("failed to create installer dir")?;
        let target_path = Self::target_path(&installer_dir)?;
        let progress_entity = this.clone();
        let mut progress_cx = cx.clone();
        download_release(
            &target_path,
            fetched_release_data,
            client,
            move |progress| {
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
            },
        )
        .await
        .with_context(|| format!("failed to download update to {}", target_path.display()))?;

        this.update(cx, |this, cx| {
            this.status = UpdateStatus::Installing {
                version: newer_version.clone(),
            };
            cx.notify();
        });

        #[cfg(test)]
        let Some(install_result) = cx
            .try_read_global::<tests::InstallOverride, _>(|global, _| global.0.clone())
            .map(|test_install| test_install(&target_path, cx))
        else {
            return Ok(());
        };

        #[cfg(not(test))]
        let install_result = {
            let running_app_path = cx.update(|cx| cx.app_path())?;
            let background_executor = cx.background_executor().clone();
            cx.background_spawn(Self::install_release(
                installer_dir,
                target_path.clone(),
                running_app_path,
                background_executor,
            ))
            .await
        };
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
        installed_version: Version,
        fetched_version: &str,
        status: UpdateStatus,
    ) -> anyhow::Result<Option<Version>> {
        let fetched_version = fetched_version
            .parse::<Version>()
            .context("failed to parse stable release version")?;
        anyhow::ensure!(
            fetched_version.pre == semver::Prerelease::EMPTY
                && fetched_version.build == semver::BuildMetadata::EMPTY,
            "stable release version must not contain prerelease or build metadata"
        );

        let current_version = if let UpdateStatus::Updated { version } = status {
            version
        } else {
            installed_version
        };
        Ok(Self::check_if_fetched_version_is_newer_stable(
            current_version,
            fetched_version,
        ))
    }

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

    async fn install_release(
        #[cfg(any(target_os = "linux", target_os = "macos"))] installer_dir: InstallerDir,
        #[cfg(target_os = "windows")] _: InstallerDir,
        target_path: PathBuf,
        #[cfg(any(target_os = "linux", target_os = "macos"))] running_app_path: PathBuf,
        #[cfg(target_os = "windows")] _: PathBuf,
        #[cfg(any(target_os = "linux", target_os = "windows"))] _: BackgroundExecutor,
        #[cfg(target_os = "macos")] background_executor: BackgroundExecutor,
    ) -> anyhow::Result<Option<PathBuf>> {
        match OS {
            #[cfg(target_os = "linux")]
            "linux" => install_release_linux(&installer_dir, &target_path, running_app_path).await,
            #[cfg(target_os = "macos")]
            "macos" => {
                install_release_macos(
                    &installer_dir,
                    &target_path,
                    running_app_path,
                    &background_executor,
                )
                .await
            }
            #[cfg(target_os = "windows")]
            "windows" => install_release_windows(&target_path).await,
            unsupported_os => anyhow::bail!("not supported: {unsupported_os}"),
        }
    }

    fn check_if_fetched_version_is_newer_stable(
        mut installed_version: Version,
        fetched_version: Version,
    ) -> Option<Version> {
        installed_version.pre = semver::Prerelease::EMPTY;
        installed_version.build = semver::BuildMetadata::EMPTY;
        (fetched_version > installed_version).then_some(fetched_version)
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
    release: ReleaseAsset,
    client: Arc<dyn HttpClient>,
    mut on_progress: impl FnMut(Option<f32>),
) -> anyhow::Result<()> {
    const PERCENTAGE_SCALE: u8 = 100;
    let mut target_file = File::create(target_path).await?;

    let mut response = client.get(&release.url, AsyncBody::default(), true).await?;
    anyhow::ensure!(
        response.status().is_success(),
        "failed to download update: {:?}",
        response.status()
    );

    let total_bytes = response
        .headers()
        .get(http_client::http::header::CONTENT_LENGTH)
        .map(|value| -> anyhow::Result<u64> {
            value
                .to_str()
                .context("content length should be valid text")?
                .parse::<u64>()
                .context("content length should be a valid integer")
        })
        .transpose();
    let total_bytes = match total_bytes {
        Ok(total_bytes) => total_bytes.filter(|total_bytes| *total_bytes > 0),
        Err(error) => {
            log::warn!("Failed to read update content length: {error:#}");
            None
        }
    };

    let mut downloaded_bytes = 0_u64;
    let mut last_reported_percent = None;
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
        target_file.write_all(bytes).await?;
        downloaded_bytes += bytes_read as u64;

        if let Some(total_bytes) = total_bytes {
            let percentage_scale = u128::from(PERCENTAGE_SCALE);
            let percent = u128::from(downloaded_bytes) * percentage_scale / u128::from(total_bytes);
            let percent = percent.min(percentage_scale);
            let percent = u8::try_from(percent).context("download percentage should fit in u8")?;
            if last_reported_percent != Some(percent) {
                last_reported_percent = Some(percent);
                let fraction = f32::from(percent) / f32::from(PERCENTAGE_SCALE);
                on_progress(Some(fraction));
            }
        }
    }
    target_file.flush().await?;
    if total_bytes.is_some() && last_reported_percent != Some(PERCENTAGE_SCALE) {
        on_progress(Some(1.0));
    }
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

#[cfg(any(target_os = "linux", target_os = "macos"))]
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

    use futures::channel::oneshot;
    use gpui::{BorrowAppContext, TestAppContext};
    use parking_lot::Mutex;
    use serde_json::json;
    use std::{
        cell::RefCell,
        rc::Rc,
        sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    use tempfile::tempdir;

    use http_client::{FakeHttpClient, Response};

    pub(super) struct InstallOverride(
        pub Rc<dyn Fn(&Path, &AsyncApp) -> anyhow::Result<Option<PathBuf>>>,
    );

    impl Global for InstallOverride {}

    #[gpui::test]
    fn test_updater_defaults_to_true(cx: &mut TestAppContext) {
        cx.update(|cx| {
            settings::init(cx);
            assert!(
                UpdateSettings::get_global(cx).automatic,
                "automatic updates should default to true"
            );
        });
    }

    #[gpui::test]
    async fn test_updater(cx: &mut TestAppContext) {
        cx.background_executor.allow_parking();

        let release_available = Arc::new(AtomicBool::new(false));
        let (download_tx, download_rx) = oneshot::channel::<Vec<u8>>();
        let cache_dir = tempdir().unwrap();

        cx.update(|cx| {
            settings::init(cx);
            metadata::init_test(Version::new(26, 0, 0), cx);

            let release_available = Arc::clone(&release_available);
            let download_rx = Arc::new(Mutex::new(Some(download_rx)));
            let discovery_path = format!("/releases/stable/latest/{OS}-{ARCH}");
            let artifact_extension = match OS {
                "linux" => "tar.gz",
                "macos" => "dmg",
                "windows" => "exe",
                unsupported_os => panic!("not supported: {unsupported_os}"),
            };
            let artifact_path = format!(
                "/releases/stable/26.1.0/{OS}-{ARCH}/Zaku-26.1.0-{ARCH}.{artifact_extension}"
            );
            let http_client = FakeHttpClient::create(move |request| {
                let download_rx = download_rx.clone();
                let discovery_path = discovery_path.clone();
                let artifact_path = artifact_path.clone();
                let release_available = release_available.load(Ordering::Relaxed);
                async move {
                    let path = request.uri().path();
                    if path == discovery_path {
                        let version = if release_available {
                            "26.1.0"
                        } else {
                            "26.0.0"
                        };
                        let url = format!(
                            "{ZAKU_SERVER_URL}/releases/stable/{version}/{OS}-{ARCH}/Zaku-{version}-{ARCH}.{artifact_extension}"
                        );
                        Ok(Response::builder()
                            .status(200)
                            .body(json!({ "version": version, "url": url }).to_string().into())
                            .unwrap())
                    } else if path == artifact_path {
                        let download_rx = download_rx.lock().take().unwrap();
                        Ok(Response::builder()
                            .status(200)
                            .body(download_rx.await.unwrap().into())
                            .unwrap())
                    } else {
                        panic!("unexpected update request path: {path}");
                    }
                }
            });
            crate::init(http_client, cache_dir.path().to_path_buf(), cx);
        });

        let updater = cx.update(|cx| Updater::get(cx).unwrap());
        cx.background_executor.run_until_parked();

        updater.read_with(cx, |updater, _| {
            assert_eq!(updater.status(), UpdateStatus::Idle);
            assert_eq!(updater.current_version(), Version::new(26, 0, 0));
        });

        release_available.store(true, Ordering::SeqCst);
        cx.background_executor.advance_clock(POLL_INTERVAL);
        cx.background_executor.run_until_parked();

        let status = updater.read_with(cx, |updater, _| updater.status());
        assert!(
            matches!(
                &status,
                UpdateStatus::Downloading {
                    version,
                    progress: None,
                } if version == &Version::new(26, 1, 0)
            ),
            "status should be downloading without progress, got {status:?}"
        );

        let installed_dir = Arc::new(tempdir().unwrap());
        cx.update(|cx| {
            cx.set_global(InstallOverride(Rc::new({
                let installed_dir = installed_dir.clone();
                move |target_path, _| {
                    let installed_path = installed_dir.path().join("zaku");
                    std::fs::copy(target_path, &installed_path)?;
                    Ok(Some(installed_path))
                }
            })));
        });

        let update_contents = b"fake-zaku-update".to_vec();
        download_tx.send(update_contents.clone()).unwrap();

        loop {
            cx.run_until_parked();
            let status = updater.read_with(cx, |updater, _| updater.status());
            if !matches!(status, UpdateStatus::Downloading { .. }) {
                break;
            }
        }

        assert_eq!(
            updater.read_with(cx, |updater, _| updater.status()),
            UpdateStatus::Updated {
                version: Version::new(26, 1, 0),
            }
        );

        let will_restart = cx.expect_restart();
        cx.update(|cx| cx.restart());
        let installed_path = will_restart.await.unwrap().unwrap();
        assert_eq!(installed_path, installed_dir.path().join("zaku"));
        assert_eq!(std::fs::read(installed_path).unwrap(), update_contents);
    }

    #[gpui::test]
    fn test_updater_watches_user_setting(cx: &mut TestAppContext) {
        cx.background_executor.allow_parking();

        let request_count = Arc::new(AtomicUsize::new(0));
        let (release_tx, release_rx) = oneshot::channel::<()>();
        let cache_dir = tempdir().unwrap();

        cx.update(|cx| {
            settings::init(cx);
            cx.update_global::<SettingsStore, _>(|store, cx| {
                store
                    .set_user_settings(r#"{ "update": { "automatic": false } }"#, cx)
                    .result()
                    .unwrap();
            });
            metadata::init_test(Version::new(26, 0, 0), cx);

            let release_rx = Arc::new(Mutex::new(Some(release_rx)));
            let request_count = Arc::clone(&request_count);
            let discovery_path = format!("/releases/stable/latest/{OS}-{ARCH}");
            let artifact_extension = match OS {
                "linux" => "tar.gz",
                "macos" => "dmg",
                "windows" => "exe",
                unsupported_os => panic!("not supported: {unsupported_os}"),
            };
            let http_client = FakeHttpClient::create(move |request| {
                let release_rx = release_rx.clone();
                let discovery_path = discovery_path.clone();
                let request_count = request_count.clone();
                async move {
                    let path = request.uri().path();
                    assert_eq!(path, discovery_path, "update request path should match");
                    request_count.fetch_add(1, Ordering::SeqCst);
                    let release_rx = release_rx.lock().take().unwrap();
                    release_rx.await.unwrap();
                    let url = format!(
                        "{ZAKU_SERVER_URL}/releases/stable/26.0.0/{OS}-{ARCH}/Zaku-26.0.0-{ARCH}.{artifact_extension}"
                    );
                    Ok(Response::builder()
                        .status(200)
                        .body(
                            json!({ "version": "26.0.0", "url": url })
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
        cx.background_executor.run_until_parked();
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
        release_tx.send(()).unwrap();

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

    #[test]
    fn test_stable_does_not_update_when_fetched_version_is_not_higher() {
        let installed_version = Version::new(26, 0, 0);

        for fetched_version in ["25.9.9", "26.0.0"] {
            let newer_version = Updater::check_if_fetched_version_is_newer(
                installed_version.clone(),
                fetched_version,
                UpdateStatus::Idle,
            );

            assert_eq!(newer_version.unwrap(), None);
        }
    }

    #[test]
    fn test_stable_does_update_when_fetched_version_is_higher() {
        let installed_version = Version::new(26, 0, 0);
        let fetched_version = Version::new(26, 1, 0);

        let newer_version = Updater::check_if_fetched_version_is_newer(
            installed_version,
            &fetched_version.to_string(),
            UpdateStatus::Idle,
        );

        assert_eq!(newer_version.unwrap(), Some(fetched_version));
    }

    #[test]
    fn test_stable_does_not_update_when_fetched_version_is_not_higher_than_cached() {
        let installed_version = Version::new(26, 0, 0);
        let status = UpdateStatus::Updated {
            version: Version::new(26, 1, 0),
        };
        let fetched_version = Version::new(26, 1, 0);

        let newer_version = Updater::check_if_fetched_version_is_newer(
            installed_version,
            &fetched_version.to_string(),
            status,
        );

        assert_eq!(newer_version.unwrap(), None);
    }

    #[test]
    fn test_stable_does_update_when_fetched_version_is_higher_than_cached() {
        let installed_version = Version::new(26, 0, 0);
        let status = UpdateStatus::Updated {
            version: Version::new(26, 1, 0),
        };
        let fetched_version = Version::new(26, 1, 1);

        let newer_version = Updater::check_if_fetched_version_is_newer(
            installed_version,
            &fetched_version.to_string(),
            status,
        );

        assert_eq!(newer_version.unwrap(), Some(fetched_version));
    }

    #[gpui::test]
    async fn test_download_release_reports_progress(cx: &mut TestAppContext) {
        cx.background_executor.allow_parking();

        let body = vec![0_u8; 20_000];
        let content_length = body.len();
        let http_client = FakeHttpClient::create(move |_| {
            let body = body.clone();
            async move {
                Ok(Response::builder()
                    .status(200)
                    .header(
                        http_client::http::header::CONTENT_LENGTH,
                        body.len().to_string(),
                    )
                    .body(body.into())
                    .unwrap())
            }
        });
        let temp_dir = tempdir().unwrap();
        let target_path = temp_dir.path().join("zaku-download");
        let artifact_extension = match OS {
            "linux" => "tar.gz",
            "macos" => "dmg",
            "windows" => "exe",
            unsupported_os => panic!("not supported: {unsupported_os}"),
        };
        let release = ReleaseAsset {
            version: "26.1.0".to_string(),
            url: format!(
                "{ZAKU_SERVER_URL}/releases/stable/26.1.0/{OS}-{ARCH}/Zaku-26.1.0-{ARCH}.{artifact_extension}"
            ),
        };
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
            downloaded_length, content_length as u64,
            "file size should match response body"
        );
    }

    #[gpui::test]
    async fn test_download_release_without_content_length_reports_no_progress(
        cx: &mut TestAppContext,
    ) {
        cx.background_executor.allow_parking();

        let body = vec![0_u8; 20_000];
        let content_length = body.len();
        let http_client = FakeHttpClient::create(move |_| {
            let body = body.clone();
            async move { Ok(Response::builder().status(200).body(body.into()).unwrap()) }
        });
        let temp_dir = tempdir().unwrap();
        let target_path = temp_dir.path().join("zaku-download");
        let artifact_extension = match OS {
            "linux" => "tar.gz",
            "macos" => "dmg",
            "windows" => "exe",
            unsupported_os => panic!("not supported: {unsupported_os}"),
        };
        let release = ReleaseAsset {
            version: "26.1.0".to_string(),
            url: format!(
                "{ZAKU_SERVER_URL}/releases/stable/26.1.0/{OS}-{ARCH}/Zaku-26.1.0-{ARCH}.{artifact_extension}"
            ),
        };
        let reported = Rc::new(RefCell::new(Vec::new()));

        download_release(&target_path, release, http_client, {
            let reported = reported.clone();
            move |fraction| {
                reported.borrow_mut().push(fraction);
            }
        })
        .await
        .unwrap();

        assert!(
            reported.borrow().is_empty(),
            "progress should not be reported without content length, got {:?}",
            reported.borrow()
        );
        let downloaded_length = std::fs::metadata(&target_path).unwrap().len();
        assert_eq!(
            downloaded_length, content_length as u64,
            "file size should match response body"
        );
    }
}
