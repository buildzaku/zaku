use ignore::gitignore::Gitignore;
use std::{ffi::OsStr, path::Path, sync::Arc};

#[derive(Debug, Clone)]
pub(crate) struct IgnoreStack {
    pub(crate) repo_root: Option<Arc<Path>>,
    pub(crate) top: Arc<IgnoreStackEntry>,
}

impl IgnoreStack {
    pub(crate) fn none() -> Self {
        Self {
            repo_root: None,
            top: Arc::new(IgnoreStackEntry::None),
        }
    }

    pub(crate) fn all() -> Self {
        Self {
            repo_root: None,
            top: Arc::new(IgnoreStackEntry::All),
        }
    }

    pub(crate) fn append(self, kind: IgnoreKind, ignore: Arc<Gitignore>) -> Self {
        let top = match self.top.as_ref() {
            IgnoreStackEntry::All => self.top.clone(),
            _ => Arc::new(match kind {
                IgnoreKind::Gitignore(abs_base_path) => IgnoreStackEntry::Some {
                    abs_base_path,
                    ignore,
                    parent: self.top.clone(),
                },
            }),
        };
        Self {
            repo_root: self.repo_root,
            top,
        }
    }

    pub(crate) fn is_abs_path_ignored(&self, abs_path: &Path, is_dir: bool) -> bool {
        if is_dir && abs_path.file_name() == Some(OsStr::new(".git")) {
            return true;
        }

        match self.top.as_ref() {
            IgnoreStackEntry::None => false,
            IgnoreStackEntry::All => true,
            IgnoreStackEntry::Some {
                abs_base_path,
                ignore,
                parent: prev,
            } => match ignore.matched(
                abs_path
                    .strip_prefix(abs_base_path)
                    .expect("ignore base path should be a parent of matched path"),
                is_dir,
            ) {
                ignore::Match::None => IgnoreStack {
                    repo_root: self.repo_root.clone(),
                    top: prev.clone(),
                }
                .is_abs_path_ignored(abs_path, is_dir),
                ignore::Match::Ignore(_) => true,
                ignore::Match::Whitelist(_) => false,
            },
        }
    }
}

#[derive(Debug)]
pub(crate) enum IgnoreStackEntry {
    None,
    Some {
        abs_base_path: Arc<Path>,
        ignore: Arc<Gitignore>,
        parent: Arc<IgnoreStackEntry>,
    },
    All,
}

#[derive(Debug)]
pub(crate) enum IgnoreKind {
    Gitignore(Arc<Path>),
}
