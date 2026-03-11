use std::{path::PathBuf, sync::OnceLock};

static CONFIG_DIR: OnceLock<PathBuf> = OnceLock::new();
static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

fn home_dir() -> PathBuf {
    dirs::home_dir().expect("failed to determine home directory")
}

/// Returns the path to the configuration directory.
///
/// - macOS: `~/.config/zaku`
/// - Linux/FreeBSD: `$XDG_CONFIG_HOME/zaku` (or `~/.config/zaku`), with Flatpak override.
/// - Windows: `%APPDATA%\\Zaku`
pub fn config_dir() -> &'static PathBuf {
    CONFIG_DIR.get_or_init(|| {
        if cfg!(target_os = "windows") {
            dirs::config_dir()
                .expect("failed to determine RoamingAppData directory")
                .join("Zaku")
        } else if cfg!(any(target_os = "linux", target_os = "freebsd")) {
            if let Ok(flatpak_xdg_config) = std::env::var("FLATPAK_XDG_CONFIG_HOME") {
                PathBuf::from(flatpak_xdg_config)
            } else {
                dirs::config_dir().expect("failed to determine XDG_CONFIG_HOME directory")
            }
            .join("zaku")
        } else {
            home_dir().join(".config").join("zaku")
        }
    })
}

/// Returns the path to the data directory.
///
/// - macOS: `~/Library/Application Support/Zaku`
/// - Linux/FreeBSD: `$XDG_DATA_HOME/zaku` (or `~/.local/share/zaku`), with Flatpak override.
/// - Windows: `%LOCALAPPDATA%\\Zaku`
pub fn data_dir() -> &'static PathBuf {
    DATA_DIR.get_or_init(|| {
        if cfg!(target_os = "macos") {
            home_dir()
                .join("Library")
                .join("Application Support")
                .join("Zaku")
        } else if cfg!(any(target_os = "linux", target_os = "freebsd")) {
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
            config_dir().clone()
        }
    })
}

/// Returns the path to the `settings.json` file.
pub fn settings_file() -> &'static PathBuf {
    static SETTINGS_FILE: OnceLock<PathBuf> = OnceLock::new();
    SETTINGS_FILE.get_or_init(|| config_dir().join("settings.json"))
}
