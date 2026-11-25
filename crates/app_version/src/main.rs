use anyhow::Context as _;
use clap::{Parser, Subcommand};
use serde::Deserialize;
use std::process::ExitCode;

use app_version::{AppVersion, Bump, ReleaseChannel, ReleaseSnapshot};

#[derive(Debug, Parser)]
#[command(name = "version", about = "Inspect, bump and promote Zaku versions.")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Inspect a version.
    Show {
        #[command(flatten)]
        target: ShowTarget,
        /// Print the long version.
        #[arg(long, group = "output")]
        long: bool,
        /// Print the release branch.
        #[arg(long, group = "output")]
        branch: bool,
        /// Print the release channel.
        #[arg(long, group = "output")]
        channel: bool,
    },
    /// Advance a version component.
    Bump { version: AppVersion, bump: Bump },
    /// Promote a dev or beta version.
    Promote {
        version: AppVersion,
        channel: ReleaseChannel,
    },
    /// Inspect the latest release for a channel.
    Latest {
        channel: ReleaseChannel,
        /// Return the latest release if its channel is active.
        #[arg(long)]
        active: bool,
        /// Print the release branch.
        #[arg(long)]
        branch: bool,
    },
}

#[derive(Debug, clap::Args)]
#[group(required = true, multiple = false)]
struct ShowTarget {
    /// Version to inspect.
    version: Option<AppVersion>,
    /// Inspect the checked-out version of a workspace package.
    #[arg(long)]
    package: Option<String>,
}

#[derive(Deserialize)]
struct CargoMetadata {
    packages: Vec<CargoPackage>,
}

#[derive(Deserialize)]
struct CargoPackage {
    name: String,
    version: AppVersion,
}

fn main() -> ExitCode {
    match execute(Args::parse()) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn execute(arguments: Args) -> anyhow::Result<String> {
    match arguments.command {
        Command::Show {
            target: ShowTarget { version, package },
            long,
            branch,
            channel,
        } => {
            let version = match package {
                None => version.expect("clap should require a version without a package"),
                Some(package) => {
                    let output = std::process::Command::new("cargo")
                        .args(["metadata", "--no-deps", "--format-version=1"])
                        .output()
                        .context("could not query cargo metadata")?;
                    if !output.status.success() {
                        anyhow::bail!(
                            "cargo metadata failed: {}",
                            String::from_utf8_lossy(&output.stderr).trim()
                        );
                    }

                    let metadata: CargoMetadata = serde_json::from_slice(&output.stdout)
                        .context("invalid cargo metadata output")?;
                    metadata
                        .packages
                        .into_iter()
                        .find(|cargo_package| cargo_package.name == package)
                        .with_context(|| {
                            format!("{package} package is missing from cargo metadata")
                        })?
                        .version
                }
            };
            if long {
                Ok(version.long())
            } else if branch {
                Ok(version.release_branch())
            } else if channel {
                Ok(version.release_channel()?.to_string())
            } else {
                Ok(version.to_string())
            }
        }
        Command::Bump { version, bump } => Ok(version.bump(bump)?.to_string()),
        Command::Promote { version, channel } => Ok(version.promote(channel)?.to_string()),
        Command::Latest {
            channel,
            active,
            branch,
        } => {
            let output = std::process::Command::new("git")
                .args([
                    "ls-remote",
                    "--tags",
                    "--refs",
                    "origin",
                    "refs/tags/[0-9][0-9].[0-9]*",
                ])
                .output()
                .context("could not query origin release tags")?;
            if !output.status.success() {
                anyhow::bail!(
                    "git ls-remote failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                );
            }

            let output =
                String::from_utf8(output.stdout).context("git tag output is not valid UTF-8")?;
            let tags = output
                .lines()
                .map(|line| -> anyhow::Result<_> {
                    let Some((_, reference)) = line.split_once('\t') else {
                        anyhow::bail!("invalid git tag output: {line}");
                    };
                    reference
                        .strip_prefix("refs/tags/")
                        .with_context(|| format!("invalid git tag output: {line}"))
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
            let snapshot = ReleaseSnapshot::from_tags(tags)?;
            let version = if active {
                snapshot.active(channel)
            } else {
                snapshot.latest(channel)
            };

            Ok(match (version, branch) {
                (Some(version), true) => version.release_branch(),
                (Some(version), false) => version.to_string(),
                (None, _) => String::new(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_command(arguments: &[&str]) -> anyhow::Result<String> {
        let arguments = std::iter::once("version").chain(arguments.iter().copied());
        let arguments = Args::try_parse_from(arguments)?;
        execute(arguments)
    }

    #[test]
    fn test_show_command() {
        run_command(&["show", "--package", "zaku"])
            .unwrap()
            .parse::<AppVersion>()
            .unwrap();
        run_command(&["show"]).unwrap_err();
        run_command(&["show", "26.1", "--package", "zaku"]).unwrap_err();
        assert_eq!(
            run_command(&["show", "26.1.0-beta.2"]).unwrap(),
            "26.1-beta.2"
        );
        assert_eq!(
            run_command(&["show", "26.1-dev", "--long"]).unwrap(),
            "26.1.0-dev"
        );
        assert_eq!(
            run_command(&["show", "26.1.1", "--branch"]).unwrap(),
            "26.1.x"
        );
        assert_eq!(
            run_command(&["show", "26.1-beta.2", "--channel"]).unwrap(),
            "beta"
        );
        assert_eq!(
            run_command(&["show", "26.1", "--channel"]).unwrap(),
            "stable"
        );
        run_command(&["show", "26.1", "--long", "--channel"]).unwrap_err();
    }
}
