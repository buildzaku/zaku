use anyhow::{Context, bail};
use gpui::{BackgroundExecutor, Task};
use std::{
    ffi::{OsStr, OsString},
    fmt, ops,
    path::{Path, PathBuf},
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

pub trait GitRepository: Send + Sync {
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
    pub working_directory: Option<PathBuf>,
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
        let working_directory = if has_working_directory {
            Some(normalize_git_metadata_path(dotgit_parent)?)
        } else {
            None
        };

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

    fn working_directory(&self) -> anyhow::Result<PathBuf> {
        self.working_directory
            .clone()
            .context("Git repository has no working directory")
    }

    fn git_binary_in_worktree(&self) -> anyhow::Result<GitBinary> {
        Ok(GitBinary::new(
            self.system_git_binary_path.clone(),
            self.working_directory()?,
        ))
    }
}

impl GitRepository for SystemGitRepository {
    fn status(&self, path_prefixes: &[RepoPath]) -> Task<anyhow::Result<GitStatus>> {
        let git = self.git_binary_in_worktree();
        let args = git_status_args(path_prefixes);
        log::debug!("Checking for Git status in {path_prefixes:?}");
        self.executor.spawn(async move {
            let git = git?;
            let output = git.build_command(&args).output().await?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                stdout.parse()
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                bail!("Git status failed: {stderr}");
            }
        })
    }
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
            std::fs::canonicalize(repository.working_directory.as_ref().unwrap()).unwrap(),
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
            std::fs::canonicalize(repository.working_directory.as_ref().unwrap()).unwrap(),
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
    async fn test_system_git_repository_new_supports_bare_repositories(cx: &mut TestAppContext) {
        cx.executor().allow_parking();

        let temp_fs = TempFs::new(cx.executor());
        let repository_dir = temp_fs.path().join(path!("repo.git"));
        let arguments = [
            OsStr::new("init"),
            OsStr::new("--bare"),
            repository_dir.as_os_str(),
        ];
        git_command(temp_fs.path(), arguments).await;

        let repository =
            SystemGitRepository::new(&repository_dir, Some("git".into()), cx.executor()).unwrap();

        assert_eq!(
            std::fs::canonicalize(&repository.git_dir).unwrap(),
            std::fs::canonicalize(&repository_dir).unwrap()
        );
        assert_eq!(
            std::fs::canonicalize(&repository.common_dir).unwrap(),
            std::fs::canonicalize(&repository_dir).unwrap()
        );
        assert_eq!(repository.working_directory, None);

        let output = git_command(&repository_dir, ["rev-parse", "--is-bare-repository"]).await;
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(stdout.as_ref(), "true\n");
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
