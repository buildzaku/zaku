use anyhow::Context;
use async_trait::async_trait;

#[cfg(feature = "test-support")]
use serde_json::Value;

use std::path::{Path, PathBuf};

#[cfg(feature = "test-support")]
use std::sync::Arc;

#[cfg(feature = "test-support")]
use tempfile::TempDir;

#[async_trait]
pub trait Fs: Send + Sync {
    async fn canonicalize(&self, path: &Path) -> anyhow::Result<PathBuf>;
    async fn metadata(&self, path: &Path) -> anyhow::Result<Option<Metadata>>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Metadata {
    pub is_dir: bool,
}

#[derive(Default)]
pub struct NativeFs;

impl NativeFs {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Fs for NativeFs {
    async fn canonicalize(&self, path: &Path) -> anyhow::Result<PathBuf> {
        smol::fs::canonicalize(path)
            .await
            .with_context(|| format!("failed to canonicalize path {}", path.display()))
    }

    async fn metadata(&self, path: &Path) -> anyhow::Result<Option<Metadata>> {
        match smol::fs::metadata(path).await {
            Ok(metadata) => Ok(Some(Metadata {
                is_dir: metadata.is_dir(),
            })),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error)
                .with_context(|| format!("failed to read metadata for {}", path.display())),
        }
    }
}

#[cfg(feature = "test-support")]
pub struct TempFs {
    _temp_dir: TempDir,
    path: PathBuf,
}

#[cfg(feature = "test-support")]
impl TempFs {
    pub fn new() -> Self {
        let temp_dir = TempDir::new().unwrap();
        let path = std::fs::canonicalize(temp_dir.path()).unwrap();

        Self {
            _temp_dir: temp_dir,
            path,
        }
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    pub fn insert_tree(&self, path: impl AsRef<Path>, tree: Value) {
        fn inner(directory: &Path, path: Arc<Path>, tree: Value) {
            match tree {
                Value::Object(map) => {
                    let absolute_path = resolve_path(directory, path.as_ref());
                    std::fs::create_dir_all(&absolute_path).unwrap();
                    for (name, contents) in map {
                        let mut new_path = PathBuf::from(path.as_ref());
                        new_path.push(name);
                        inner(directory, Arc::from(new_path), contents);
                    }
                }
                Value::Null => {
                    let absolute_path = resolve_path(directory, path.as_ref());
                    std::fs::create_dir_all(&absolute_path).unwrap();
                }
                Value::String(contents) => {
                    let absolute_path = resolve_path(directory, path.as_ref());
                    if let Some(parent) = absolute_path.parent() {
                        std::fs::create_dir_all(parent).unwrap();
                    }
                    std::fs::write(&absolute_path, contents.as_bytes()).unwrap();
                }
                _ => {
                    panic!("JSON object must contain only objects, strings, or null");
                }
            }
        }

        inner(self.path(), Arc::from(path.as_ref()), tree)
    }
}

#[cfg(feature = "test-support")]
#[async_trait]
impl Fs for TempFs {
    async fn canonicalize(&self, path: &Path) -> anyhow::Result<PathBuf> {
        let absolute_path = resolve_path(self.path(), path);
        std::fs::canonicalize(&absolute_path)
            .with_context(|| format!("failed to canonicalize path {}", absolute_path.display()))
    }

    async fn metadata(&self, path: &Path) -> anyhow::Result<Option<Metadata>> {
        let absolute_path = resolve_path(self.path(), path);
        match std::fs::metadata(&absolute_path) {
            Ok(metadata) => Ok(Some(Metadata {
                is_dir: metadata.is_dir(),
            })),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error).with_context(|| {
                format!("failed to read metadata for {}", absolute_path.display())
            }),
        }
    }
}

#[cfg(feature = "test-support")]
fn resolve_path(root: &Path, path: &Path) -> PathBuf {
    if !path.is_absolute() {
        return root.join(path);
    }

    path.to_path_buf()
}
