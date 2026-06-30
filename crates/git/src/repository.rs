use std::{ffi::OsString, fmt, ops, path::Path, sync::Arc};

use path::{PathStyle, RelPath};

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

pub fn git_status_args(path_prefixes: &[RepoPath]) -> Vec<OsString> {
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
