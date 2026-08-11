use anyhow::Context as _;
use clap::ValueEnum;
use jiff::civil::Date;
use semver::{Prerelease, Version};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::{fmt, str::FromStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ReleaseChannel {
    Beta,
    Stable,
}

impl fmt::Display for ReleaseChannel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Beta => "beta",
            Self::Stable => "stable",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Bump {
    Minor,
    Patch,
    Beta,
    Year,
}

#[derive(Debug, Default)]
pub struct ReleaseSnapshot {
    beta: Option<AppVersion>,
    stable: Option<AppVersion>,
}

impl ReleaseSnapshot {
    pub fn from_tags(tags: impl IntoIterator<Item: AsRef<str>>) -> anyhow::Result<Self> {
        let mut snapshot = Self::default();

        for tag in tags {
            let tag = tag.as_ref();
            let version: AppVersion = tag
                .parse()
                .with_context(|| format!("invalid release tag {tag}"))?;

            let slot = if version.is_beta() {
                &mut snapshot.beta
            } else if version.is_stable() {
                &mut snapshot.stable
            } else {
                continue;
            };

            *slot = slot.take().max(Some(version));
        }

        Ok(snapshot)
    }

    pub fn latest(&self, channel: ReleaseChannel) -> Option<&AppVersion> {
        match channel {
            ReleaseChannel::Beta => self.beta.as_ref(),
            ReleaseChannel::Stable => self.stable.as_ref(),
        }
    }

    pub fn active(&self, channel: ReleaseChannel) -> Option<&AppVersion> {
        match channel {
            ReleaseChannel::Beta => match (&self.beta, &self.stable) {
                (Some(beta), Some(stable)) if beta > stable => Some(beta),
                (Some(beta), None) => Some(beta),
                _ => None,
            },
            ReleaseChannel::Stable => self.stable.as_ref(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AppVersion(Version);

impl AppVersion {
    pub fn is_stable(&self) -> bool {
        self.0.pre.is_empty()
    }

    pub fn is_beta(&self) -> bool {
        self.0.pre.as_str().starts_with("beta.")
    }

    pub fn long(&self) -> String {
        self.0.to_string()
    }

    pub fn release_branch(&self) -> String {
        format!("{}.{}.x", self.0.major, self.0.minor)
    }

    pub fn release_channel(&self) -> anyhow::Result<ReleaseChannel> {
        if self.is_beta() {
            Ok(ReleaseChannel::Beta)
        } else if self.is_stable() {
            Ok(ReleaseChannel::Stable)
        } else {
            anyhow::bail!("version is not a beta or stable release")
        }
    }

    pub fn bump(&self, bump: Bump) -> anyhow::Result<Self> {
        let mut version = self.0.clone();
        match bump {
            Bump::Minor => {
                if version.patch != 0 || version.pre.as_str() != "dev" {
                    anyhow::bail!("minor and year bumps require a dev version");
                }

                version.minor += 1;
                version.patch = 0;
                version.pre = Prerelease::new("dev").expect("dev prerelease should be valid");
            }
            Bump::Patch => {
                if !self.is_stable() {
                    anyhow::bail!("patch bump requires a stable version");
                }
                version.patch += 1;
            }
            Bump::Beta => {
                let Some(beta_number) = version.pre.as_str().strip_prefix("beta.") else {
                    anyhow::bail!("beta bump requires a beta version");
                };
                let beta_number = beta_number
                    .parse::<u64>()
                    .expect("beta number should be valid")
                    + 1;
                version.pre = Prerelease::new(&format!("beta.{beta_number}"))
                    .expect("beta prerelease should be valid");
            }
            Bump::Year => {
                if version.patch != 0 || version.pre.as_str() != "dev" {
                    anyhow::bail!("minor and year bumps require a dev version");
                }

                version.major += 1;
                version.minor = 0;
                version.patch = 0;
                version.pre = Prerelease::new("dev").expect("dev prerelease should be valid");
            }
        }

        Ok(Self(version))
    }

    pub fn promote(&self, channel: ReleaseChannel) -> anyhow::Result<Self> {
        let prerelease = match (self.0.pre.as_str(), channel) {
            ("dev", ReleaseChannel::Beta) => {
                Prerelease::new("beta.1").expect("beta prerelease should be valid")
            }
            ("dev", ReleaseChannel::Stable) => Prerelease::EMPTY,
            (prerelease, ReleaseChannel::Stable) if prerelease.starts_with("beta.") => {
                Prerelease::EMPTY
            }
            (prerelease, ReleaseChannel::Beta) if prerelease.starts_with("beta.") => {
                anyhow::bail!("beta versions can only be promoted to stable");
            }
            _ => {
                anyhow::bail!("promotion requires a dev or beta version");
            }
        };

        let mut version = self.0.clone();
        version.pre = prerelease;
        Ok(Self(version))
    }
}

impl fmt::Display for AppVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.patch == 0 {
            write!(formatter, "{}.{}", self.0.major, self.0.minor)?;
        } else {
            write!(
                formatter,
                "{}.{}.{}",
                self.0.major, self.0.minor, self.0.patch
            )?;
        }
        if !self.0.pre.is_empty() {
            write!(formatter, "-{}", self.0.pre.as_str())?;
        }

        Ok(())
    }
}

impl FromStr for AppVersion {
    type Err = anyhow::Error;

    fn from_str(version: &str) -> anyhow::Result<Self> {
        if version.contains('+') {
            anyhow::bail!("version must use YY.MINOR[.PATCH][-PRERELEASE]");
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
            anyhow::bail!("version must use YY.MINOR[.PATCH][-PRERELEASE]");
        };
        let version = Version::parse(&normalized_version)?;

        if !version.pre.is_empty() {
            let identifiers = version.pre.as_str().split('.').collect::<Vec<_>>();
            let supported = match identifiers.as_slice() {
                ["beta", number] => {
                    version.patch == 0 && number.parse::<u64>().is_ok_and(|number| number > 0)
                }
                ["nightly", date] => {
                    version.patch == 0
                        && Date::strptime("%Y-%m-%d", date)
                            .is_ok_and(|parsed| parsed.to_string() == *date)
                }
                ["dev"] => version.patch == 0,
                ["dev", build, commit_sha] => {
                    version.patch == 0
                        && build.parse::<u64>().is_ok_and(|build| build > 0)
                        && commit_sha
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
                }
                _ => false,
            };
            if !supported {
                anyhow::bail!("unsupported prerelease version");
            }
        }

        Ok(Self(version))
    }
}

impl Serialize for AppVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for AppVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_version_parse() {
        for (input, expected) in [
            ("26.1.0", "26.1.0"),
            ("26.1", "26.1.0"),
            ("26.1.1", "26.1.1"),
            ("26.2.0-beta.2", "26.2.0-beta.2"),
            ("26.2-beta.2", "26.2.0-beta.2"),
            ("26.3.0-nightly.2026-07-19", "26.3.0-nightly.2026-07-19"),
            ("26.3-nightly.2026-07-19", "26.3.0-nightly.2026-07-19"),
            ("26.3.0-dev", "26.3.0-dev"),
            ("26.3-dev", "26.3.0-dev"),
            ("26.3.0-dev.1000.aaaaaaaa", "26.3.0-dev.1000.aaaaaaaa"),
            ("26.3-dev.1000.aaaaaaaa", "26.3.0-dev.1000.aaaaaaaa"),
        ] {
            assert_eq!(
                input.parse::<AppVersion>().unwrap(),
                AppVersion(Version::parse(expected).unwrap())
            );
        }
    }

    #[test]
    fn test_app_version_display() {
        for (input, expected) in [
            ("26.1.0", "26.1"),
            ("26.1.1", "26.1.1"),
            ("26.2.0-beta.2", "26.2-beta.2"),
            ("26.3.0-nightly.2026-07-19", "26.3-nightly.2026-07-19"),
            ("26.3.0-dev", "26.3-dev"),
            ("26.3.0-dev.1000.aaaaaaaa", "26.3-dev.1000.aaaaaaaa"),
        ] {
            assert_eq!(input.parse::<AppVersion>().unwrap().to_string(), expected);
        }
    }

    #[test]
    fn test_app_version_rejects_unsupported_formats() {
        for (input, reason) in [
            ("26", "minor version should be required"),
            ("26.1-alpha", "unknown prereleases should be rejected"),
            ("26.1-beta.0", "beta numbers should start at one"),
            ("26.1.1-beta.1", "beta version should not contain a patch"),
            ("26.1-nightly.2026-02-30", "nightly date should be valid"),
            (
                "26.1-nightly.2026-7-19",
                "nightly date should use fixed-width ISO format",
            ),
            ("26.1-dev.aaaaaaaa", "dev build number should be required"),
            ("26.1-dev.1000", "dev commit should be required"),
            (
                "26.1-dev.1000.AAAAAAAA",
                "dev commit should use lowercase hexadecimal",
            ),
            (
                "26.1.1-nightly.2026-07-19",
                "nightly version should not contain a patch",
            ),
            (
                "26.1.1-dev.1000.aaaaaaaa",
                "dev version should not contain a patch",
            ),
            ("26.1.1-dev", "dev version should not contain a patch"),
            ("26.1.0+build", "build metadata should not be supported"),
            (
                "26.1.0.1",
                "version should not contain a fourth numeric component",
            ),
            ("v26.1", "version should start with a numeric year"),
        ] {
            assert!(input.parse::<AppVersion>().is_err(), "{reason}");
        }
    }

    #[test]
    fn test_app_version_bump() {
        for (version, bump, expected) in [
            ("26.1.0-dev", Bump::Minor, "26.2-dev"),
            ("26.1.0", Bump::Patch, "26.1.1"),
            ("26.1.0-beta.1", Bump::Beta, "26.1-beta.2"),
            ("26.1.0-dev", Bump::Year, "27.0-dev"),
        ] {
            assert_eq!(
                version
                    .parse::<AppVersion>()
                    .unwrap()
                    .bump(bump)
                    .unwrap()
                    .to_string(),
                expected
            );
        }

        for (version, bump, reason) in [
            (
                "26.1.0",
                Bump::Minor,
                "minor bumps should require a dev version",
            ),
            (
                "26.1.0-beta.1",
                Bump::Patch,
                "patch bumps should require a stable version",
            ),
            (
                "26.1.0",
                Bump::Beta,
                "beta bumps should require a beta version",
            ),
        ] {
            version
                .parse::<AppVersion>()
                .unwrap()
                .bump(bump)
                .expect_err(reason);
        }
    }

    #[test]
    fn test_app_version_promote() {
        for (version, channel, expected) in [
            ("26.1.0-dev", ReleaseChannel::Beta, "26.1-beta.1"),
            ("26.1.0-dev", ReleaseChannel::Stable, "26.1"),
            ("26.1.0-beta.2", ReleaseChannel::Stable, "26.1"),
        ] {
            assert_eq!(
                version
                    .parse::<AppVersion>()
                    .unwrap()
                    .promote(channel)
                    .unwrap()
                    .to_string(),
                expected
            );
        }

        for (version, channel, reason) in [
            (
                "26.1.0-beta.2",
                ReleaseChannel::Beta,
                "beta versions should only be promoted to stable",
            ),
            (
                "26.1.0",
                ReleaseChannel::Stable,
                "promotion should require a dev or beta version",
            ),
        ] {
            version
                .parse::<AppVersion>()
                .unwrap()
                .promote(channel)
                .expect_err(reason);
        }
    }

    #[test]
    fn test_release_snapshot_from_tags() {
        let snapshot = ReleaseSnapshot::from_tags([
            "26.1-beta.2",
            "26.0.1",
            "26.1-nightly.2026-08-05",
            "25.9-beta.99",
            "26.0-beta.1",
            "26.1-beta.1",
            "26.1-dev.1000.aaaaaaaa",
            "25.9.99",
            "26.0",
        ])
        .unwrap();

        assert_eq!(
            snapshot.latest(ReleaseChannel::Beta).unwrap().to_string(),
            "26.1-beta.2"
        );
        assert_eq!(
            snapshot.latest(ReleaseChannel::Stable).unwrap().to_string(),
            "26.0.1"
        );
    }

    #[test]
    fn test_release_snapshot_active() {
        let snapshot = ReleaseSnapshot::from_tags(["26.1-beta.1"]).unwrap();

        assert_eq!(
            snapshot.active(ReleaseChannel::Beta).unwrap().to_string(),
            "26.1-beta.1"
        );
        assert!(snapshot.active(ReleaseChannel::Stable).is_none());

        let snapshot = ReleaseSnapshot::from_tags(["26.1"]).unwrap();

        assert!(snapshot.latest(ReleaseChannel::Beta).is_none());
        assert!(snapshot.active(ReleaseChannel::Beta).is_none());
        assert_eq!(
            snapshot.active(ReleaseChannel::Stable).unwrap().to_string(),
            "26.1"
        );

        let snapshot = ReleaseSnapshot::from_tags(["26.0.1", "26.1-beta.2"]).unwrap();

        assert_eq!(
            snapshot.active(ReleaseChannel::Beta).unwrap().to_string(),
            "26.1-beta.2"
        );
        assert_eq!(
            snapshot.active(ReleaseChannel::Stable).unwrap().to_string(),
            "26.0.1"
        );

        let snapshot = ReleaseSnapshot::from_tags(["26.0.1", "26.1-beta.2", "26.1"]).unwrap();

        assert!(snapshot.active(ReleaseChannel::Beta).is_none());
        assert_eq!(
            snapshot.latest(ReleaseChannel::Beta).unwrap().to_string(),
            "26.1-beta.2"
        );
        assert_eq!(
            snapshot.active(ReleaseChannel::Stable).unwrap().to_string(),
            "26.1"
        );
    }
}
