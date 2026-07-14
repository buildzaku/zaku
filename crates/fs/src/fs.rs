pub mod fs_watcher;

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use futures::{FutureExt, Stream, StreamExt, channel::oneshot, future::BoxFuture};
use gpui::BackgroundExecutor;
use is_executable::IsExecutable;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
#[cfg(feature = "test")]
use serde_json::Value;
#[cfg(target_os = "macos")]
use std::{
    ffi::{CStr, OsStr},
    mem::MaybeUninit,
};
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    task,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tempfile::TempDir;
#[cfg(target_os = "windows")]
use {
    smol::fs::windows::OpenOptionsExt as SmolOpenOptionsExt,
    std::{
        ffi::OsString,
        mem::MaybeUninit,
        os::windows::{
            ffi::{OsStrExt, OsStringExt},
            fs::OpenOptionsExt,
            io::AsRawHandle,
        },
    },
    util::command::new_command,
    windows::{
        Win32::{
            Foundation::HANDLE,
            Storage::FileSystem::{
                BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS, FILE_NAME_NORMALIZED,
                GetFileInformationByHandle, GetFinalPathNameByHandleW, GetVolumePathNameW,
                MOVE_FILE_FLAGS, MoveFileExW, REPLACE_FILE_FLAGS, ReplaceFileW,
            },
        },
        core::{HSTRING, PCWSTR},
    },
};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use {
    std::{
        ffi::CString,
        os::{
            fd::{AsFd, AsRawFd},
            unix::{
                ffi::OsStrExt,
                fs::{FileTypeExt, MetadataExt},
            },
        },
    },
    tempfile::NamedTempFile,
};

use path::SanitizedPath;
use util::ResultExt;

use crate::fs_watcher::FsWatcher;

pub trait Watcher: Send + Sync {
    fn add(&self, path: &Path) -> anyhow::Result<()>;
    fn remove(&self, path: &Path) -> anyhow::Result<()>;
}

struct WatchStream {
    stream: Pin<Box<dyn Send + Stream<Item = Vec<PathEvent>>>>,
    _watcher: Arc<dyn Watcher>,
}

impl WatchStream {
    fn new(
        stream: Pin<Box<dyn Send + Stream<Item = Vec<PathEvent>>>>,
        watcher: Arc<dyn Watcher>,
    ) -> Self {
        Self {
            stream,
            _watcher: watcher,
        }
    }
}

impl Stream for WatchStream {
    type Item = Vec<PathEvent>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        context: &mut task::Context<'_>,
    ) -> task::Poll<Option<Self::Item>> {
        self.stream.as_mut().poll_next(context)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PathEventKind {
    Removed,
    Created,
    Changed,
    Rescan,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PathEvent {
    pub path: PathBuf,
    pub kind: Option<PathEventKind>,
}

impl From<PathEvent> for PathBuf {
    fn from(event: PathEvent) -> Self {
        event.path
    }
}

#[derive(Clone, Copy, Default)]
pub struct RenameOptions {
    pub overwrite: bool,
    pub ignore_if_exists: bool,
    pub create_parents: bool,
}

#[derive(Clone, Copy, Default)]
pub struct RemoveOptions {
    pub recursive: bool,
    pub ignore_if_not_exists: bool,
}

#[derive(Clone, Copy, Default)]
pub struct CopyOptions {
    pub overwrite: bool,
    pub ignore_if_exists: bool,
}

#[async_trait]
pub trait Fs: Send + Sync {
    async fn create_dir(&self, path: &Path) -> anyhow::Result<()>;
    async fn create_symlink(&self, path: &Path, target: PathBuf) -> anyhow::Result<()>;
    async fn canonicalize(&self, path: &Path) -> anyhow::Result<PathBuf>;
    async fn metadata(&self, path: &Path) -> anyhow::Result<Option<Metadata>>;
    async fn load(&self, path: &Path) -> anyhow::Result<String>;
    async fn load_bytes(&self, path: &Path) -> anyhow::Result<Vec<u8>>;
    async fn atomic_write(&self, path: PathBuf, content: String) -> anyhow::Result<()>;
    async fn open_handle(&self, path: &Path) -> anyhow::Result<Arc<dyn FileHandle>>;
    async fn read_link(&self, path: &Path) -> anyhow::Result<PathBuf>;
    async fn read_dir(
        &self,
        path: &Path,
    ) -> anyhow::Result<Pin<Box<dyn Send + Stream<Item = anyhow::Result<PathBuf>>>>>;
    async fn copy_file(
        &self,
        source: &Path,
        target: &Path,
        options: CopyOptions,
    ) -> anyhow::Result<()>;
    async fn rename(
        &self,
        source: &Path,
        target: &Path,
        options: RenameOptions,
    ) -> anyhow::Result<()>;
    async fn trash(&self, path: &Path, options: RemoveOptions) -> anyhow::Result<()>;
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

    #[cfg(target_os = "macos")]
    fn current_path(&self, _: &Arc<dyn Fs>) -> anyhow::Result<PathBuf> {
        let fd = self.as_fd();
        let mut path_buf = MaybeUninit::<[u8; libc::PATH_MAX as usize]>::uninit();

        // SAFETY: `fd` remains valid for the duration of this call and `path_buf`
        // provides writable `PATH_MAX`-sized storage for the kernel to fill.
        let result = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETPATH, path_buf.as_mut_ptr()) };

        anyhow::ensure!(result != -1, "fcntl returned -1");

        // SAFETY: Successful `libc::fcntl()` call above populates `path_buf` with
        // a valid C string.
        let c_str = unsafe { CStr::from_ptr(path_buf.as_ptr().cast()) };

        anyhow::ensure!(
            !c_str.is_empty(),
            "could not find a path for the file handle"
        );
        Ok(PathBuf::from(OsStr::from_bytes(c_str.to_bytes())))
    }

    #[cfg(target_os = "windows")]
    fn current_path(&self, _: &Arc<dyn Fs>) -> anyhow::Result<PathBuf> {
        let handle = HANDLE(self.as_raw_handle());

        // SAFETY: `handle` remains valid for the duration of this call and the empty
        // buffer is used to query the required path length.
        let required_len =
            unsafe { GetFinalPathNameByHandleW(handle, &mut [], FILE_NAME_NORMALIZED) };

        anyhow::ensure!(
            required_len != 0,
            "GetFinalPathNameByHandleW returned 0 length"
        );

        let required_len =
            usize::try_from(required_len).context("required path length should fit in usize")?;
        let buffer_len = required_len
            .checked_add(1)
            .context("required path length should leave room for terminator")?;
        let mut buf = vec![0u16; buffer_len];

        // SAFETY: `handle` remains valid for the duration of this call and `buf`
        // provides writable storage for the returned UTF-16 path.
        let written = unsafe { GetFinalPathNameByHandleW(handle, &mut buf, FILE_NAME_NORMALIZED) };

        anyhow::ensure!(
            written != 0,
            "GetFinalPathNameByHandleW failed to write path"
        );

        let written =
            usize::try_from(written).context("written path length should fit in usize")?;
        let path = buf
            .get(..written)
            .context("written path length should be in bounds")?;
        let os_str = OsString::from_wide(path);
        anyhow::ensure!(
            !os_str.is_empty(),
            "could not find a path for the file handle"
        );
        Ok(PathBuf::from(os_str))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

    pub fn bad_is_greater_than(self, other: MTime) -> bool {
        self.0 > other.0
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Metadata {
    pub inode: u64,
    pub mtime: MTime,
    pub is_symlink: bool,
    pub is_dir: bool,
    pub len: u64,
    pub is_fifo: bool,
    pub is_executable: bool,
}

#[cfg(target_os = "linux")]
fn rename_without_replace(source: &Path, target: &Path) -> io::Result<()> {
    let source = path_to_c_string(source)?;
    let target = path_to_c_string(target)?;

    // SAFETY: `source` and `target` remain valid NUL-terminated C strings for the
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
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "macos")]
fn rename_without_replace(source: &Path, target: &Path) -> io::Result<()> {
    let source = path_to_c_string(source)?;
    let target = path_to_c_string(target)?;

    // SAFETY: `source` and `target` remain valid NUL-terminated C strings for the
    // duration of this call.
    let result = unsafe { libc::renamex_np(source.as_ptr(), target.as_ptr(), libc::RENAME_EXCL) };

    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "windows")]
fn rename_without_replace(source: &Path, target: &Path) -> io::Result<()> {
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();

    // SAFETY: `source` and `target` remain valid NUL-terminated UTF-16 strings for
    // the duration of this call.
    let result = unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(target.as_ptr()),
            MOVE_FILE_FLAGS::default(),
        )
    };

    if let Err(_error) = result {
        return Err(io::Error::last_os_error());
    }

    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn path_to_c_string(path: &Path) -> io::Result<CString> {
    CString::new(path.as_os_str().as_bytes()).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("path contains interior NUL: {}: {error}", path.display()),
        )
    })
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
        #[cfg(any(target_os = "linux", target_os = "macos"))]
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

            // SAFETY: `path_hstring` remains valid for the duration of this call and
            // `volume_buffer` provides writable storage for the returned UTF-16 volume root.
            unsafe { GetVolumePathNameW(&path_hstring, &mut volume_buffer)? };

            let volume_root = {
                let len = volume_buffer
                    .iter()
                    .position(|&character| character == 0)
                    .unwrap_or(volume_buffer.len());
                let volume = volume_buffer
                    .get(..len)
                    .context("volume path length should be in bounds")?;
                PathBuf::from(OsString::from_wide(volume))
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

#[async_trait]
impl Fs for NativeFs {
    async fn create_dir(&self, path: &Path) -> anyhow::Result<()> {
        Ok(smol::fs::create_dir_all(path).await?)
    }

    async fn create_symlink(&self, path: &Path, target: PathBuf) -> anyhow::Result<()> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
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
                    return Err(anyhow!(
                        "Failed to create junction from {} to {}",
                        path.display(),
                        target.display()
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

        #[cfg(any(target_os = "linux", target_os = "macos"))]
        let inode = metadata.ino();

        #[cfg(target_os = "windows")]
        let inode = file_id(&path).await?;

        #[cfg(any(target_os = "linux", target_os = "macos"))]
        let is_fifo = metadata.file_type().is_fifo();

        #[cfg(target_os = "windows")]
        let is_fifo = false;

        let path_buf = path.clone();
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

    async fn load_bytes(&self, path: &Path) -> anyhow::Result<Vec<u8>> {
        let path = path.to_path_buf();
        self.executor
            .spawn(async move {
                std::fs::read(&path)
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
            Err(error) => Err(anyhow!("failed to read dir entry {error:?}")),
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

    async fn copy_file(
        &self,
        source: &Path,
        target: &Path,
        options: CopyOptions,
    ) -> anyhow::Result<()> {
        if !options.overwrite && smol::fs::metadata(target).await.is_ok() {
            if options.ignore_if_exists {
                return Ok(());
            }
            anyhow::bail!("{} already exists", target.display());
        }

        smol::fs::copy(source, target).await?;
        Ok(())
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
            let source = source.to_path_buf();
            let target = target.to_path_buf();
            match self
                .executor
                .spawn(async move { rename_without_replace(&source, &target) })
                .await
            {
                Ok(()) => return Ok(()),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
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
        };

        if use_metadata_fallback && smol::fs::metadata(target).await.is_ok() {
            if options.ignore_if_exists {
                return Ok(());
            }
            anyhow::bail!("{} already exists", target.display());
        }

        smol::fs::rename(source, target).await?;
        Ok(())
    }

    async fn trash(&self, path: &Path, _options: RemoveOptions) -> anyhow::Result<()> {
        let path = self
            .canonicalize(path)
            .await
            .context("Could not canonicalize the path of the file")?;

        let (tx, rx) = oneshot::channel();
        std::thread::Builder::new()
            .name("trash file or dir".to_string())
            .spawn(move || {
                if tx.send(trash::delete(path)).is_err() {
                    log::trace!("Trash receiver dropped");
                }
            })
            .context("Failed to spawn trash thread")?;

        rx.await
            .context("Trash sender dropped")?
            .context("Could not trash file or dir")
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
                if error.kind() == io::ErrorKind::NotFound && options.ignore_if_not_exists =>
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
                if error.kind() == io::ErrorKind::NotFound && options.ignore_if_not_exists =>
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
        let pending_paths: Arc<Mutex<Vec<PathEvent>>> = Arc::default();
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

        if let Ok(mut target) = self.read_link(path).await {
            log::trace!(
                "Watching symlink target {} -> {}",
                path.display(),
                target.display()
            );

            if target.is_relative()
                && let Some(parent) = path.parent()
            {
                target = parent.join(target);

                if let Ok(canonical) = self.canonicalize(&target).await {
                    target = SanitizedPath::new(&canonical).as_path().to_path_buf();
                }
            }

            watcher.add(&target).log_err();

            if let Some(parent) = target.parent() {
                watcher.add(parent).log_err();
            }
        }

        let stream: Pin<Box<dyn Send + Stream<Item = Vec<PathEvent>>>> = Box::pin(rx.filter_map({
            let executor = executor.clone();

            move |()| {
                let pending_paths = pending_paths.clone();
                let executor = executor.clone();

                async move {
                    executor.timer(latency).await;
                    let paths = std::mem::take(&mut *pending_paths.lock());
                    (!paths.is_empty()).then_some(paths)
                }
            }
        }));

        (Box::pin(WatchStream::new(stream, watcher.clone())), watcher)
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

    async fn atomic_write(&self, path: PathBuf, content: String) -> anyhow::Result<()> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            smol::unblock(move || {
                let mut temp_file =
                    NamedTempFile::new_in(path.parent().unwrap_or_else(|| path::cache_dir()))?;
                temp_file.write_all(content.as_bytes())?;
                temp_file.persist(path)?;
                anyhow::Ok(())
            })
            .await?;
        }

        #[cfg(target_os = "windows")]
        {
            smol::unblock(move || {
                let temp_dir = TempDir::new_in(path.parent().unwrap_or_else(|| path::cache_dir()))?;
                let temp_file = {
                    let temp_file_path = temp_dir.path().join("temp_file");
                    let mut file = std::fs::File::create_new(&temp_file_path)?;
                    file.write_all(content.as_bytes())?;
                    temp_file_path
                };

                match std::fs::File::create_new(path.as_path()) {
                    Ok(_) => {}
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error.into()),
                }

                // SAFETY: Paths are converted to owned Windows strings that remain valid
                // for the duration of the call.
                unsafe {
                    ReplaceFileW(
                        &HSTRING::from(path.to_string_lossy().into_owned()),
                        &HSTRING::from(temp_file.to_string_lossy().into_owned()),
                        None,
                        REPLACE_FILE_FLAGS::default(),
                        None,
                        None,
                    )?;
                }

                anyhow::Ok(())
            })
            .await?;
        }

        Ok(())
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

#[cfg(feature = "test")]
pub struct TempFs {
    path: PathBuf,
    executor: BackgroundExecutor,
    _temp_dir: TempDir,
}

#[cfg(feature = "test")]
impl TempFs {
    pub fn new(executor: BackgroundExecutor) -> Arc<Self> {
        let temp_dir = TempDir::new().expect("failed to create temporary filesystem");
        let path = NativeFs::canonicalize(temp_dir.path())
            .expect("failed to canonicalize temporary filesystem path");

        Arc::new(Self {
            path,
            executor,
            _temp_dir: temp_dir,
        })
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    pub fn insert_tree(&self, path: impl AsRef<Path>, tree: Value) {
        fn inner(directory: &Path, path: &Path, tree: Value) -> anyhow::Result<()> {
            match tree {
                Value::Object(map) => {
                    let absolute_path = resolve_path(directory, path);
                    std::fs::create_dir_all(&absolute_path)
                        .context("failed to create test directory")?;

                    for (name, contents) in map {
                        let mut new_path = PathBuf::from(path);
                        new_path.push(name);
                        inner(directory, &new_path, contents)?;
                    }
                }
                Value::Null => {
                    let absolute_path = resolve_path(directory, path);
                    std::fs::create_dir_all(&absolute_path)
                        .context("failed to create test directory")?;
                }
                Value::String(contents) => {
                    let absolute_path = resolve_path(directory, path);
                    if let Some(parent) = absolute_path.parent() {
                        std::fs::create_dir_all(parent)
                            .context("failed to create test file parent directory")?;
                    }

                    std::fs::write(&absolute_path, contents.as_bytes())
                        .context("failed to write test file")?;
                }
                _ => {
                    anyhow::bail!("JSON object must contain only objects, strings, or null");
                }
            }

            Ok(())
        }

        inner(self.path(), path.as_ref(), tree).expect("failed to insert test filesystem tree");
    }
}

#[cfg(feature = "test")]
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

    async fn load_bytes(&self, path: &Path) -> anyhow::Result<Vec<u8>> {
        let absolute_path = resolve_path(self.path(), path);
        NativeFs::new(self.executor.clone())
            .load_bytes(&absolute_path)
            .await
    }

    async fn atomic_write(&self, path: PathBuf, content: String) -> anyhow::Result<()> {
        let absolute_path = resolve_path(self.path(), &path);
        NativeFs::new(self.executor.clone())
            .atomic_write(absolute_path, content)
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
        let mut entries = NativeFs::new(self.executor.clone())
            .read_dir(&absolute_path)
            .await?;
        let mut paths = Vec::new();
        while let Some(entry) = entries.next().await {
            paths.push(entry?);
        }
        paths.sort();

        let result = futures::stream::iter(paths.into_iter().map(Ok::<_, anyhow::Error>));
        Ok(Box::pin(result))
    }

    async fn read_link(&self, path: &Path) -> anyhow::Result<PathBuf> {
        let absolute_path = resolve_path(self.path(), path);
        NativeFs::new(self.executor.clone())
            .read_link(&absolute_path)
            .await
    }

    async fn copy_file(
        &self,
        source: &Path,
        target: &Path,
        options: CopyOptions,
    ) -> anyhow::Result<()> {
        let absolute_source = resolve_path(self.path(), source);
        let absolute_target = resolve_path(self.path(), target);
        NativeFs::new(self.executor.clone())
            .copy_file(&absolute_source, &absolute_target, options)
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

    async fn trash(&self, path: &Path, options: RemoveOptions) -> anyhow::Result<()> {
        let absolute_path = resolve_path(self.path(), path);
        let fs = NativeFs::new(self.executor.clone());
        let Some(metadata) = fs.metadata(&absolute_path).await? else {
            if options.ignore_if_not_exists {
                return Ok(());
            }

            anyhow::bail!("{} does not exist", absolute_path.display());
        };

        if metadata.is_dir && !metadata.is_symlink {
            fs.remove_dir(&absolute_path, options).await
        } else {
            fs.remove_file(&absolute_path, options).await
        }
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
            if error.kind() == io::ErrorKind::NotFound
                || error.kind() == io::ErrorKind::NotADirectory =>
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
            Err(error) if error.kind() == io::ErrorKind::NotFound => symlink_metadata,
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

pub async fn copy_recursive(
    fs: &dyn Fs,
    source: &Path,
    target: &Path,
    options: CopyOptions,
) -> anyhow::Result<()> {
    for (item, is_dir) in read_dir_items(fs, source).await? {
        let Ok(item_relative_path) = item.strip_prefix(source) else {
            continue;
        };
        let target_item = if item_relative_path == Path::new("") {
            target.to_path_buf()
        } else {
            target.join(item_relative_path)
        };

        if is_dir {
            if let Some(metadata) = fs.metadata(&target_item).await? {
                if !options.overwrite {
                    if options.ignore_if_exists {
                        continue;
                    }
                    anyhow::bail!("{} already exists", target_item.display());
                }

                if metadata.is_dir {
                    fs.remove_dir(
                        &target_item,
                        RemoveOptions {
                            recursive: true,
                            ignore_if_not_exists: true,
                        },
                    )
                    .await?;
                } else {
                    fs.remove_file(
                        &target_item,
                        RemoveOptions {
                            recursive: false,
                            ignore_if_not_exists: true,
                        },
                    )
                    .await?;
                }
            }
            fs.create_dir(&target_item).await?;
        } else {
            fs.copy_file(&item, &target_item, options).await?;
        }
    }

    Ok(())
}

fn read_recursive<'a>(
    fs: &'a dyn Fs,
    source: &'a Path,
    output: &'a mut Vec<(PathBuf, bool)>,
) -> BoxFuture<'a, anyhow::Result<()>> {
    async move {
        let metadata = fs
            .metadata(source)
            .await?
            .with_context(|| format!("path does not exist: {}", source.display()))?;

        if metadata.is_dir {
            output.push((source.to_path_buf(), true));
            let mut children = fs.read_dir(source).await?;
            while let Some(child_path) = children.next().await {
                let child_path = child_path?;
                read_recursive(fs, &child_path, output).await?;
            }
        } else {
            output.push((source.to_path_buf(), false));
        }

        Ok(())
    }
    .boxed()
}

pub async fn read_dir_items(fs: &dyn Fs, source: &Path) -> anyhow::Result<Vec<(PathBuf, bool)>> {
    let mut items = Vec::new();
    read_recursive(fs, source, &mut items).await?;
    Ok(items)
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
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => false,
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to create {}", test_file_2.display()));
        }
    };

    Ok(case_sensitive)
}

#[cfg(feature = "test")]
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

        // SAFETY: `file` remains valid for the duration of this call, so the raw handle remains valid,
        // and `info.as_mut_ptr()` points to writable storage for `BY_HANDLE_FILE_INFORMATION`.
        unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), info.as_mut_ptr())? };

        // SAFETY: A successful `GetFileInformationByHandle` call above guarantees
        // that the output buffer was filled.
        let info = unsafe { info.assume_init() };

        Ok((u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow))
    })
    .await
}
