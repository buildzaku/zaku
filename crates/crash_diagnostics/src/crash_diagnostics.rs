pub use minidumper::Client;

use crash_handler::{CrashEventResult, CrashHandler};
use minidumper::{LoopAction, MinidumpBinary, Server, SocketName};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
#[cfg(any(target_os = "linux", target_os = "windows", test))]
use std::mem;
use std::{
    env,
    fs::{self, File},
    io::{self, Write as _},
    panic::{self, Location},
    path::{Path, PathBuf},
    pin::Pin,
    process,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
#[cfg(target_os = "windows")]
use std::{ffi::OsStr, iter::once, os::windows::ffi::OsStrExt};
#[cfg(target_os = "windows")]
use windows::{
    Win32::{
        Storage::FileSystem::WriteFile,
        System::{
            Console::{GetStdHandle, STD_ERROR_HANDLE},
            Threading::{
                CreateProcessW, PROCESS_CREATION_FLAGS, PROCESS_INFORMATION,
                STARTF_FORCEOFFFEEDBACK, STARTUPINFOW,
            },
        },
    },
    core::PWSTR,
};

use system_specs::GpuSpecs;

const CRASH_HANDLER_PING_TIMEOUT: Duration = Duration::from_mins(1);
const CRASH_HANDLER_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

fn stderr_println(message: &str) {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        for mut bytes in [message.as_bytes(), b"\n".as_slice()] {
            while !bytes.is_empty() {
                // SAFETY: `bytes` provides a valid buffer for `libc::write`.
                let written =
                    unsafe { libc::write(libc::STDERR_FILENO, bytes.as_ptr().cast(), bytes.len()) };
                let written = match usize::try_from(written) {
                    Ok(0) | Err(_) => return,
                    Ok(written) => written,
                };

                let Some(remaining) = bytes.get(written..) else {
                    return;
                };
                bytes = remaining;
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        // SAFETY: `GetStdHandle` has no preconditions.
        let stderr = match unsafe { GetStdHandle(STD_ERROR_HANDLE) } {
            Ok(stderr) => stderr,
            Err(_) => return,
        };
        for mut bytes in [message.as_bytes(), b"\n".as_slice()] {
            while !bytes.is_empty() {
                let mut written = 0;

                // SAFETY: `stderr` is a valid handle, `bytes` provides a valid buffer and
                // `written` provides writable storage for `WriteFile`.
                let result =
                    unsafe { WriteFile(stderr, Some(bytes), Some(&raw mut written), None) };
                if result.is_err() || written == 0 {
                    return;
                }

                let written =
                    usize::try_from(written).expect("written byte count should fit in usize");
                let Some(remaining) = bytes.get(written..) else {
                    return;
                };
                bytes = remaining;
            }
        }
    }
}

/// Force a backtrace to be printed on panic.
///
/// # Safety
///
/// Call this function before spawning any other threads.
pub unsafe fn force_backtrace() {
    // SAFETY: The caller guarantees that no other threads are running.
    unsafe { env::set_var("RUST_BACKTRACE", "1") };

    let old_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        old_hook(info);
        // On macOS, exit after the previous panic hook runs to prevent the system crash dialog.
        if cfg!(target_os = "macos") {
            process::exit(1);
        }
    }));
}

pub fn init<F, S, C, P>(
    crash_init: InitCrashHandler,
    spawn: S,
    socket_path: P,
    wait_timer: C,
) -> impl Future<Output = Arc<Client>> + use<F, C, S, P>
where
    F: Future<Output = ()> + Send + Sync + 'static,
    C: (Fn(Duration) -> F) + Send + Sync + 'static,
    S: FnOnce(Pin<Box<dyn Future<Output = ()> + Send + 'static>>),
    P: FnOnce(u32) -> PathBuf,
{
    connect_and_keepalive(crash_init, socket_path, wait_timer, spawn)
}

async fn connect_and_keepalive<F, C, S, P>(
    crash_init: InitCrashHandler,
    socket_path: P,
    wait_timer: C,
    spawn: S,
) -> Arc<Client>
where
    F: Future<Output = ()> + Send + Sync + 'static,
    C: (Fn(Duration) -> F) + Send + Sync + 'static,
    S: FnOnce(Pin<Box<dyn Future<Output = ()> + Send + 'static>>),
    P: FnOnce(u32) -> PathBuf,
{
    let executable = env::current_exe().expect("unable to find current executable");
    let socket_path = socket_path(process::id());
    let crash_handler = spawn_crash_handler(&executable, &socket_path);
    log::info!("Spawning crash handler process");
    let mut elapsed = Duration::ZERO;
    let retry_frequency = Duration::from_millis(100);
    let client = loop {
        if let Ok(client) = Client::with_name(SocketName::Path(&socket_path)) {
            log::info!("Connected to crash handler process after {elapsed:?}");
            break client;
        }
        elapsed += retry_frequency;
        wait_timer(retry_frequency).await;
    };
    let client = Arc::new(client);

    panic::set_hook({
        let client = client.clone();
        Box::new(move |payload| {
            panic_hook(
                &client,
                payload.payload_as_str().unwrap_or("Box<Any>"),
                payload.location(),
            );
        })
    });
    log::info!("Panic handler registered");
    let crash_event = {
        let client = client.clone();
        let handler = move |crash_context: &crash_handler::CrashContext| {
            static REQUESTED_MINIDUMP: AtomicBool = AtomicBool::new(false);

            let result = if REQUESTED_MINIDUMP
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                #[cfg(target_os = "macos")]
                macos::suspend_all_other_threads();

                // On macOS, `ping` ensures the crash server processes every preceding
                // `send_message` call before `request_dump`.
                #[cfg(target_os = "macos")]
                if client.ping().is_err() {
                    stderr_println("failed to synchronize with crash handler");
                }
                let result = client.request_dump(crash_context);
                if result.is_err() {
                    stderr_println("failed to request dump");
                }
                #[cfg(target_os = "macos")]
                macos::resume_all_other_threads();
                result.is_ok()
            } else {
                true
            };
            CrashEventResult::Handled(result)
        };

        // SAFETY: `handler` requests a minidump from the separate crash handler process
        // without accessing application data that may be corrupted by the crash.
        unsafe { crash_handler::make_crash_event(handler) }
    };
    let handler = CrashHandler::attach(crash_event).expect("failed to attach signal handler");

    log::info!("Crash signal handlers installed");
    send_crash_server_message(&client, &CrashServerMessage::Init(crash_init));

    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    if let Some(address) = abort_message_address() {
        send_crash_server_message(
            &client,
            &CrashServerMessage::AbortMessageLocation(AbortMessageLocation {
                pid: process::id(),
                address,
            }),
        );
    }

    #[cfg(target_os = "linux")]
    handler.set_ptracer(Some(crash_handler.id()));

    log::info!("Crash handler registered");
    spawn(Box::pin({
        let client = client.clone();
        async move {
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            let mut crash_handler = crash_handler;

            #[cfg(target_os = "windows")]
            let () = crash_handler;

            let _handler = handler;
            loop {
                if let Err(error) = client.ping() {
                    #[cfg(any(target_os = "linux", target_os = "macos"))]
                    log::error!(
                        "Crash handler ping failed: {error:?}, process exit status: {:?}",
                        crash_handler.try_status()
                    );

                    #[cfg(target_os = "windows")]
                    log::error!("Crash handler ping failed: {error:?}");
                    break;
                }
                wait_timer(Duration::from_secs(10)).await;
            }
        }
    }));
    client
}

pub struct CrashServer {
    initialization_params: Mutex<Option<InitCrashHandler>>,
    panic_info: Mutex<Option<CrashPanic>>,
    active_gpu: Mutex<Option<GpuSpecs>>,
    abort_message_location: Mutex<Option<AbortMessageLocation>>,
    has_connection: Arc<AtomicBool>,
    logs_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashInfo {
    pub init: InitCrashHandler,
    pub panic: Option<CrashPanic>,
    pub minidump_error: Option<String>,
    pub abort_message: Option<String>,
    pub gpus: Vec<system_specs::GpuInfo>,
    pub active_gpu: Option<GpuSpecs>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AbortMessageLocation {
    pub pid: u32,
    pub address: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitCrashHandler {
    pub session_id: String,
    pub app_version: String,
    pub binary: String,
    pub release_channel: String,
    pub commit_sha: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashPanic {
    pub message: String,
    pub span: String,
}

fn send_crash_server_message(crash_client: &Arc<Client>, message: &CrashServerMessage) {
    let data = match serde_json::to_vec(message) {
        Ok(data) => data,
        Err(error) => {
            log::warn!("Failed to serialize crash server message: {error:?}");
            return;
        }
    };

    if let Err(error) = crash_client.send_message(0, data) {
        log::warn!("Failed to send data to crash server: {error:?}");
    }
}

pub fn set_gpu_info(crash_client: &Arc<Client>, specs: GpuSpecs) {
    send_crash_server_message(crash_client, &CrashServerMessage::GpuInfo(specs));
}

#[derive(Debug, Serialize, Deserialize)]
enum CrashServerMessage {
    Init(InitCrashHandler),
    Panic(CrashPanic),
    GpuInfo(GpuSpecs),
    AbortMessageLocation(AbortMessageLocation),
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn abort_message_address() -> Option<u64> {
    // glibc exposes `__abort_msg` through the `GLIBC_PRIVATE` symbol version. If the lookup
    // fails, minidump capture continues without the abort diagnostic.
    // SAFETY: The symbol name and version are static NUL-terminated C strings and
    // `RTLD_DEFAULT` selects a valid lookup scope.
    let pointer = unsafe {
        libc::dlvsym(
            libc::RTLD_DEFAULT,
            c"__abort_msg".as_ptr(),
            c"GLIBC_PRIVATE".as_ptr(),
        )
    };
    std::ptr::NonNull::new(pointer).map(|pointer| {
        u64::try_from(pointer.as_ptr().addr()).expect("pointer address should fit in u64")
    })
}

#[cfg(target_os = "linux")]
fn read_abort_message(location: AbortMessageLocation) -> Option<String> {
    let pointer_bytes =
        read_process_memory(location.pid, location.address, mem::size_of::<usize>())?;
    let pointer_bytes = pointer_bytes.try_into().ok()?;
    let message_address = usize::from_ne_bytes(pointer_bytes);
    let message_address =
        u64::try_from(message_address).expect("pointer address should fit in u64");
    if message_address == 0 {
        return None;
    }
    let header_len = mem::size_of::<u32>();
    let size_bytes = read_process_memory(location.pid, message_address, header_len)?;
    let size = u32::from_ne_bytes(size_bytes.try_into().ok()?);
    let message_bytes = read_process_memory(
        location.pid,
        message_address + u64::try_from(header_len).expect("header length should fit in u64"),
        abort_message_read_len(size)?,
    )?;
    parse_abort_message(&message_bytes)
}

#[cfg(any(target_os = "linux", test))]
fn abort_message_read_len(size: u32) -> Option<usize> {
    const PAGE_MULTIPLE: usize = 4096;
    const MAX_READ: usize = 4096;

    let size = usize::try_from(size).expect("abort message size should fit in usize");
    if size == 0 || !size.is_multiple_of(PAGE_MULTIPLE) {
        log::warn!("Abort message size {size} is not page-rounded; layout may have changed");
        return None;
    }
    Some(size.min(MAX_READ) - mem::size_of::<u32>())
}

#[cfg(any(target_os = "linux", test))]
fn parse_abort_message(bytes: &[u8]) -> Option<String> {
    let length = bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(bytes.len());
    let message = String::from_utf8_lossy(bytes.get(..length)?)
        .trim()
        .to_string();
    (!message.is_empty()).then_some(message)
}

#[cfg(target_os = "linux")]
fn read_process_memory(pid: u32, address: u64, length: usize) -> Option<Vec<u8>> {
    let mut buffer = vec![0_u8; length];
    let local = libc::iovec {
        iov_base: buffer.as_mut_ptr().cast(),
        iov_len: length,
    };
    let remote = libc::iovec {
        iov_base: std::ptr::without_provenance_mut(usize::try_from(address).ok()?),
        iov_len: length,
    };
    let pid = libc::pid_t::try_from(pid).ok()?;

    // SAFETY: `local` provides writable storage for `length` bytes and the kernel
    // validates the address described by `remote`.
    let bytes_read = unsafe { libc::process_vm_readv(pid, &local, 1, &remote, 1, 0) };

    if bytes_read < 0 {
        log::warn!(
            "Failed to read {length} bytes at {address:#x} in pid {pid}: {}",
            io::Error::last_os_error()
        );
        return None;
    }
    let bytes_read =
        usize::try_from(bytes_read).expect("non-negative byte count should fit in usize");
    if bytes_read != length {
        log::warn!("Short read at {address:#x} in pid {pid}: {bytes_read} of {length} bytes");
        return None;
    }
    Some(buffer)
}

impl minidumper::ServerHandler for CrashServer {
    fn create_minidump_file(&self) -> Result<(File, PathBuf), io::Error> {
        let dump_path = self
            .logs_dir
            .join(
                &self
                    .initialization_params
                    .lock()
                    .as_ref()
                    .expect("missing initialization data")
                    .session_id,
            )
            .with_extension("dmp");
        let file = File::create(&dump_path)?;
        Ok((file, dump_path))
    }

    fn on_minidump_created(&self, result: Result<MinidumpBinary, minidumper::Error>) -> LoopAction {
        let minidump_error = match result {
            Ok(MinidumpBinary { mut file, path, .. }) => {
                // TODO: Compress the dump while writing once minidumper supports custom writers
                // https://github.com/EmbarkStudios/crash-handling/issues/101
                let compression_result = (|| -> io::Result<()> {
                    file.flush()?;
                    drop(file);
                    let original_file = File::open(&path)?;
                    let compressed_path = path.with_extension("zstd");
                    let compressed_file = File::create(&compressed_path)?;
                    zstd::stream::copy_encode(original_file, compressed_file, 0)?;
                    fs::rename(compressed_path, path)?;
                    Ok(())
                })();
                compression_result.err().map(|error| format!("{error:#}"))
            }
            Err(error) => Some(format!("{error:?}")),
        };

        #[cfg(target_os = "linux")]
        let gpus = match system_specs::read_gpu_info_from_sys_class_drm() {
            Ok(gpus) => gpus,
            Err(error) => {
                log::warn!("Failed to collect GPU information for crash report: {error:#}");
                Vec::new()
            }
        };

        #[cfg(any(target_os = "macos", target_os = "windows"))]
        let gpus = Vec::new();

        #[cfg(target_os = "linux")]
        let abort_message = (*self.abort_message_location.lock()).and_then(read_abort_message);

        #[cfg(any(target_os = "macos", target_os = "windows"))]
        let abort_message = None;

        let crash_info = CrashInfo {
            init: self
                .initialization_params
                .lock()
                .clone()
                .expect("crash server should be initialized"),
            panic: self.panic_info.lock().clone(),
            minidump_error,
            abort_message,
            active_gpu: self.active_gpu.lock().clone(),
            gpus,
        };

        let crash_data_path = self
            .logs_dir
            .join(&crash_info.init.session_id)
            .with_extension("json");
        match serde_json::to_vec(&crash_info) {
            Ok(crash_data) => {
                if let Err(error) = fs::write(crash_data_path, crash_data) {
                    log::error!("Failed to write crash metadata: {error}");
                }
            }
            Err(error) => log::error!("Failed to serialize crash metadata: {error}"),
        }

        LoopAction::Exit
    }

    fn on_message(&self, _: u32, buffer: Vec<u8>) {
        let message: CrashServerMessage =
            serde_json::from_slice(&buffer).expect("invalid crash server message");
        match message {
            CrashServerMessage::Init(init_data) => {
                self.initialization_params.lock().replace(init_data);
            }
            CrashServerMessage::Panic(crash_panic) => {
                self.panic_info.lock().replace(crash_panic);
            }
            CrashServerMessage::GpuInfo(gpu_specs) => {
                self.active_gpu.lock().replace(gpu_specs);
            }
            CrashServerMessage::AbortMessageLocation(location) => {
                self.abort_message_location.lock().replace(location);
            }
        }
    }

    fn on_client_disconnected(&self, _: usize) -> LoopAction {
        LoopAction::Exit
    }

    fn on_client_connected(&self, _: usize) -> LoopAction {
        self.has_connection.store(true, Ordering::SeqCst);
        LoopAction::Continue
    }
}

pub fn panic_hook(crash_client: &Arc<Client>, message: &str, location: Option<&Location>) {
    let span = location
        .map(|location| format!("{}:{}", location.file(), location.line()))
        .unwrap_or_default();
    let current_thread = thread::current();
    let thread_name = current_thread.name().unwrap_or("<unnamed>");
    let location = location.map_or_else(|| "<unknown>".to_owned(), ToString::to_string);
    log::error!("Thread '{thread_name}' panicked at {location}:\n{message}...");

    send_crash_server_message(
        crash_client,
        &CrashServerMessage::Panic(CrashPanic {
            message: message.to_owned(),
            span,
        }),
    );
    log::error!("Triggering a crash to generate a minidump...");

    #[cfg(target_os = "windows")]
    {
        CrashHandler.simulate_exception(Some(234));
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        process::abort();
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;

    pub(super) fn suspend_all_other_threads() {
        // SAFETY: `current_task` has no preconditions.
        let task = unsafe { mach2::traps::current_task() };

        let mut threads: mach2::mach_types::thread_act_array_t = std::ptr::null_mut();
        let mut count = 0;

        // SAFETY: `task` is a valid task port and `threads` and `count` provide
        // writable storage for `task_threads`.
        let result = unsafe { mach2::task::task_threads(task, &raw mut threads, &raw mut count) };
        if result != mach2::kern_return::KERN_SUCCESS {
            stderr_println("failed to list threads before crash capture");
            return;
        }

        // SAFETY: `mach_thread_self` has no preconditions.
        let current = unsafe { mach2::mach_init::mach_thread_self() };

        let count = usize::try_from(count).expect("thread count should fit in usize");
        for index in 0..count {
            // SAFETY: `threads` points to an array of `count` elements and `index` is in bounds.
            let thread = unsafe { threads.add(index) };
            // SAFETY: `thread` points to an initialized element in the array returned by `task_threads`.
            let thread = unsafe { *thread };

            if thread != current {
                // SAFETY: `thread` is a valid thread port returned by `task_threads`.
                let result = unsafe { mach2::thread_act::thread_suspend(thread) };
                if result != mach2::kern_return::KERN_SUCCESS {
                    stderr_println("failed to suspend thread for crash capture");
                }
            }
        }
    }

    pub(super) fn resume_all_other_threads() {
        // SAFETY: `current_task` has no preconditions.
        let task = unsafe { mach2::traps::current_task() };

        let mut threads: mach2::mach_types::thread_act_array_t = std::ptr::null_mut();
        let mut count = 0;

        // SAFETY: `task` is a valid task port and `threads` and `count` provide
        // writable storage for `task_threads`.
        let result = unsafe { mach2::task::task_threads(task, &raw mut threads, &raw mut count) };
        if result != mach2::kern_return::KERN_SUCCESS {
            stderr_println("failed to list threads after crash capture");
            return;
        }

        // SAFETY: `mach_thread_self` has no preconditions.
        let current = unsafe { mach2::mach_init::mach_thread_self() };

        let count = usize::try_from(count).expect("thread count should fit in usize");
        for index in 0..count {
            // SAFETY: `threads` points to an array of `count` elements and `index` is in bounds.
            let thread = unsafe { threads.add(index) };
            // SAFETY: `thread` points to an initialized element in the array returned by `task_threads`.
            let thread = unsafe { *thread };

            if thread != current {
                // SAFETY: `thread` is a valid thread port returned by `task_threads`.
                let result = unsafe { mach2::thread_act::thread_resume(thread) };
                if result != mach2::kern_return::KERN_SUCCESS {
                    stderr_println("failed to resume thread after crash capture");
                }
            }
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn spawn_crash_handler(executable: &Path, socket_name: &Path) -> async_process::Child {
    async_process::Command::new(executable)
        .arg("--crash-handler")
        .arg(socket_name)
        .spawn()
        .expect("unable to spawn crash handler process")
}

#[cfg(target_os = "windows")]
fn spawn_crash_handler(executable: &Path, socket_name: &Path) {
    let mut command_line: Vec<u16> = OsStr::new(&format!(
        "\"{}\" --crash-handler \"{}\"",
        executable.display(),
        socket_name.display()
    ))
    .encode_wide()
    .chain(once(0))
    .collect();
    let startup_info = STARTUPINFOW {
        cb: u32::try_from(mem::size_of::<STARTUPINFOW>())
            .expect("startup info size should fit in u32"),
        // Spawning the same Windows GUI executable enables startup busy-cursor feedback.
        // The crash server has no window message loop to clear it, so disable the feedback.
        dwFlags: STARTF_FORCEOFFFEEDBACK,
        ..Default::default()
    };
    let mut process_info = PROCESS_INFORMATION::default();

    // SAFETY: `command_line` is a writable NUL-terminated UTF-16 buffer,
    // `startup_info` contains the required size and `process_info` provides writable storage.
    unsafe {
        CreateProcessW(
            None,
            Some(PWSTR(command_line.as_mut_ptr())),
            None,
            None,
            false,
            PROCESS_CREATION_FLAGS(0),
            None,
            None,
            &startup_info,
            &mut process_info,
        )
        .expect("unable to spawn crash handler process");
    }

    // SAFETY: `hProcess` is a valid open handle returned by `CreateProcessW`.
    let close_process_result =
        unsafe { windows::Win32::Foundation::CloseHandle(process_info.hProcess) };
    if let Err(error) = close_process_result {
        log::warn!("Failed to close crash handler process handle: {error}");
    }

    // SAFETY: `hThread` is a valid open handle returned by `CreateProcessW`.
    let close_thread_result =
        unsafe { windows::Win32::Foundation::CloseHandle(process_info.hThread) };
    if let Err(error) = close_thread_result {
        log::warn!("Failed to close crash handler thread handle: {error}");
    }
}

pub fn crash_server(socket: &Path, logs_dir: PathBuf) {
    let Ok(mut server) = Server::with_name(SocketName::Path(socket)) else {
        log::info!("Could not create socket; a crash server may already be running");
        return;
    };

    let shutdown = Arc::new(AtomicBool::new(false));
    let has_connection = Arc::new(AtomicBool::new(false));
    thread::Builder::new()
        .name("CrashServerTimeout".to_owned())
        .spawn({
            let shutdown = shutdown.clone();
            let has_connection = has_connection.clone();
            move || {
                thread::sleep(CRASH_HANDLER_CONNECT_TIMEOUT);
                if !has_connection.load(Ordering::SeqCst) {
                    shutdown.store(true, Ordering::SeqCst);
                }
            }
        })
        .expect("failed to spawn crash server timeout thread");

    server
        .run(
            Box::new(CrashServer {
                initialization_params: Mutex::default(),
                panic_info: Mutex::default(),
                active_gpu: Mutex::default(),
                abort_message_location: Mutex::default(),
                has_connection,
                logs_dir,
            }),
            &shutdown,
            Some(CRASH_HANDLER_PING_TIMEOUT),
        )
        .expect("failed to run crash server");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abort_message_read_len_requires_page_rounded_total() {
        assert_eq!(abort_message_read_len(0), None);
        // A non-page-rounded total indicates that the glibc layout may have changed.
        assert_eq!(abort_message_read_len(23), None);
        assert_eq!(abort_message_read_len(4097), None);
        // Exclude the four-byte size header to keep the read within the mapping.
        assert_eq!(abort_message_read_len(4096), Some(4092));
        // Clamp larger mappings to one page minus the size header.
        assert_eq!(abort_message_read_len(8192), Some(4092));
        assert_eq!(abort_message_read_len(65536), Some(4092));
    }

    #[test]
    fn test_parse_abort_message_truncates_at_nul() {
        let mut buffer = b"free(): invalid pointer\n\0".to_vec();
        buffer.resize(4092, 0);
        assert_eq!(
            parse_abort_message(&buffer),
            Some("free(): invalid pointer".to_string())
        );
    }

    #[test]
    fn test_parse_abort_message_handles_missing_nul() {
        assert_eq!(
            parse_abort_message(b"double free or corruption (out)"),
            Some("double free or corruption (out)".to_string())
        );
    }

    #[test]
    fn test_parse_abort_message_rejects_empty() {
        assert_eq!(parse_abort_message(&[]), None);
        assert_eq!(parse_abort_message(&[0; 16]), None);
        assert_eq!(parse_abort_message(b"\n \0garbage after nul"), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_read_abort_message_reads_glibc_layout_from_live_process() {
        // SAFETY: `_SC_PAGESIZE` is a valid `sysconf` selector.
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        let page_size = usize::try_from(page_size).unwrap();

        // SAFETY: The null address lets `mmap` place the two anonymous pages and
        // `MAP_ANON` makes the `-1` file descriptor valid.
        let mapping = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                2 * page_size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_ANON | libc::MAP_PRIVATE,
                -1,
                0,
            )
        };
        assert_ne!(mapping, libc::MAP_FAILED);

        // SAFETY: `mapping` spans two pages, so advancing by `page_size` is in bounds.
        let second_page = unsafe { mapping.cast::<u8>().add(page_size).cast() };

        // SAFETY: `second_page` points to the start of the second mapped page and
        // advancing by `page_size` reaches the end of the mapping.
        let protect_result = unsafe { libc::mprotect(second_page, page_size, libc::PROT_NONE) };
        assert_eq!(protect_result, 0);

        let mapping_size = u32::try_from(page_size).unwrap();
        // SAFETY: `mapping` is aligned for `u32` and points to at least four writable bytes.
        unsafe { mapping.cast::<u32>().write(mapping_size) };

        let message = b"free(): invalid pointer\n\0";
        // SAFETY: Advancing `mapping` by the `u32` header size remains within the first page.
        let message_destination = unsafe { mapping.cast::<u8>().add(mem::size_of::<u32>()) };

        // SAFETY: `message` is readable and `message_destination` is writable for
        // `message.len()` bytes and their allocations do not overlap.
        unsafe {
            std::ptr::copy_nonoverlapping(message.as_ptr(), message_destination, message.len());
        }

        let abort_message: *mut libc::c_void = mapping;
        let location = AbortMessageLocation {
            pid: process::id(),
            address: u64::try_from((&raw const abort_message).addr()).unwrap(),
        };
        assert_eq!(
            read_abort_message(location),
            Some("free(): invalid pointer".to_string())
        );

        // SAFETY: `mapping` is the address returned by `mmap` and `2 * page_size` is
        // the original mapping length.
        let unmap_result = unsafe { libc::munmap(mapping, 2 * page_size) };
        assert_eq!(unmap_result, 0);
    }
}
