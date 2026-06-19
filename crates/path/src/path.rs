use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

pub fn home_dir() -> &'static PathBuf {
    static HOME_DIR: OnceLock<PathBuf> = OnceLock::new();
    HOME_DIR.get_or_init(|| dirs::home_dir().expect("failed to determine home directory"))
}

pub trait PathExt {
    fn compact(&self) -> PathBuf;
}

impl<T: AsRef<Path>> PathExt for T {
    fn compact(&self) -> PathBuf {
        if cfg!(any(target_os = "linux", target_os = "macos")) {
            match self.as_ref().strip_prefix(home_dir().as_path()) {
                Ok(relative_path) => {
                    let mut shortened_path = PathBuf::new();
                    shortened_path.push("~");
                    shortened_path.push(relative_path);
                    shortened_path
                }
                Err(_) => self.as_ref().to_path_buf(),
            }
        } else {
            self.as_ref().to_path_buf()
        }
    }
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

/// Returns the path to the `keymap.json` file.
pub fn keymap_file() -> &'static PathBuf {
    static KEYMAP_FILE: OnceLock<PathBuf> = OnceLock::new();
    KEYMAP_FILE.get_or_init(|| config_dir().join("keymap.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_compact() {
        let path = home_dir()
            .join(".config")
            .join("zaku")
            .join("settings.json");

        if cfg!(any(target_os = "linux", target_os = "macos")) {
            assert_eq!(
                path.compact().to_str(),
                Some("~/.config/zaku/settings.json")
            );
        } else {
            assert_eq!(path.compact().to_str(), path.to_str());
        }
    }
}
