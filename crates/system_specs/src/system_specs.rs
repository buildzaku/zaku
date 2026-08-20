pub use gpui::GpuSpecs;

#[cfg(target_os = "linux")]
use anyhow::Context as _;
use gpui::{App, AppContext, Task, Window};
use serde::{Deserialize, Serialize};
#[cfg(target_os = "linux")]
use std::path::Path;
use std::{
    env,
    fmt::{self, Display},
};
use sysinfo::{MemoryRefreshKind, RefreshKind, System};

use metadata::{ZAKU_COMMIT_SHA, ZAKU_NAME};

#[derive(Debug, Clone)]
pub struct SystemSpecs {
    app_version: String,
    os_name: String,
    os_version: String,
    memory: u64,
    arch: &'static str,
    commit_sha: String,
    gpu_specs: Option<String>,
}

impl SystemSpecs {
    pub fn new(
        window: &mut Window,
        cx: &mut App,
        os_name: String,
        os_version: String,
    ) -> Task<Self> {
        let app_version = metadata::version(cx).to_string();
        let system = System::new_with_specifics(
            RefreshKind::nothing().with_memory(MemoryRefreshKind::everything()),
        );
        let memory = system.total_memory();
        let arch = env::consts::ARCH;
        let commit_sha = ZAKU_COMMIT_SHA.to_string();

        let gpu_specs = window.gpu_specs().map(|specs| {
            format!(
                "{} || {} || {}",
                specs.device_name, specs.driver_name, specs.driver_info
            )
        });

        cx.background_spawn(async move {
            SystemSpecs {
                app_version,
                os_name,
                os_version,
                memory,
                arch,
                commit_sha,
                gpu_specs,
            }
        })
    }
}

impl Display for SystemSpecs {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let os_information = format!("OS: {} {}", self.os_name, self.os_version);
        let app_version_information =
            format!("{ZAKU_NAME}: {} ({})", self.app_version, self.commit_sha);
        let system_specs = [
            app_version_information,
            os_information,
            format!("Memory: {}", format_bytes(self.memory)),
            format!("Architecture: {}", self.arch),
        ]
        .into_iter()
        .chain(self.gpu_specs.as_ref().map(|specs| format!("GPU: {specs}")))
        .collect::<Vec<String>>()
        .join("\n");

        write!(formatter, "{system_specs}")
    }
}

fn format_bytes(bytes: u64) -> String {
    const SUFFIX: [&str; 9] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB", "ZiB", "YiB"];
    const UNIT: u128 = 1024;

    if bytes == 0 {
        return "0 B".to_string();
    }

    let bytes = u128::from(bytes);
    let mut divisor = 1;
    let mut unit_index = 0;

    while bytes / divisor >= UNIT && unit_index < SUFFIX.len() - 1 {
        divisor *= UNIT;
        unit_index += 1;
    }

    let rounded_tenths = (bytes * 10 + divisor / 2) / divisor;
    let whole = rounded_tenths / 10;
    let fraction = rounded_tenths % 10;
    let suffix = SUFFIX
        .get(unit_index)
        .expect("unit index should be in bounds");

    if fraction == 0 {
        format!("{whole} {suffix}")
    } else {
        format!("{whole}.{fraction} {suffix}")
    }
}

pub fn os_name() -> String {
    System::name().unwrap_or_else(|| env::consts::OS.to_string())
}

pub fn os_version() -> String {
    System::long_os_version().unwrap_or_else(|| "unknown".to_string())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuInfo {
    pub device_name: Option<String>,
    pub device_pci_id: u16,
    pub vendor_name: Option<String>,
    pub vendor_pci_id: u16,
    pub driver_version: Option<String>,
    pub driver_name: Option<String>,
}

#[cfg(target_os = "linux")]
pub fn read_gpu_info_from_sys_class_drm() -> anyhow::Result<Vec<GpuInfo>> {
    let directory = std::fs::read_dir("/sys/class/drm").context("failed to read /sys/class/drm")?;
    let mut pci_addresses = Vec::new();
    let mut gpus = Vec::new();
    let pci_db = match pciid_parser::Database::read() {
        Ok(db) => Some(db),
        Err(error) => {
            log::warn!("Failed to read PCI ID database: {error}");
            None
        }
    };

    for entry in directory {
        let Ok(entry) = entry else {
            continue;
        };
        let device_path = entry.path().join("device");
        let Some(pci_address) = device_path.read_link().ok().and_then(|pci_address| {
            pci_address
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .map(str::trim)
                .map(str::to_string)
        }) else {
            continue;
        };
        let Ok(device_pci_id) = read_pci_id_from_path(device_path.join("device")) else {
            continue;
        };
        let Ok(vendor_pci_id) = read_pci_id_from_path(device_path.join("vendor")) else {
            continue;
        };
        let driver_name = std::fs::read_link(device_path.join("driver"))
            .ok()
            .and_then(|driver_link| {
                driver_link
                    .file_name()
                    .and_then(std::ffi::OsStr::to_str)
                    .map(str::trim)
                    .map(str::to_string)
            });
        let driver_version = driver_name
            .as_ref()
            .and_then(|driver_name| {
                std::fs::read_to_string(format!("/sys/module/{driver_name}/version")).ok()
            })
            .as_deref()
            .map(str::trim)
            .map(str::to_string);

        let already_found = gpus
            .iter()
            .zip(&pci_addresses)
            .any(|(gpu, gpu_pci_address)| {
                gpu_pci_address == &pci_address
                    && gpu.driver_version == driver_version
                    && gpu.driver_name == driver_name
            });
        if already_found {
            continue;
        }

        let vendor = pci_db
            .as_ref()
            .and_then(|db| db.vendors.get(&vendor_pci_id));
        let vendor_name = vendor.map(|vendor| vendor.name.clone());
        let device_name = vendor
            .and_then(|vendor| vendor.devices.get(&device_pci_id))
            .map(|device| device.name.clone());

        gpus.push(GpuInfo {
            device_name,
            device_pci_id,
            vendor_name,
            vendor_pci_id,
            driver_version,
            driver_name,
        });
        pci_addresses.push(pci_address);
    }

    Ok(gpus)
}

#[cfg(target_os = "linux")]
fn read_pci_id_from_path(path: impl AsRef<Path>) -> anyhow::Result<u16> {
    let id = std::fs::read_to_string(path)?;
    let id = id.trim();
    let id = id
        .strip_prefix("0x")
        .with_context(|| format!("device ID is missing 0x prefix: {id}"))?;
    let id_length = id.len();
    anyhow::ensure!(
        id_length == 4,
        "not a device ID, expected 4 digits, found {id_length}"
    );
    u16::from_str_radix(id, 16).context("failed to parse device ID")
}
