use anyhow::Context as _;
use clap::{Parser, Subcommand};
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
        /// Version to inspect. Defaults to the checked-out Zaku version.
        version: Option<AppVersion>,
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
            version,
            long,
            branch,
            channel,
        } => {
            let version = match version {
                Some(version) => version,
                None => env!("CARGO_PKG_VERSION").parse()?,
            };
            if long {
                Ok(version.long())
            } else if branch {
                Ok(version.release_branch())
            } else if channel {
                Ok(version.release_channel()?.as_str().to_string())
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
        assert_eq!(
            run_command(&["show"]).unwrap(),
            env!("CARGO_PKG_VERSION")
                .parse::<AppVersion>()
                .unwrap()
                .to_string()
        );
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
