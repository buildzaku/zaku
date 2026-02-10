#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub enum PlatformStyle {
    Mac,
    Linux,
    Windows,
}

impl PlatformStyle {
    pub const fn platform() -> Self {
        if cfg!(any(target_os = "linux", target_os = "freebsd")) {
            Self::Linux
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Mac
        }
    }
}
