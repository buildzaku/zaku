pub mod fs_watcher;

use anyhow::Context;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use gpui::BackgroundExecutor;
use is_executable::IsExecutable;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tempfile::TempDir;

#[cfg(feature = "test-support")]
use serde_json::Value;

use std::{
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use std::ffi::CString;

#[cfg(target_os = "windows")]
use util::command::new_command;

use util::{ResultExt, path::SanitizedPath};

#[cfg(unix)]
use std::os::fd::{AsFd, AsRawFd};

#[cfg(target_os = "macos")]
use std::ffi::{CStr, OsStr};

#[cfg(unix)]
use std::os::unix::fs::{FileTypeExt, MetadataExt};

#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;

#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::mem::MaybeUninit;

#[cfg(target_os = "windows")]
use smol::fs::windows::OpenOptionsExt as SmolOpenOptionsExt;

#[cfg(target_os = "windows")]
use windows::{
    Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS, FILE_NAME_NORMALIZED,
            GetFileInformationByHandle, GetFinalPathNameByHandleW, GetVolumePathNameW,
            MOVE_FILE_FLAGS, MoveFileExW,
        },
    },
    core::{HSTRING, PCWSTR},
};

#[cfg(target_os = "windows")]
use std::{
    ffi::OsString,
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        fs::OpenOptionsExt,
        io::AsRawHandle,
    },
};

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
    Rescan,
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

#[derive(Copy, Clone, Default)]
pub struct RenameOptions {
    pub overwrite: bool,
    pub ignore_if_exists: bool,
    pub create_parents: bool,
}

#[derive(Copy, Clone, Default)]
pub struct RemoveOptions {
    pub recursive: bool,
    pub ignore_if_not_exists: bool,
}

#[async_trait]
pub trait Fs: Send + Sync {
    async fn create_dir(&self, path: &Path) -> anyhow::Result<()>;
    async fn create_symlink(&self, path: &Path, target: PathBuf) -> anyhow::Result<()>;
    async fn canonicalize(&self, path: &Path) -> anyhow::Result<PathBuf>;
    async fn metadata(&self, path: &Path) -> anyhow::Result<Option<Metadata>>;
    async fn load(&self, path: &Path) -> anyhow::Result<String>;
    async fn open_handle(&self, path: &Path) -> anyhow::Result<Arc<dyn FileHandle>>;
    async fn read_link(&self, path: &Path) -> anyhow::Result<PathBuf>;
    async fn read_dir(
        &self,
        path: &Path,
    ) -> anyhow::Result<Pin<Box<dyn Send + Stream<Item = anyhow::Result<PathBuf>>>>>;
    async fn rename(
        &self,
        source: &Path,
        target: &Path,
        options: RenameOptions,
    ) -> anyhow::Result<()>;
    async fn remove_dir(&self, path: &Path, options: RemoveOptions) -> anyhow::Result<()>;
    async fn remove_file(&self, path: &Path, options: RemoveOptions) -> anyhow::Result<()>;
    async fn watch(
        &self,
        path: &Path,
        latency: Duration,
    ) -> (
        Pin<Box<dyn Send + Stream<Item = Vec<PathEvent>>>>,
        Arc<dyn Watcher>,
    );
    async fn write(&self, path: &Path, content: &[u8]) -> anyhow::Result<()>;
    async fn is_case_sensitive(&self) -> bool;
}

pub trait FileHandle: Send + Sync + std::fmt::Debug {
    fn current_path(&self, fs: &Arc<dyn Fs>) -> anyhow::Result<PathBuf>;
}

impl FileHandle for std::fs::File {
    #[cfg(target_os = "macos")]
    fn current_path(&self, _: &Arc<dyn Fs>) -> anyhow::Result<PathBuf> {
        let fd = self.as_fd();
        let mut path_buf = MaybeUninit::<[u8; libc::PATH_MAX as usize]>::uninit();

        // Safety: `fd` remains valid for the duration of this call and `path_buf`
        // provides writable `PATH_MAX`-sized storage for the kernel to fill.
        let result = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETPATH, path_buf.as_mut_ptr()) };

        anyhow::ensure!(result != -1, "fcntl returned -1");

        // Safety: Successful `libc::fcntl()` call above populates `path_buf` with
        // a valid C string.
        let c_str = unsafe { CStr::from_ptr(path_buf.as_ptr().cast()) };

        anyhow::ensure!(
            !c_str.is_empty(),
            "could not find a path for the file handle"
        );
        Ok(PathBuf::from(OsStr::from_bytes(c_str.to_bytes())))
    }

    #[cfg(target_os = "linux")]
    fn current_path(&self, _: &Arc<dyn Fs>) -> anyhow::Result<PathBuf> {
        let fd = self.as_fd();
        let fd_path = format!("/proc/self/fd/{}", fd.as_raw_fd());
        let new_path = std::fs::read_link(fd_path)?;
        if new_path
            .file_name()
            .is_some_and(|file_name| file_name.to_string_lossy().ends_with(" (deleted)"))
        {
            anyhow::bail!("file was deleted");
        }

        Ok(new_path)
    }

    #[cfg(target_os = "windows")]
    fn current_path(&self, _: &Arc<dyn Fs>) -> anyhow::Result<PathBuf> {
        let handle = HANDLE(self.as_raw_handle());

        // Safety: `handle` remains valid for the duration of this call and the empty
        // buffer is used to query the required path length.
        let required_len =
            unsafe { GetFinalPathNameByHandleW(handle, &mut [], FILE_NAME_NORMALIZED) };

        anyhow::ensure!(
            required_len != 0,
            "GetFinalPathNameByHandleW returned 0 length"
        );

        let mut buf = vec![0u16; required_len as usize + 1];

        // Safety: `handle` remains valid for the duration of this call and `buf`
        // provides writable storage for the returned UTF-16 path.
        let written = unsafe { GetFinalPathNameByHandleW(handle, &mut buf, FILE_NAME_NORMALIZED) };

        anyhow::ensure!(
            written != 0,
            "GetFinalPathNameByHandleW failed to write path"
        );

        let os_str = OsString::from_wide(&buf[..written as usize]);
        anyhow::ensure!(
            !os_str.is_empty(),
            "could not find a path for the file handle"
        );
        Ok(PathBuf::from(os_str))
    }
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
    pub is_fifo: bool,
    pub is_executable: bool,
}

pub struct NativeFs {
    executor: BackgroundExecutor,
    is_case_sensitive: AtomicU8,
}

impl NativeFs {
    pub fn new(executor: BackgroundExecutor) -> Self {
        Self {
            executor,
            is_case_sensitive: AtomicU8::new(0),
        }
    }

    fn canonicalize(path: &Path) -> anyhow::Result<PathBuf> {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            std::fs::canonicalize(path).map_err(Into::into)
        }

        #[cfg(target_os = "windows")]
        {
            let abs_path = if path.is_relative() {
                std::env::current_dir()?.join(path)
            } else {
                path.to_path_buf()
            };

            let path_hstring = HSTRING::from(abs_path.as_os_str());
            let mut volume_buffer = vec![0u16; abs_path.as_os_str().len() + 2];

            // Safety: `path_hstring` remains valid for the duration of this call and
            // `volume_buffer` provides writable storage for the returned UTF-16 volume root.
            unsafe { GetVolumePathNameW(&path_hstring, &mut volume_buffer)? };

            let volume_root = {
                let len = volume_buffer
                    .iter()
                    .position(|&character| character == 0)
                    .unwrap_or(volume_buffer.len());
                PathBuf::from(OsString::from_wide(&volume_buffer[..len]))
            };

            let resolved_path = dunce::canonicalize(&abs_path)?;
            let resolved_root = dunce::canonicalize(&volume_root)?;

            if let Ok(relative) = resolved_path.strip_prefix(&resolved_root) {
                let mut result = volume_root;
                result.push(relative);
                Ok(result)
            } else {
                Ok(resolved_path)
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn rename_without_replace(source: &Path, target: &Path) -> std::io::Result<()> {
    let source = path_to_c_string(source)?;
    let target = path_to_c_string(target)?;

    // Safety: `source` and `target` remain valid NUL-terminated C strings for the
    // duration of this call.
    let result = unsafe { libc::renamex_np(source.as_ptr(), target.as_ptr(), libc::RENAME_EXCL) };

    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(target_os = "linux")]
fn rename_without_replace(source: &Path, target: &Path) -> std::io::Result<()> {
    let source = path_to_c_string(source)?;
    let target = path_to_c_string(target)?;

    // Safety: `source` and `target` remain valid NUL-terminated C strings for the
    // duration of this call.
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            target.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };

    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(target_os = "windows")]
fn rename_without_replace(source: &Path, target: &Path) -> std::io::Result<()> {
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();

    // Safety: `source` and `target` remain valid NUL-terminated UTF-16 strings for
    // the duration of this call.
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(target.as_ptr()),
            MOVE_FILE_FLAGS::default(),
        )
    }
    .map_err(|_| std::io::Error::last_os_error())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn path_to_c_string(path: &Path) -> std::io::Result<CString> {
    CString::new(path.as_os_str().as_bytes()).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("path contains interior NUL: {}", path.display()),
        )
    })
}

#[async_trait]
impl Fs for NativeFs {
    async fn create_dir(&self, path: &Path) -> anyhow::Result<()> {
        Ok(smol::fs::create_dir_all(path).await?)
    }

    async fn create_symlink(&self, path: &Path, target: PathBuf) -> anyhow::Result<()> {
        #[cfg(unix)]
        {
            smol::fs::unix::symlink(target, path).await?;
        }

        #[cfg(target_os = "windows")]
        {
            let resolved_target = if target.is_relative() {
                path.parent()
                    .context("missing parent for relative symlink target")?
                    .join(&target)
            } else {
                target.clone()
            };

            if smol::fs::metadata(&resolved_target).await?.is_dir() {
                let resolved_target = Self::canonicalize(&resolved_target)?;
                let status = new_command("cmd")
                    .args(["/C", "mklink", "/J"])
                    .args([path, resolved_target.as_path()])
                    .status()
                    .await?;

                if !status.success() {
                    return Err(anyhow::anyhow!(
                        "Failed to create junction from {:?} to {:?}",
                        path,
                        target
                    ));
                }
            } else {
                smol::fs::windows::symlink_file(target, path).await?;
            }
        }

        Ok(())
    }

    async fn canonicalize(&self, path: &Path) -> anyhow::Result<PathBuf> {
        let path = path.to_path_buf();
        self.executor
            .spawn(async move {
                let result = Self::canonicalize(&path);

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

        #[cfg(unix)]
        let is_fifo = metadata.file_type().is_fifo();

        #[cfg(target_os = "windows")]
        let is_fifo = false;

        let path_buf = path.to_path_buf();
        let is_executable = self
            .executor
            .spawn(async move { path_buf.is_executable() })
            .await;

        Ok(Some(Metadata {
            inode,
            mtime: MTime(metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH)),
            is_symlink,
            is_dir: metadata.file_type().is_dir(),
            len: metadata.len(),
            is_fifo,
            is_executable,
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

    async fn open_handle(&self, path: &Path) -> anyhow::Result<Arc<dyn FileHandle>> {
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(target_os = "windows")]
        {
            options.custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0);
        }
        Ok(Arc::new(options.open(path)?))
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

    async fn rename(
        &self,
        source: &Path,
        target: &Path,
        options: RenameOptions,
    ) -> anyhow::Result<()> {
        if options.create_parents
            && let Some(parent) = target.parent()
        {
            self.create_dir(parent).await?;
        }

        if options.overwrite {
            smol::fs::rename(source, target).await?;
            return Ok(());
        }

        let use_metadata_fallback = {
            #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
            {
                let source = source.to_path_buf();
                let target = target.to_path_buf();
                match self
                    .executor
                    .spawn(async move { rename_without_replace(&source, &target) })
                    .await
                {
                    Ok(()) => return Ok(()),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                        if options.ignore_if_exists {
                            return Ok(());
                        }
                        return Err(error.into());
                    }
                    Err(error)
                        if error.raw_os_error().is_some_and(|code| {
                            code == libc::ENOSYS
                                || code == libc::ENOTSUP
                                || code == libc::EOPNOTSUPP
                                || code == libc::EINVAL
                        }) =>
                    {
                        true
                    }
                    Err(error) => return Err(error.into()),
                }
            }
        };

        if use_metadata_fallback && smol::fs::metadata(target).await.is_ok() {
            if options.ignore_if_exists {
                return Ok(());
            } else {
                anyhow::bail!("{target:?} already exists");
            }
        }

        smol::fs::rename(source, target).await?;
        Ok(())
    }

    async fn remove_dir(&self, path: &Path, options: RemoveOptions) -> anyhow::Result<()> {
        let result = if options.recursive {
            smol::fs::remove_dir_all(path).await
        } else {
            smol::fs::remove_dir(path).await
        };

        match result {
            Ok(()) => Ok(()),
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound && options.ignore_if_not_exists =>
            {
                Ok(())
            }
            Err(error) => Err(error.into()),
        }
    }

    async fn remove_file(&self, path: &Path, options: RemoveOptions) -> anyhow::Result<()> {
        #[cfg(target_os = "windows")]
        if let Ok(Some(metadata)) = self.metadata(path).await
            && metadata.is_symlink
            && metadata.is_dir
        {
            self.remove_dir(
                path,
                RemoveOptions {
                    recursive: false,
                    ignore_if_not_exists: true,
                },
            )
            .await?;
            return Ok(());
        }

        match smol::fs::remove_file(path).await {
            Ok(()) => Ok(()),
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound && options.ignore_if_not_exists =>
            {
                Ok(())
            }
            Err(error) => Err(error.into()),
        }
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

    async fn write(&self, path: &Path, content: &[u8]) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            self.create_dir(parent).await?;
        }

        let path = path.to_path_buf();
        let content = content.to_vec();
        self.executor
            .spawn(async move {
                std::fs::write(&path, content)
                    .with_context(|| format!("failed to write file {}", path.display()))?;
                Ok(())
            })
            .await
    }

    async fn is_case_sensitive(&self) -> bool {
        const UNINITIALIZED: u8 = 0;
        const CASE_SENSITIVE: u8 = 1;
        const NOT_CASE_SENSITIVE: u8 = 2;

        let load = self.is_case_sensitive.load(Ordering::Acquire);
        if load != UNINITIALIZED {
            return load == CASE_SENSITIVE;
        }

        let is_case_sensitive = self
            .executor
            .spawn(async move {
                is_filesystem_case_sensitive().unwrap_or_else(|error| {
                    log::error!(
                        "Failed to determine whether filesystem is case sensitive. Falling back to true: {error:#}"
                    );
                    true
                })
            })
            .await;

        self.is_case_sensitive.store(
            if is_case_sensitive {
                CASE_SENSITIVE
            } else {
                NOT_CASE_SENSITIVE
            },
            Ordering::Release,
        );

        is_case_sensitive
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
        let path = NativeFs::canonicalize(temp_dir.path()).unwrap();

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
    async fn create_dir(&self, path: &Path) -> anyhow::Result<()> {
        let absolute_path = resolve_path(self.path(), path);
        NativeFs::new(self.executor.clone())
            .create_dir(&absolute_path)
            .await
    }

    async fn create_symlink(&self, path: &Path, target: PathBuf) -> anyhow::Result<()> {
        let absolute_path = resolve_path(self.path(), path);
        NativeFs::new(self.executor.clone())
            .create_symlink(&absolute_path, target)
            .await
    }

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

    async fn open_handle(&self, path: &Path) -> anyhow::Result<Arc<dyn FileHandle>> {
        let absolute_path = resolve_path(self.path(), path);
        NativeFs::new(self.executor.clone())
            .open_handle(&absolute_path)
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

    async fn rename(
        &self,
        source: &Path,
        target: &Path,
        options: RenameOptions,
    ) -> anyhow::Result<()> {
        let absolute_source = resolve_path(self.path(), source);
        let absolute_target = resolve_path(self.path(), target);
        NativeFs::new(self.executor.clone())
            .rename(&absolute_source, &absolute_target, options)
            .await
    }

    async fn remove_dir(&self, path: &Path, options: RemoveOptions) -> anyhow::Result<()> {
        let absolute_path = resolve_path(self.path(), path);
        NativeFs::new(self.executor.clone())
            .remove_dir(&absolute_path, options)
            .await
    }

    async fn remove_file(&self, path: &Path, options: RemoveOptions) -> anyhow::Result<()> {
        let absolute_path = resolve_path(self.path(), path);
        NativeFs::new(self.executor.clone())
            .remove_file(&absolute_path, options)
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

    async fn write(&self, path: &Path, content: &[u8]) -> anyhow::Result<()> {
        let absolute_path = resolve_path(self.path(), path);
        NativeFs::new(self.executor.clone())
            .write(&absolute_path, content)
            .await
    }

    async fn is_case_sensitive(&self) -> bool {
        self.executor
            .spawn(async move {
                is_filesystem_case_sensitive().unwrap_or_else(|error| {
                    log::error!(
                        "Failed to determine whether filesystem is case sensitive. Falling back to true: {error:#}"
                    );
                    true
                })
            })
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

fn is_filesystem_case_sensitive() -> anyhow::Result<bool> {
    let temp_dir =
        TempDir::new().context("failed to create temporary case sensitivity directory")?;

    let test_file_1 = temp_dir.path().join("case_sensitivity_test.tmp");
    let test_file_2 = temp_dir.path().join("CASE_SENSITIVITY_TEST.TMP");

    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&test_file_1)
        .with_context(|| format!("failed to create {}", test_file_1.display()))?;

    let case_sensitive = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&test_file_2)
    {
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to create {}", test_file_2.display()));
        }
    };

    Ok(case_sensitive)
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

        // Safety: `file` remains valid for the duration of this call, so the raw handle remains valid,
        // and `info.as_mut_ptr()` points to writable storage for `BY_HANDLE_FILE_INFORMATION`.
        unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), info.as_mut_ptr())? };

        // Safety: A successful `GetFileInformationByHandle` call above guarantees
        // that the output buffer was filled.
        let info = unsafe { info.assume_init() };

        Ok(((info.nFileIndexHigh as u64) << 32) | (info.nFileIndexLow as u64))
    })
    .await
}
