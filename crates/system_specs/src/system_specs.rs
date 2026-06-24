use gpui::{App, AppContext, Task, Window};
use std::{
    env,
    fmt::{self, Display},
};
use sysinfo::{MemoryRefreshKind, RefreshKind, System};

use metadata::{ZAKU_COMMIT_SHA, ZAKU_NAME, ZAKU_VERSION};

#[derive(Clone, Debug)]
pub struct SystemSpecs {
    app_version: String,
    os_name: String,
    os_version: String,
    memory: u64,
    architecture: &'static str,
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
        let app_version = ZAKU_VERSION.to_string();
        let system = System::new_with_specifics(
            RefreshKind::nothing().with_memory(MemoryRefreshKind::everything()),
        );
        let memory = system.total_memory();
        let architecture = env::consts::ARCH;
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
                architecture,
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
            format!("Architecture: {}", self.architecture),
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
