pub mod fs_watcher;

use anyhow::Context;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use gpui::BackgroundExecutor;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

#[cfg(feature = "test-support")]
use serde_json::Value;

use std::{
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use util::{ResultExt, path::SanitizedPath};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

#[cfg(target_os = "windows")]
use std::{mem::MaybeUninit, os::windows::io::AsRawHandle, path::Component};

#[cfg(target_os = "windows")]
use smol::fs::windows::OpenOptionsExt;

#[cfg(target_os = "windows")]
use windows::Win32::{
    Foundation::HANDLE,
    Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS, GetFileInformationByHandle,
    },
};

#[cfg(feature = "test-support")]
use tempfile::TempDir;

use crate::fs_watcher::FsWatcher;

pub trait Watcher: Send + Sync {
    fn add(&self, path: &Path) -> anyhow::Result<()>;
    fn remove(&self, path: &Path) -> anyhow::Result<()>;
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum PathEventKind {
    Removed,
    Created,
    Changed,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct PathEvent {
    pub path: PathBuf,
    pub kind: Option<PathEventKind>,
}

impl From<PathEvent> for PathBuf {
    fn from(event: PathEvent) -> Self {
        event.path
    }
}

#[async_trait]
pub trait Fs: Send + Sync {
    async fn canonicalize(&self, path: &Path) -> anyhow::Result<PathBuf>;
    async fn metadata(&self, path: &Path) -> anyhow::Result<Option<Metadata>>;
    async fn load(&self, path: &Path) -> anyhow::Result<String>;
    async fn read_link(&self, path: &Path) -> anyhow::Result<PathBuf>;
    async fn read_dir(
        &self,
        path: &Path,
    ) -> anyhow::Result<Pin<Box<dyn Send + Stream<Item = anyhow::Result<PathBuf>>>>>;
    async fn watch(
        &self,
        path: &Path,
        latency: Duration,
    ) -> (
        Pin<Box<dyn Send + Stream<Item = Vec<PathEvent>>>>,
        Arc<dyn Watcher>,
    );
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(transparent)]
pub struct MTime(SystemTime);

impl MTime {
    pub fn from_seconds_and_nanos(secs: u64, nanos: u32) -> Self {
        Self(UNIX_EPOCH + Duration::new(secs, nanos))
    }

    pub fn to_seconds_and_nanos(self) -> Option<(u64, u32)> {
        self.0
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|duration| (duration.as_secs(), duration.subsec_nanos()))
    }

    pub fn timestamp_for_user(self) -> SystemTime {
        self.0
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Metadata {
    pub inode: u64,
    pub mtime: MTime,
    pub is_symlink: bool,
    pub is_dir: bool,
    pub len: u64,
}

pub struct NativeFs {
    executor: BackgroundExecutor,
}

impl NativeFs {
    pub fn new(executor: BackgroundExecutor) -> Self {
        Self { executor }
    }

    #[cfg(target_os = "windows")]
    fn canonicalize(path: &Path) -> anyhow::Result<PathBuf> {
        let mut strip_prefix = None;

        let mut new_path = PathBuf::new();
        for component in path.components() {
            match component {
                Component::Prefix(_) => {
                    let component = component.as_os_str();
                    let canonicalized = if component
                        .to_str()
                        .map(|component| component.ends_with("\\"))
                        .unwrap_or(false)
                    {
                        std::fs::canonicalize(component)
                    } else {
                        let mut component = component.to_os_string();
                        component.push("\\");
                        std::fs::canonicalize(component)
                    }?;

                    let mut strip = PathBuf::new();
                    for component in canonicalized.components() {
                        match component {
                            Component::Prefix(prefix_component) => {
                                match prefix_component.kind() {
                                    std::path::Prefix::Verbatim(os_str) => {
                                        strip.push(os_str);
                                    }
                                    std::path::Prefix::VerbatimUNC(host, share) => {
                                        strip.push("\\\\");
                                        strip.push(host);
                                        strip.push(share);
                                    }
                                    std::path::Prefix::VerbatimDisk(disk) => {
                                        strip.push(format!("{}:", disk as char));
                                    }
                                    _ => strip.push(component),
                                };
                            }
                            _ => strip.push(component),
                        }
                    }
                    strip_prefix = Some(strip);
                    new_path.push(component);
                }
                Component::RootDir => {
                    new_path.push(component);
                }
                Component::CurDir => {
                    if strip_prefix.is_none() {
                        new_path.push(component);
                    }
                }
                Component::ParentDir => {
                    if strip_prefix.is_some() {
                        new_path.pop();
                    } else {
                        new_path.push(component);
                    }
                }
                Component::Normal(_) => {
                    if let Ok(link) = std::fs::read_link(new_path.join(component)) {
                        let link = match &strip_prefix {
                            Some(prefix) => link.strip_prefix(prefix).unwrap_or(&link),
                            None => &link,
                        };
                        new_path.extend(link);
                    } else {
                        new_path.push(component);
                    }
                }
            }
        }

        Ok(new_path)
    }
}

#[async_trait]
impl Fs for NativeFs {
    async fn canonicalize(&self, path: &Path) -> anyhow::Result<PathBuf> {
        let path = path.to_path_buf();
        self.executor
            .spawn(async move {
                #[cfg(target_os = "windows")]
                let result = Self::canonicalize(&path);

                #[cfg(not(target_os = "windows"))]
                let result = std::fs::canonicalize(&path);

                result.with_context(|| format!("failed to canonicalize path {}", path.display()))
            })
            .await
    }

    async fn metadata(&self, path: &Path) -> anyhow::Result<Option<Metadata>> {
        let path = path.to_path_buf();
        let Some((metadata, is_symlink)) = self
            .executor
            .spawn({
                let path = path.clone();
                async move { metadata_for_path(&path) }
            })
            .await?
        else {
            return Ok(None);
        };

        #[cfg(unix)]
        let inode = metadata.ino();

        #[cfg(target_os = "windows")]
        let inode = file_id(&path).await?;

        #[cfg(not(any(unix, target_os = "windows")))]
        let inode = 0;

        Ok(Some(Metadata {
            inode,
            mtime: MTime(metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH)),
            is_symlink,
            is_dir: metadata.file_type().is_dir(),
            len: metadata.len(),
        }))
    }

    async fn load(&self, path: &Path) -> anyhow::Result<String> {
        let path = path.to_path_buf();
        self.executor
            .spawn(async move {
                std::fs::read_to_string(&path)
                    .with_context(|| format!("failed to read file {}", path.display()))
            })
            .await
    }

    async fn read_dir(
        &self,
        path: &Path,
    ) -> anyhow::Result<Pin<Box<dyn Send + Stream<Item = anyhow::Result<PathBuf>>>>> {
        let path = path.to_path_buf();
        let result = futures::stream::iter(
            self.executor
                .spawn(async move {
                    std::fs::read_dir(&path)
                        .with_context(|| format!("failed to read directory {}", path.display()))
                })
                .await?,
        )
        .map(|entry| match entry {
            Ok(entry) => Ok(entry.path()),
            Err(error) => Err(anyhow::anyhow!("failed to read dir entry {error:?}")),
        });
        Ok(Box::pin(result))
    }

    async fn read_link(&self, path: &Path) -> anyhow::Result<PathBuf> {
        let path = path.to_path_buf();
        let path = self
            .executor
            .spawn(async move {
                std::fs::read_link(&path)
                    .with_context(|| format!("failed to read symbolic link {}", path.display()))
            })
            .await?;
        Ok(path)
    }

    async fn watch(
        &self,
        path: &Path,
        latency: Duration,
    ) -> (
        Pin<Box<dyn Send + Stream<Item = Vec<PathEvent>>>>,
        Arc<dyn Watcher>,
    ) {
        let executor = self.executor.clone();
        let (tx, rx) = smol::channel::unbounded();
        let pending_paths: Arc<Mutex<Vec<PathEvent>>> = Default::default();
        let watcher = Arc::new(FsWatcher::new(tx, pending_paths.clone()));

        if let Err(error) = watcher.add(path)
            && let Some(parent) = path.parent()
            && let Err(parent_error) = watcher.add(parent)
        {
            log::warn!(
                "Failed to watch {} and its parent directory {}:\n{error}\n{parent_error}",
                path.display(),
                parent.display(),
            );
        }

        if let Some(mut target) = self.read_link(path).await.ok() {
            log::trace!("Watching symlink target {path:?} -> {target:?}");

            if target.is_relative()
                && let Some(parent) = path.parent()
            {
                target = parent.join(target);

                if let Ok(canonical) = self.canonicalize(&target).await {
                    target = SanitizedPath::new(&canonical).as_path().to_path_buf();
                }
            }

            watcher.add(&target).ok();

            if let Some(parent) = target.parent() {
                watcher.add(parent).log_err();
            }
        }

        (
            Box::pin(rx.filter_map({
                let watcher = watcher.clone();
                let executor = executor.clone();

                move |_| {
                    let _ = watcher.clone();
                    let pending_paths = pending_paths.clone();
                    let executor = executor.clone();

                    async move {
                        executor.timer(latency).await;
                        let paths = std::mem::take(&mut *pending_paths.lock());
                        (!paths.is_empty()).then_some(paths)
                    }
                }
            })),
            watcher,
        )
    }
}

#[cfg(feature = "test-support")]
pub struct TempFs {
    _temp_dir: TempDir,
    path: PathBuf,
    executor: BackgroundExecutor,
}

#[cfg(feature = "test-support")]
impl TempFs {
    pub fn new(executor: BackgroundExecutor) -> Self {
        let temp_dir = TempDir::new().unwrap();
        let path = std::fs::canonicalize(temp_dir.path()).unwrap();

        Self {
            _temp_dir: temp_dir,
            path,
            executor,
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
        NativeFs::new(self.executor.clone())
            .canonicalize(&absolute_path)
            .await
    }

    async fn metadata(&self, path: &Path) -> anyhow::Result<Option<Metadata>> {
        let absolute_path = resolve_path(self.path(), path);
        NativeFs::new(self.executor.clone())
            .metadata(&absolute_path)
            .await
    }

    async fn load(&self, path: &Path) -> anyhow::Result<String> {
        let absolute_path = resolve_path(self.path(), path);
        NativeFs::new(self.executor.clone())
            .load(&absolute_path)
            .await
    }

    async fn read_dir(
        &self,
        path: &Path,
    ) -> anyhow::Result<Pin<Box<dyn Send + Stream<Item = anyhow::Result<PathBuf>>>>> {
        let absolute_path = resolve_path(self.path(), path);
        NativeFs::new(self.executor.clone())
            .read_dir(&absolute_path)
            .await
    }

    async fn read_link(&self, path: &Path) -> anyhow::Result<PathBuf> {
        let absolute_path = resolve_path(self.path(), path);
        NativeFs::new(self.executor.clone())
            .read_link(&absolute_path)
            .await
    }

    async fn watch(
        &self,
        path: &Path,
        latency: Duration,
    ) -> (
        Pin<Box<dyn Send + Stream<Item = Vec<PathEvent>>>>,
        Arc<dyn Watcher>,
    ) {
        let absolute_path = resolve_path(self.path(), path);
        NativeFs::new(self.executor.clone())
            .watch(&absolute_path, latency)
            .await
    }
}

fn metadata_for_path(path: &Path) -> anyhow::Result<Option<(std::fs::Metadata, bool)>> {
    let symlink_metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound
                || error.kind() == std::io::ErrorKind::NotADirectory =>
        {
            return Ok(None);
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to read metadata for {}", path.display()));
        }
    };

    let is_symlink = symlink_metadata.file_type().is_symlink();
    let metadata = if is_symlink {
        match std::fs::metadata(path) {
            Ok(target_metadata) => target_metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => symlink_metadata,
            Err(error) => {
                log::warn!(
                    "Failed to read symlink target metadata for path {}: {error}",
                    path.display(),
                );
                symlink_metadata
            }
        }
    } else {
        symlink_metadata
    };

    Ok(Some((metadata, is_symlink)))
}

#[cfg(feature = "test-support")]
fn resolve_path(root: &Path, path: &Path) -> PathBuf {
    if !path.is_absolute() {
        return root.join(path);
    }

    path.to_path_buf()
}

#[cfg(target_os = "windows")]
async fn file_id(path: impl AsRef<Path>) -> anyhow::Result<u64> {
    let file = smol::fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0)
        .open(path)
        .await?;

    smol::unblock(move || {
        let mut info = MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();

        // Safety: `file` stays alive for the duration of this call, so the raw handle remains valid,
        // and `info.as_mut_ptr()` points to writable storage for `BY_HANDLE_FILE_INFORMATION`.
        unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), info.as_mut_ptr())? };

        // Safety: A successful `GetFileInformationByHandle` call above guarantees
        // that the output buffer was filled.
        let info = unsafe { info.assume_init() };

        Ok(((info.nFileIndexHigh as u64) << 32) | (info.nFileIndexLow as u64))
    })
    .await
}
