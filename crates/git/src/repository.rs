use anyhow::Context;
use futures::{FutureExt, future::BoxFuture};
use gpui::{BackgroundExecutor, SharedString, Task};
use std::{
    ffi::{OsStr, OsString},
    fmt, ops,
    path::{Path, PathBuf},
    process::Output,
    sync::Arc,
};

use path::{PathStyle, RelPath};
use util::command;

use crate::status::GitStatus;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RepoPath(Arc<RelPath>);

impl RepoPath {
    pub fn new<S: AsRef<str> + ?Sized>(path: &S) -> anyhow::Result<Self> {
        let rel_path = RelPath::unix(path.as_ref())?;
        Ok(Self::from_rel_path(rel_path))
    }

    pub fn from_std_path(path: &Path, path_style: PathStyle) -> anyhow::Result<Self> {
        let rel_path = RelPath::new(path, path_style)?;
        Ok(Self::from_rel_path(&rel_path))
    }

    pub fn from_rel_path(path: &RelPath) -> RepoPath {
        Self(Arc::from(path))
    }

    pub fn as_std_path(&self) -> &Path {
        if self.is_empty() {
            Path::new(".")
        } else {
            self.0.as_std_path()
        }
    }
}

impl fmt::Debug for RepoPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl AsRef<Arc<RelPath>> for RepoPath {
    fn as_ref(&self) -> &Arc<RelPath> {
        &self.0
    }
}

impl ops::Deref for RepoPath {
    type Target = RelPath;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Branch {
    pub is_head: bool,
    pub ref_name: SharedString,
    pub upstream: Option<Upstream>,
    pub most_recent_commit: Option<CommitSummary>,
}

impl Branch {
    pub fn name(&self) -> &str {
        self.ref_name
            .as_ref()
            .strip_prefix("refs/heads/")
            .or_else(|| self.ref_name.as_ref().strip_prefix("refs/remotes/"))
            .unwrap_or(self.ref_name.as_ref())
    }

    pub fn is_remote(&self) -> bool {
        self.ref_name.starts_with("refs/remotes/")
    }

    pub fn remote_name(&self) -> Option<&str> {
        self.ref_name
            .strip_prefix("refs/remotes/")
            .and_then(|stripped| stripped.split('/').next())
    }

    pub fn tracking_status(&self) -> Option<UpstreamTrackingStatus> {
        self.upstream
            .as_ref()
            .and_then(|upstream| upstream.tracking.status())
    }

    pub fn priority_key(&self) -> (bool, Option<i64>) {
        (
            self.is_head,
            self.most_recent_commit
                .as_ref()
                .map(|commit| commit.commit_timestamp),
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BranchesScanResult {
    pub branches: Vec<Branch>,
    pub error: Option<SharedString>,
}

impl From<Vec<Branch>> for BranchesScanResult {
    fn from(branches: Vec<Branch>) -> Self {
        Self {
            branches,
            error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Upstream {
    pub ref_name: SharedString,
    pub tracking: UpstreamTracking,
}

impl Upstream {
    pub fn is_remote(&self) -> bool {
        self.remote_name().is_some()
    }

    pub fn remote_name(&self) -> Option<&str> {
        self.ref_name
            .strip_prefix("refs/remotes/")
            .and_then(|stripped| stripped.split('/').next())
    }

    pub fn stripped_ref_name(&self) -> Option<&str> {
        self.ref_name.strip_prefix("refs/remotes/")
    }

    pub fn branch_name(&self) -> Option<&str> {
        self.ref_name
            .strip_prefix("refs/remotes/")
            .and_then(|stripped| stripped.split_once('/').map(|(_, name)| name))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UpstreamTracking {
    Gone,
    Tracked(UpstreamTrackingStatus),
}

impl UpstreamTracking {
    pub fn is_gone(&self) -> bool {
        matches!(self, UpstreamTracking::Gone)
    }

    pub fn status(&self) -> Option<UpstreamTrackingStatus> {
        match self {
            UpstreamTracking::Gone => None,
            UpstreamTracking::Tracked(status) => Some(*status),
        }
    }
}

impl From<UpstreamTrackingStatus> for UpstreamTracking {
    fn from(status: UpstreamTrackingStatus) -> Self {
        UpstreamTracking::Tracked(status)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UpstreamTrackingStatus {
    pub ahead: u32,
    pub behind: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommitSummary {
    pub sha: SharedString,
    pub subject: SharedString,
    pub commit_timestamp: i64,
    pub author_name: SharedString,
    pub has_parent: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct CommitDetails {
    pub sha: SharedString,
    pub message: SharedString,
    pub commit_timestamp: i64,
    pub author_email: SharedString,
    pub author_name: SharedString,
}

pub trait GitRepository: Send + Sync {
    fn branches(&self) -> BoxFuture<'_, anyhow::Result<BranchesScanResult>>;
    fn show(&self, commit: String) -> BoxFuture<'_, anyhow::Result<CommitDetails>>;
    fn status(&self, path_prefixes: &[RepoPath]) -> Task<anyhow::Result<GitStatus>>;
}

impl fmt::Debug for dyn GitRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("dyn GitRepository<...>").finish()
    }
}

fn normalize_git_metadata_path(path: &Path) -> anyhow::Result<PathBuf> {
    path::normalize_lexically(path).with_context(|| {
        format!(
            "Git metadata path escapes its filesystem root: {}",
            path.display()
        )
    })
}

pub struct SystemGitRepository {
    pub git_dir: PathBuf,
    pub common_dir: PathBuf,
    pub working_directory: PathBuf,
    pub system_git_binary_path: PathBuf,
    executor: BackgroundExecutor,
}

impl SystemGitRepository {
    pub fn new(
        dotgit_path: &Path,
        system_git_binary_path: Option<PathBuf>,
        executor: BackgroundExecutor,
    ) -> anyhow::Result<Self> {
        let system_git_binary_path = system_git_binary_path.context("no git binary available")?;
        log::info!(
            "Opening Git repository at {} using Git binary {}",
            dotgit_path.display(),
            system_git_binary_path.display()
        );

        let dotgit_parent = dotgit_path.parent().context(".git has no parent")?;
        let has_working_directory =
            dotgit_path.is_file() || dotgit_path.file_name() == Some(OsStr::new(".git"));
        if !has_working_directory {
            anyhow::bail!(
                "Git repository has no working directory: {}",
                dotgit_path.display()
            );
        }
        let working_directory = normalize_git_metadata_path(dotgit_parent)?;

        let git_dir = if dotgit_path.is_file() {
            let content =
                std::fs::read_to_string(dotgit_path).context("reading .git worktree file")?;
            let path_str = content
                .strip_prefix("gitdir: ")
                .context("expected .git file to start with 'gitdir: '")?
                .trim();
            let resolved = PathBuf::from(path_str);
            let resolved = if resolved.is_absolute() {
                resolved
            } else {
                dotgit_parent.join(resolved)
            };
            normalize_git_metadata_path(&resolved)?
        } else {
            normalize_git_metadata_path(dotgit_path)?
        };

        let common_dir = {
            let commondir_file = git_dir.join("commondir");
            if commondir_file.is_file() {
                let content =
                    std::fs::read_to_string(&commondir_file).context("reading commondir file")?;
                let path_str = content.trim();
                let resolved = PathBuf::from(path_str);
                let resolved = if resolved.is_absolute() {
                    resolved
                } else {
                    git_dir.join(resolved)
                };
                normalize_git_metadata_path(&resolved)?
            } else {
                git_dir.clone()
            }
        };

        Ok(Self {
            git_dir,
            common_dir,
            working_directory,
            system_git_binary_path,
            executor,
        })
    }

    fn git_binary_in_worktree(&self) -> GitBinary {
        GitBinary::new(
            self.system_git_binary_path.clone(),
            self.working_directory.clone(),
        )
    }
}

impl GitRepository for SystemGitRepository {
    fn branches(&self) -> BoxFuture<'_, anyhow::Result<BranchesScanResult>> {
        let git = self.git_binary_in_worktree();
        self.executor
            .spawn(async move {
                let fields = [
                    "%(HEAD)",
                    "%(objectname)",
                    "%(parent)",
                    "%(refname)",
                    "%(upstream)",
                    "%(upstream:track)",
                    "%(committerdate:unix)",
                    "%(authorname)",
                    "%(contents:subject)",
                ]
                .join("%00");
                let args = vec![
                    "for-each-ref",
                    "refs/heads/**/*",
                    "refs/remotes/**/*",
                    "--format",
                    &fields,
                ];
                let output = git.build_command(&args).output().await?;

                let error = if output.status.success() {
                    None
                } else {
                    let error = format_branch_scan_error(&output);
                    log::warn!("Failed to get Git branches with commit metadata: {error}");
                    Some(error.into())
                };

                let input = String::from_utf8_lossy(&output.stdout);
                let mut branches = parse_branch_input(&input);
                if branches.is_empty() {
                    let output = git
                        .build_command(&["symbolic-ref", "--quiet", "HEAD"])
                        .output()
                        .await?;

                    if output.status.success() {
                        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();

                        branches.push(Branch {
                            ref_name: name.into(),
                            is_head: true,
                            upstream: None,
                            most_recent_commit: None,
                        });
                    }
                }

                Ok(BranchesScanResult { branches, error })
            })
            .boxed()
    }

    fn show(&self, commit: String) -> BoxFuture<'_, anyhow::Result<CommitDetails>> {
        let git = self.git_binary_in_worktree();
        self.executor
            .spawn(async move {
                let output = git
                    .build_command(&[
                        "show",
                        "--no-patch",
                        "--format=%H%x00%B%x00%at%x00%ae%x00%an%x00",
                        &commit,
                    ])
                    .output()
                    .await?;
                let output = std::str::from_utf8(&output.stdout)?;
                let mut fields = output.split('\0');
                let (
                    Some(sha),
                    Some(message),
                    Some(commit_timestamp),
                    Some(author_email),
                    Some(author_name),
                    Some(""),
                    None,
                ) = (
                    fields.next(),
                    fields.next(),
                    fields.next(),
                    fields.next(),
                    fields.next(),
                    fields.next(),
                    fields.next(),
                )
                else {
                    anyhow::bail!("Unexpected git-show output for {commit:?}: {output:?}");
                };
                let sha = sha.to_string().into();
                let message = message.to_string().into();
                let commit_timestamp = commit_timestamp.parse()?;
                let author_email = author_email.to_string().into();
                let author_name = author_name.to_string().into();
                Ok(CommitDetails {
                    sha,
                    message,
                    commit_timestamp,
                    author_email,
                    author_name,
                })
            })
            .boxed()
    }

    fn status(&self, path_prefixes: &[RepoPath]) -> Task<anyhow::Result<GitStatus>> {
        let git = self.git_binary_in_worktree();
        let args = git_status_args(path_prefixes);
        log::debug!("Checking for Git status in {path_prefixes:?}");
        self.executor.spawn(async move {
            let output = git.build_command(&args).output().await?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                stdout.parse()
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("Git status failed: {stderr}");
            }
        })
    }
}

fn parse_branch_input(input: &str) -> Vec<Branch> {
    let mut branches = Vec::new();
    for line in input.split('\n') {
        if line.is_empty() {
            continue;
        }
        let mut fields = line.split('\x00');
        let Some(head) = fields.next() else {
            continue;
        };
        let Some(head_sha) = fields.next().map(|field| field.to_string().into()) else {
            continue;
        };
        let Some(parent_sha) = fields.next().map(|field| field.to_string()) else {
            continue;
        };
        let Some(ref_name) = fields.next().map(|field| field.to_string().into()) else {
            continue;
        };
        let Some(upstream_name) = fields.next().map(|field| field.to_string()) else {
            continue;
        };
        let Some(upstream_tracking) = fields
            .next()
            .and_then(|field| parse_upstream_track(field).ok())
        else {
            continue;
        };
        let Some(committer_date) = fields.next().and_then(|field| field.parse::<i64>().ok()) else {
            continue;
        };
        let Some(author_name) = fields.next().map(|field| field.to_string().into()) else {
            continue;
        };
        let Some(subject) = fields.next().map(|field| field.to_string().into()) else {
            continue;
        };

        branches.push(Branch {
            is_head: head == "*",
            ref_name,
            most_recent_commit: Some(CommitSummary {
                sha: head_sha,
                subject,
                commit_timestamp: committer_date,
                author_name,
                has_parent: !parent_sha.is_empty(),
            }),
            upstream: if upstream_name.is_empty() {
                None
            } else {
                Some(Upstream {
                    ref_name: upstream_name.into(),
                    tracking: upstream_tracking,
                })
            },
        });
    }

    branches
}

fn format_branch_scan_error(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr)
        .trim()
        .replace('\n', " ");
    if stderr.is_empty() {
        format!("Git for-each-ref exited with {}", output.status)
    } else {
        stderr
    }
}

fn parse_upstream_track(upstream_track: &str) -> anyhow::Result<UpstreamTracking> {
    if upstream_track.is_empty() {
        return Ok(UpstreamTracking::Tracked(UpstreamTrackingStatus {
            ahead: 0,
            behind: 0,
        }));
    }

    let upstream_track = upstream_track.strip_prefix("[").context("missing [")?;
    let upstream_track = upstream_track.strip_suffix("]").context("missing ]")?;
    let mut ahead = 0;
    let mut behind = 0;
    for component in upstream_track.split(", ") {
        if component == "gone" {
            return Ok(UpstreamTracking::Gone);
        }
        if let Some(ahead_count) = component.strip_prefix("ahead ") {
            ahead = ahead_count.parse::<u32>()?;
        }
        if let Some(behind_count) = component.strip_prefix("behind ") {
            behind = behind_count.parse::<u32>()?;
        }
    }
    Ok(UpstreamTracking::Tracked(UpstreamTrackingStatus {
        ahead,
        behind,
    }))
}

struct GitBinary {
    git_binary_path: PathBuf,
    working_directory: PathBuf,
}

impl GitBinary {
    fn new(git_binary_path: PathBuf, working_directory: PathBuf) -> Self {
        Self {
            git_binary_path,
            working_directory,
        }
    }

    fn build_command<S>(&self, args: &[S]) -> command::Command
    where
        S: AsRef<OsStr>,
    {
        let mut command = command::new_command(&self.git_binary_path);
        command.current_dir(&self.working_directory);
        command.args(["-c", "core.fsmonitor=false"]);
        command.args(["-c", "log.showSignature=false"]);
        command.arg("--no-optional-locks");
        command.arg("--no-pager");
        command.args(args);

        command
    }
}

fn git_status_args(path_prefixes: &[RepoPath]) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("status"),
        OsString::from("--porcelain=v1"),
        OsString::from("--untracked-files=all"),
        OsString::from("--no-renames"),
        OsString::from("-z"),
        OsString::from("--"),
    ];
    args.extend(path_prefixes.iter().map(|path_prefix| {
        if path_prefix.is_empty() {
            Path::new(".").into()
        } else {
            path_prefix.as_std_path().into()
        }
    }));
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    use gpui::TestAppContext;
    use serde_json::json;

    use fs::TempFs;
    use util_macros::path;

    async fn git_command<I, S>(working_directory: &Path, arguments: I) -> std::process::Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let arguments = arguments.into_iter().collect::<Vec<_>>();
        let git = GitBinary::new(PathBuf::from("git"), working_directory.to_path_buf());
        let mut command = git.build_command(&arguments);
        let output = command
            .env("GIT_CONFIG_GLOBAL", "")
            .env("GIT_CONFIG_SYSTEM", "")
            .env("GIT_AUTHOR_NAME", "test")
            .env("GIT_AUTHOR_EMAIL", "test@zaku.dev")
            .env("GIT_COMMITTER_NAME", "test")
            .env("GIT_COMMITTER_EMAIL", "test@zaku.dev")
            .output()
            .await
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "Git command failed: {stderr}");

        output
    }

    #[test]
    fn test_branches_parsing() {
        #[expect(
            clippy::octal_escapes,
            reason = "Git output uses NUL-delimited fields before SHAs"
        )]
        let input = "*\035248d531f5484bc0cd4755538e6e30de71aff63\0\0refs/heads/main\0refs/remotes/origin/main\0\01770091759\0Mayank Verma\0Setup project files\n";
        assert_eq!(
            parse_branch_input(input),
            vec![Branch {
                is_head: true,
                ref_name: "refs/heads/main".into(),
                upstream: Some(Upstream {
                    ref_name: "refs/remotes/origin/main".into(),
                    tracking: UpstreamTracking::Tracked(UpstreamTrackingStatus {
                        ahead: 0,
                        behind: 0,
                    }),
                }),
                most_recent_commit: Some(CommitSummary {
                    sha: "35248d531f5484bc0cd4755538e6e30de71aff63".into(),
                    subject: "Setup project files".into(),
                    commit_timestamp: 1770091759,
                    author_name: SharedString::new_static("Mayank Verma"),
                    has_parent: false,
                }),
            }]
        );
    }

    #[test]
    fn test_branches_parsing_containing_refs_with_missing_fields() {
        #[expect(
            clippy::octal_escapes,
            reason = "Git output uses NUL-delimited fields before SHAs"
        )]
        let input = " \090012116c03db04344ab10d50348553aa94f1ea0\0refs/heads/broken\n \0668e059e269848c7449093c2481169c89b7b0d40\035248d531f5484bc0cd4755538e6e30de71aff63\0refs/heads/dev\0\0\01770112670\0Mayank Verma\0zaku: Initial setup with basic GPUI window\n*\035248d531f5484bc0cd4755538e6e30de71aff63\0\0refs/heads/main\0\0\01770091759\0Mayank Verma\0Setup project files\n";

        let branches = parse_branch_input(input);
        assert_eq!(branches.len(), 2);
        assert_eq!(
            branches,
            vec![
                Branch {
                    is_head: false,
                    ref_name: "refs/heads/dev".into(),
                    upstream: None,
                    most_recent_commit: Some(CommitSummary {
                        sha: "668e059e269848c7449093c2481169c89b7b0d40".into(),
                        subject: "zaku: Initial setup with basic GPUI window".into(),
                        commit_timestamp: 1770112670,
                        author_name: SharedString::new_static("Mayank Verma"),
                        has_parent: true,
                    }),
                },
                Branch {
                    is_head: true,
                    ref_name: "refs/heads/main".into(),
                    upstream: None,
                    most_recent_commit: Some(CommitSummary {
                        sha: "35248d531f5484bc0cd4755538e6e30de71aff63".into(),
                        subject: "Setup project files".into(),
                        commit_timestamp: 1770091759,
                        author_name: SharedString::new_static("Mayank Verma"),
                        has_parent: false,
                    }),
                },
            ]
        );
    }

    #[gpui::test]
    async fn test_system_git_repository_new_resolves_normal_repository_paths(
        cx: &mut TestAppContext,
    ) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        temp_fs.insert_tree(path!("repo"), json!({}));
        let repository_dir = temp_fs.path().join(path!("repo"));
        git_command(&repository_dir, ["init", "-b", "main"]).await;
        let dotgit_path = repository_dir.join(path!(".git"));

        let repository =
            SystemGitRepository::new(&dotgit_path, Some("git".into()), cx.executor()).unwrap();

        assert_eq!(
            std::fs::canonicalize(&repository.git_dir).unwrap(),
            std::fs::canonicalize(&dotgit_path).unwrap()
        );
        assert_eq!(
            std::fs::canonicalize(&repository.common_dir).unwrap(),
            std::fs::canonicalize(&dotgit_path).unwrap()
        );
        assert_eq!(
            std::fs::canonicalize(&repository.working_directory).unwrap(),
            std::fs::canonicalize(&repository_dir).unwrap()
        );
    }

    #[gpui::test]
    async fn test_system_git_repository_new_resolves_linked_worktree_paths(
        cx: &mut TestAppContext,
    ) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let repository_dir = temp_fs.path().join(path!("repo"));
        let worktree_dir = temp_fs.path().join(path!("worktree"));
        temp_fs.insert_tree(path!("repo"), json!({}));

        git_command(&repository_dir, ["init", "-b", "main"]).await;

        temp_fs.insert_tree(path!("repo"), json!({ "README.md": "# Repo" }));
        git_command(&repository_dir, ["add", "README.md"]).await;
        git_command(&repository_dir, ["commit", "-m", "Initial commit"]).await;

        let arguments = [
            OsStr::new("worktree"),
            OsStr::new("add"),
            OsStr::new("-b"),
            OsStr::new("feature"),
            worktree_dir.as_os_str(),
        ];
        git_command(&repository_dir, arguments).await;

        let git_dir = repository_dir.join(path!(".git"));
        let linked_git_dir = git_dir.join(path!("worktrees/worktree"));
        let repository = SystemGitRepository::new(
            &worktree_dir.join(path!(".git")),
            Some("git".into()),
            cx.executor(),
        )
        .unwrap();

        assert_eq!(
            std::fs::canonicalize(&repository.working_directory).unwrap(),
            std::fs::canonicalize(&worktree_dir).unwrap()
        );
        assert_eq!(
            std::fs::canonicalize(&repository.git_dir).unwrap(),
            std::fs::canonicalize(&linked_git_dir).unwrap()
        );
        assert_eq!(
            std::fs::canonicalize(&repository.common_dir).unwrap(),
            std::fs::canonicalize(&git_dir).unwrap()
        );
    }

    #[gpui::test]
    async fn test_system_git_repository_new_rejects_bare_repositories(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let repository_dir = temp_fs.path().join(path!("repo.git"));
        let arguments = [
            OsStr::new("init"),
            OsStr::new("--bare"),
            repository_dir.as_os_str(),
        ];
        git_command(temp_fs.path(), arguments).await;

        let Err(error) =
            SystemGitRepository::new(&repository_dir, Some("git".into()), cx.executor())
        else {
            panic!("Bare repository should be rejected");
        };

        assert!(
            error
                .to_string()
                .contains("Git repository has no working directory")
        );
    }

    #[gpui::test]
    fn test_system_git_repository_new_rejects_malformed_git_file(cx: &mut TestAppContext) {
        let temp_fs = TempFs::new(cx.executor());
        temp_fs.insert_tree(
            path!("worktree"),
            json!({
                ".git": "not a gitdir file\n",
            }),
        );
        let worktree_dir = temp_fs.path().join(path!("worktree"));

        let Err(error) = SystemGitRepository::new(
            &worktree_dir.join(path!(".git")),
            Some("git".into()),
            cx.executor(),
        ) else {
            panic!("Malformed .git file should be rejected");
        };

        assert!(
            error
                .to_string()
                .contains("expected .git file to start with 'gitdir: '"),
            "Unexpected error: {error:#}"
        );
    }
}
