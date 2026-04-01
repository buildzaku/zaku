#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
compile_error!("settings only supports macOS, Linux and Windows");

use std::{path::PathBuf, sync::OnceLock};

fn home_dir() -> PathBuf {
    dirs::home_dir().expect("failed to determine home directory")
}

/// Returns the path to the configuration directory.
///
/// - macOS: `~/.config/zaku`
/// - Linux: `$XDG_CONFIG_HOME/zaku` (or `~/.config/zaku`), with Flatpak override.
/// - Windows: `%APPDATA%\\Zaku`
pub fn config_dir() -> &'static PathBuf {
    static CONFIG_DIR: OnceLock<PathBuf> = OnceLock::new();
    CONFIG_DIR.get_or_init(|| {
        if cfg!(target_os = "macos") {
            home_dir().join(".config").join("zaku")
        } else if cfg!(target_os = "linux") {
            if let Ok(flatpak_xdg_config) = std::env::var("FLATPAK_XDG_CONFIG_HOME") {
                PathBuf::from(flatpak_xdg_config)
            } else {
                dirs::config_dir().expect("failed to determine XDG_CONFIG_HOME directory")
            }
            .join("zaku")
        } else if cfg!(target_os = "windows") {
            dirs::config_dir()
                .expect("failed to determine RoamingAppData directory")
                .join("Zaku")
        } else {
            unreachable!("Unsupported platform")
        }
    })
}

/// Returns the path to the data directory.
///
/// - macOS: `~/Library/Application Support/Zaku`
/// - Linux: `$XDG_DATA_HOME/zaku` (or `~/.local/share/zaku`), with Flatpak override.
/// - Windows: `%LOCALAPPDATA%\\Zaku`
pub fn data_dir() -> &'static PathBuf {
    static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();
    DATA_DIR.get_or_init(|| {
        if cfg!(target_os = "macos") {
            home_dir()
                .join("Library")
                .join("Application Support")
                .join("Zaku")
        } else if cfg!(target_os = "linux") {
            if let Ok(flatpak_xdg_data) = std::env::var("FLATPAK_XDG_DATA_HOME") {
                PathBuf::from(flatpak_xdg_data)
            } else {
                dirs::data_local_dir().expect("failed to determine XDG_DATA_HOME directory")
            }
            .join("zaku")
        } else if cfg!(target_os = "windows") {
            dirs::data_local_dir()
                .expect("failed to determine LocalAppData directory")
                .join("Zaku")
        } else {
            unreachable!("Unsupported platform")
        }
    })
}

/// Returns the path to the logs directory.
///
/// - macOS: `~/Library/Logs/Zaku`
/// - Linux: `$XDG_DATA_HOME/zaku/logs` (or `~/.local/share/zaku/logs`), with Flatpak override.
/// - Windows: `%LOCALAPPDATA%\\Zaku\\logs`
pub fn logs_dir() -> &'static PathBuf {
    static LOGS_DIR: OnceLock<PathBuf> = OnceLock::new();
    LOGS_DIR.get_or_init(|| {
        if cfg!(target_os = "macos") {
            home_dir().join("Library/Logs/Zaku")
        } else if cfg!(target_os = "linux") || cfg!(target_os = "windows") {
            data_dir().join("logs")
        } else {
            unreachable!("Unsupported platform")
        }
    })
}

/// Returns the path to the `Zaku.log` file.
pub fn log_file() -> &'static PathBuf {
    static LOG_FILE: OnceLock<PathBuf> = OnceLock::new();
    LOG_FILE.get_or_init(|| logs_dir().join("Zaku.log"))
}

/// Returns the path to the `Zaku.log.old` file.
pub fn old_log_file() -> &'static PathBuf {
    static OLD_LOG_FILE: OnceLock<PathBuf> = OnceLock::new();
    OLD_LOG_FILE.get_or_init(|| logs_dir().join("Zaku.log.old"))
}

/// Returns the path to the `settings.json` file.
pub fn settings_file() -> &'static PathBuf {
    static SETTINGS_FILE: OnceLock<PathBuf> = OnceLock::new();
    SETTINGS_FILE.get_or_init(|| config_dir().join("settings.json"))
}
