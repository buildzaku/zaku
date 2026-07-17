use anyhow::Context as _;
use std::{
    io,
    os::windows::ffi::OsStrExt,
    path::Path,
    process::Command,
    sync::LazyLock,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS, HWND, LPARAM, WPARAM},
        System::RestartManager::{
            CCH_RM_SESSION_KEY, RmEndSession, RmGetList, RmRegisterResources, RmShutdown,
            RmStartSession,
        },
        UI::{Controls::PBM_STEPIT, WindowsAndMessaging::PostMessageW},
    },
    core::{PCWSTR, PWSTR},
};

const RETRY_INTERVAL: Duration = Duration::from_millis(50);
const RETRY_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) struct Job {
    apply: Box<dyn Fn(&Path) -> anyhow::Result<()> + Send + Sync>,
    rollback: Box<dyn Fn(&Path) -> anyhow::Result<()> + Send + Sync>,
}

impl Job {
    fn mkdir(name: &'static Path) -> Self {
        Self {
            apply: Box::new(move |app_dir| {
                let directory = app_dir.join(name);
                std::fs::create_dir_all(&directory)
                    .with_context(|| format!("failed to create directory {}", directory.display()))
            }),
            rollback: Box::new(move |app_dir| {
                let directory = app_dir.join(name);
                std::fs::remove_dir_all(&directory)
                    .with_context(|| format!("failed to remove directory {}", directory.display()))
            }),
        }
    }

    fn move_file(filename: &'static Path, new_filename: &'static Path) -> Self {
        Self {
            apply: Box::new(move |app_dir| {
                let old_file = app_dir.join(filename);
                let new_file = app_dir.join(new_filename);
                log::info!(
                    "Moving update file from {} to {}",
                    old_file.display(),
                    new_file.display()
                );
                std::fs::rename(&old_file, &new_file).with_context(|| {
                    format!(
                        "failed to move update file from {} to {}",
                        old_file.display(),
                        new_file.display()
                    )
                })
            }),
            rollback: Box::new(move |app_dir| {
                let old_file = app_dir.join(filename);
                let new_file = app_dir.join(new_filename);
                log::info!(
                    "Rolling back update file from {} to {}",
                    new_file.display(),
                    old_file.display()
                );
                std::fs::rename(&new_file, &old_file).with_context(|| {
                    format!(
                        "failed to roll back update file from {} to {}",
                        new_file.display(),
                        old_file.display()
                    )
                })
            }),
        }
    }

    fn rmdir_nofail(name: &'static Path) -> Self {
        Self {
            apply: Box::new(move |app_dir| {
                let directory = app_dir.join(name);
                match std::fs::remove_dir_all(&directory) {
                    Ok(()) => log::info!("Removed update directory {}", directory.display()),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => log::warn!(
                        "Failed to remove update directory {}: {error}",
                        directory.display()
                    ),
                }
                Ok(())
            }),
            rollback: Box::new(move |app_dir| {
                anyhow::bail!(
                    "directory removal cannot be rolled back: {}",
                    app_dir.join(name).display()
                )
            }),
        }
    }
}

pub(crate) static JOBS: LazyLock<[Job; 6]> = LazyLock::new(|| {
    [
        Job::mkdir(Path::new("old")),
        Job::move_file(Path::new("Zaku.exe"), Path::new("old\\Zaku.exe")),
        Job::move_file(Path::new("install\\Zaku.exe"), Path::new("Zaku.exe")),
        Job::rmdir_nofail(Path::new("updates")),
        Job::rmdir_nofail(Path::new("install")),
        Job::rmdir_nofail(Path::new("old")),
    ]
});

fn release_file_handles(app_dir: &Path) -> anyhow::Result<()> {
    let files_to_release = [app_dir.join("Zaku.exe")];
    let mut session = 0;
    let session_key_length = usize::try_from(CCH_RM_SESSION_KEY)
        .context("restart manager session key length should fit in usize")?
        + 1;
    let mut session_key = vec![0_u16; session_key_length];

    // SAFETY: `session` and `session_key` are valid writable outputs.
    let result = unsafe {
        RmStartSession(
            &mut session,
            Some(0),
            PWSTR::from_raw(session_key.as_mut_ptr()),
        )
    };
    if result.is_err() {
        anyhow::bail!("restart manager RmStartSession failed: {result:?}");
    }

    let _session_guard = scopeguard::guard(session, |session| {
        // SAFETY: `session` is an active Restart Manager session.
        let result = unsafe { RmEndSession(session) };
        if result.is_err() {
            log::warn!("Failed to end Restart Manager session: {result:?}");
        }
    });
    let wide_paths = files_to_release
        .iter()
        .filter(|path| path.exists())
        .map(|path| {
            path.as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    if wide_paths.is_empty() {
        return Ok(());
    }

    let paths = wide_paths
        .iter()
        .map(|path| PCWSTR::from_raw(path.as_ptr()))
        .collect::<Vec<_>>();
    // SAFETY: `session` and the null-terminated strings referenced by `paths` remain valid for the
    // duration of this call.
    let result = unsafe { RmRegisterResources(session, Some(&paths), None, None) };
    if result.is_err() {
        anyhow::bail!("restart manager RmRegisterResources failed: {result:?}");
    }

    let mut needed = 0;
    let mut count = 0;
    let mut reboot_reasons = 0;
    // SAFETY: `session` and all writable `RmGetList` arguments remain valid for the duration of
    // this call.
    let result = unsafe { RmGetList(session, &mut needed, &mut count, None, &mut reboot_reasons) };
    if result != ERROR_SUCCESS && result != ERROR_MORE_DATA {
        anyhow::bail!("restart manager RmGetList failed: {result:?}");
    }
    if needed == 0 {
        return Ok(());
    }

    log::info!("Requesting {needed} process(es) to release Zaku update files");
    // SAFETY: `session` remains valid for the duration of this call.
    let result = unsafe { RmShutdown(session, 0, None) };
    if result.is_err() {
        anyhow::bail!("restart manager RmShutdown failed: {result:?}");
    }

    Ok(())
}

pub(crate) fn perform_update(
    app_dir: &Path,
    progress_handle: Option<isize>,
    launch: bool,
) -> anyhow::Result<()> {
    if let Err(error) = release_file_handles(app_dir) {
        log::warn!("Restart Manager could not release update files: {error:#}");
    }

    execute_jobs(
        app_dir,
        JOBS.as_slice(),
        RETRY_TIMEOUT,
        RETRY_INTERVAL,
        || {
            let Some(progress_handle) = progress_handle else {
                return;
            };
            // SAFETY: `progress_handle` reconstructs the updater progress control `HWND`, which
            // remains valid until `WM_TERMINATE`.
            if let Err(error) = unsafe {
                PostMessageW(
                    Some(HWND(progress_handle as *mut core::ffi::c_void)),
                    PBM_STEPIT,
                    WPARAM(0),
                    LPARAM(0),
                )
            } {
                log::warn!("Failed to report updater progress: {error}");
            }
        },
    )?;

    if launch {
        Command::new(app_dir.join("Zaku.exe"))
            .spawn()
            .context("failed to launch updated Zaku")?;
    }
    log::info!("Zaku update completed successfully");
    Ok(())
}

fn execute_jobs(
    app_dir: &Path,
    jobs: &[Job],
    retry_timeout: Duration,
    retry_interval: Duration,
    mut job_completed: impl FnMut(),
) -> anyhow::Result<()> {
    let mut applied_jobs = 0;

    for job in jobs {
        let started_at = Instant::now();
        let result: anyhow::Result<()> = loop {
            let Err(error) = (job.apply)(app_dir) else {
                break Ok(());
            };
            let Some(io_error) = error.downcast_ref::<io::Error>() else {
                break Err(error);
            };
            if io_error.kind() == io::ErrorKind::NotFound {
                break Err(error);
            }
            if started_at.elapsed() >= retry_timeout {
                break Err(error.context("timed out while applying update job"));
            }

            log::warn!("Update operation failed and will be retried: {error:#}");
            std::thread::sleep(retry_interval);
        };

        if let Err(error) = result {
            if applied_jobs == 0 {
                return Err(error.context("update failed before any changes were applied"));
            }

            let mut rollback_errors = Vec::new();
            for job in jobs.iter().take(applied_jobs).rev() {
                if let Err(rollback_error) = (job.rollback)(app_dir) {
                    rollback_errors.push(format!("{rollback_error:#}"));
                }
            }

            if rollback_errors.is_empty() {
                return Err(error.context("update failed; rollback succeeded"));
            }
            anyhow::bail!(
                "update failed: {error:#}; rollback failed: {}",
                rollback_errors.join("; ")
            );
        }

        applied_jobs += 1;
        job_completed();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[test]
    fn test_execute_jobs() {
        let app_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(app_dir.path().join("install")).unwrap();
        std::fs::create_dir(app_dir.path().join("updates")).unwrap();
        std::fs::write(app_dir.path().join("Zaku.exe"), "old").unwrap();
        std::fs::write(app_dir.path().join("install").join("Zaku.exe"), "new").unwrap();
        std::fs::write(
            app_dir.path().join("updates").join("versions.txt"),
            "26.1.0",
        )
        .unwrap();
        let completed_jobs = AtomicUsize::new(0);

        execute_jobs(
            app_dir.path(),
            JOBS.as_slice(),
            Duration::ZERO,
            Duration::ZERO,
            || {
                completed_jobs.fetch_add(1, Ordering::SeqCst);
            },
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(app_dir.path().join("Zaku.exe")).unwrap(),
            "new"
        );
        assert!(
            !app_dir.path().join("install").exists(),
            "staged installation should be removed"
        );
        assert!(
            !app_dir.path().join("updates").exists(),
            "update metadata should be removed"
        );
        assert!(
            !app_dir.path().join("old").exists(),
            "old installation should be removed"
        );
        assert_eq!(
            completed_jobs.load(Ordering::SeqCst),
            JOBS.len()
        );
    }

    #[test]
    fn test_execute_jobs_rolls_back_partial_update() {
        let app_dir = tempfile::tempdir().unwrap();
        std::fs::write(app_dir.path().join("Zaku.exe"), "old").unwrap();
        let jobs = [
            Job::mkdir(Path::new("old")),
            Job::move_file(Path::new("Zaku.exe"), Path::new("old\\Zaku.exe")),
            Job {
                apply: Box::new(|_| anyhow::bail!("simulated failure")),
                rollback: Box::new(|_| Ok(())),
            },
        ];

        let error =
            execute_jobs(app_dir.path(), &jobs, Duration::ZERO, Duration::ZERO, || {}).unwrap_err();

        assert_eq!(error.to_string(), "update failed; rollback succeeded");
        assert_eq!(
            std::fs::read_to_string(app_dir.path().join("Zaku.exe")).unwrap(),
            "old"
        );
        assert!(
            !app_dir.path().join("old").exists(),
            "rollback should remove backup directory"
        );
    }

    #[test]
    fn test_execute_jobs_retries_transient_io_errors() {
        let app_dir = tempfile::tempdir().unwrap();
        let attempts = Arc::new(AtomicUsize::new(0));
        let jobs = [Job {
            apply: Box::new({
                let attempts = attempts.clone();
                move |_| {
                    if attempts.fetch_add(1, Ordering::SeqCst) < 2 {
                        Err(io::Error::new(io::ErrorKind::PermissionDenied, "file locked").into())
                    } else {
                        Ok(())
                    }
                }
            }),
            rollback: Box::new(|_| Ok(())),
        }];

        execute_jobs(
            app_dir.path(),
            &jobs,
            Duration::from_secs(1),
            Duration::ZERO,
            || {},
        )
        .unwrap();

        assert_eq!(
            attempts.load(Ordering::SeqCst),
            3
        );
    }

    #[test]
    fn test_execute_jobs_stops_after_timeout() {
        let app_dir = tempfile::tempdir().unwrap();
        let attempts = Arc::new(AtomicUsize::new(0));
        let jobs = [
            Job {
                apply: Box::new({
                    let attempts = attempts.clone();
                    move |_| {
                        attempts.fetch_add(1, Ordering::SeqCst);
                        Err(io::Error::new(io::ErrorKind::PermissionDenied, "file locked").into())
                    }
                }),
                rollback: Box::new(|_| panic!("timed-out job should not be rolled back")),
            },
            Job {
                apply: Box::new(|_| panic!("next job should not run")),
                rollback: Box::new(|_| Ok(())),
            },
        ];

        let error =
            execute_jobs(app_dir.path(), &jobs, Duration::ZERO, Duration::ZERO, || {}).unwrap_err();

        assert_eq!(
            format!("{error:#}"),
            "update failed before any changes were applied: timed out while applying update job: file locked"
        );
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }
}
