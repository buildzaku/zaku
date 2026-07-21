use gpui::{App, Global};
use jiff::civil::Date;
use semver::{BuildMetadata, Prerelease, Version};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{cmp::Ordering, env, error, fmt, num::NonZeroU64, str::FromStr};

pub const ZAKU_NAME: &str = env!("ZAKU_NAME");
pub const ZAKU_DESCRIPTION: &str = env!("ZAKU_DESCRIPTION");
const ZAKU_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const ZAKU_IDENTIFIER: &str = env!("ZAKU_IDENTIFIER");
pub const ZAKU_REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
pub const ZAKU_BUILD_ID: Option<&str> = option_env!("ZAKU_BUILD_ID");
pub const ZAKU_COMMIT_SHA: &str = env!("ZAKU_COMMIT_SHA");
pub const ZAKU_SERVER_URL: &str = match option_env!("ZAKU_SERVER_URL") {
    Some(url) => url,
    None => "https://api.zaku.dev",
};

struct GlobalAppVersion(AppVersion);

impl Global for GlobalAppVersion {}

#[derive(Debug)]
pub enum AppVersionError {
    InvalidFormat,
    InvalidPrerelease,
    InvalidVersion(semver::Error),
}

impl fmt::Display for AppVersionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFormat => {
                write!(formatter, "version must use YY.MINOR[.PATCH][-PRERELEASE]")
            }
            Self::InvalidPrerelease => write!(formatter, "unsupported prerelease version"),
            Self::InvalidVersion(error) => write!(formatter, "{error}"),
        }
    }
}

impl error::Error for AppVersionError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::InvalidFormat | Self::InvalidPrerelease => None,
            Self::InvalidVersion(error) => Some(error),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppVersion {
    major: u64,
    minor: u64,
    patch: Option<NonZeroU64>,
    prerelease: Prerelease,
}

impl AppVersion {
    pub fn load(package_version: &str) -> Self {
        if let Ok(from_env) = env::var("ZAKU_APP_VERSION") {
            from_env.parse().expect("invalid ZAKU_APP_VERSION")
        } else {
            package_version
                .parse()
                .expect("invalid version in Cargo.toml")
        }
    }

    pub fn global(cx: &App) -> Self {
        cx.global::<GlobalAppVersion>().0.clone()
    }

    pub fn is_stable(&self) -> bool {
        self.prerelease.is_empty()
    }

    pub fn display(&self) -> String {
        let mut version = if let Some(patch) = self.patch {
            format!("{}.{}.{patch}", self.major, self.minor)
        } else {
            format!("{}.{}", self.major, self.minor)
        };
        if !self.prerelease.is_empty() {
            version.push('-');
            version.push_str(self.prerelease.as_str());
        }

        version
    }
}

impl Ord for AppVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        Version::from(self).cmp(&Version::from(other))
    }
}

impl PartialOrd for AppVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl FromStr for AppVersion {
    type Err = AppVersionError;

    fn from_str(version: &str) -> Result<Self, Self::Err> {
        if version.contains('+') {
            return Err(AppVersionError::InvalidFormat);
        }

        let (core, prerelease) = version
            .split_once('-')
            .map_or((version, None), |(core, prerelease)| {
                (core, Some(prerelease))
            });
        let component_count = core.split('.').count();
        let normalized_version = if component_count == 2 {
            if let Some(prerelease) = prerelease {
                format!("{core}.0-{prerelease}")
            } else {
                format!("{core}.0")
            }
        } else if component_count == 3 {
            version.to_string()
        } else {
            return Err(AppVersionError::InvalidFormat);
        };
        let version =
            Version::parse(&normalized_version).map_err(AppVersionError::InvalidVersion)?;

        if !version.pre.is_empty() {
            let identifiers = version.pre.as_str().split('.').collect::<Vec<_>>();
            let supported = match identifiers.as_slice() {
                ["preview", number] => number.parse::<NonZeroU64>().is_ok(),
                ["nightly", date] => {
                    version.patch == 0
                        && Date::strptime("%Y-%m-%d", date)
                            .is_ok_and(|parsed| parsed.to_string() == *date)
                }
                ["dev", build, commit_sha] => {
                    version.patch == 0
                        && build.parse::<NonZeroU64>().is_ok()
                        && commit_sha
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
                }
                _ => false,
            };
            if !supported {
                return Err(AppVersionError::InvalidPrerelease);
            }
        }

        Ok(Self {
            major: version.major,
            minor: version.minor,
            patch: NonZeroU64::new(version.patch),
            prerelease: version.pre,
        })
    }
}

impl Serialize for AppVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.display())
    }
}

impl<'de> Deserialize<'de> for AppVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

impl From<&AppVersion> for Version {
    fn from(version: &AppVersion) -> Self {
        Self {
            major: version.major,
            minor: version.minor,
            patch: version.patch.map_or(0, NonZeroU64::get),
            pre: version.prerelease.clone(),
            build: BuildMetadata::EMPTY,
        }
    }
}

impl From<AppVersion> for Version {
    fn from(version: AppVersion) -> Self {
        Self::from(&version)
    }
}

pub fn init(cx: &mut App) {
    cx.set_global(GlobalAppVersion(AppVersion::load(ZAKU_VERSION)));
}

pub fn init_test(app_version: AppVersion, cx: &mut App) {
    cx.set_global(GlobalAppVersion(app_version));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_version_parses_supported_formats() {
        for (version, display_version) in [
            ("26.1", "26.1"),
            ("26.1.0", "26.1"),
            ("26.1.1", "26.1.1"),
            ("26.2-preview.2", "26.2-preview.2"),
            ("26.2.0-preview.2", "26.2-preview.2"),
            ("26.3-nightly.2026-07-19", "26.3-nightly.2026-07-19"),
            ("26.3-dev.1000.aaaaaaaa", "26.3-dev.1000.aaaaaaaa"),
        ] {
            let parsed = version.parse::<AppVersion>().unwrap();
            assert_eq!(parsed.display(), display_version);
        }
    }

    #[test]
    fn test_app_version_orders_releases() {
        for (older, newer) in [
            ("26.1", "26.2-preview.1"),
            ("26.2-preview.1", "26.2-preview.2"),
            ("26.2-preview.2", "26.2"),
            ("26.2", "26.2.1-preview.1"),
            ("26.2.1-preview.1", "26.2.1"),
            ("26.3-nightly.2026-07-31", "26.3-nightly.2026-08-01"),
            ("26.3-dev.999.ffffffff", "26.3-dev.1000.aaaaaaaa"),
        ] {
            assert!(
                older.parse::<AppVersion>().unwrap() < newer.parse::<AppVersion>().unwrap(),
                "{older} should be older than {newer}"
            );
        }
    }

    #[test]
    fn test_app_version_rejects_unsupported_formats() {
        for (version, reason) in [
            ("26", "minor version should be required"),
            ("26.1-alpha", "unknown prereleases should be rejected"),
            ("26.1-preview.0", "preview numbers should start at one"),
            ("26.1-nightly.2026-02-30", "nightly date should be valid"),
            (
                "26.1-nightly.2026-7-19",
                "nightly date should use fixed-width ISO format",
            ),
            (
                "26.1-dev.aaaaaaaa",
                "development build number should be required",
            ),
            ("26.1-dev.1000", "development commit should be required"),
            (
                "26.1-dev.1000.AAAAAAAA",
                "development commit should use lowercase hexadecimal",
            ),
            (
                "26.1.1-nightly.2026-07-19",
                "nightly version should not contain a patch",
            ),
            (
                "26.1.1-dev.1000.aaaaaaaa",
                "development version should not contain a patch",
            ),
            ("26.1.0+build", "build metadata should not be supported"),
            (
                "26.1.0.1",
                "version should not contain a fourth numeric component",
            ),
            ("v26.1", "version should start with a numeric year"),
        ] {
            assert!(version.parse::<AppVersion>().is_err(), "{reason}");
        }
    }
}
