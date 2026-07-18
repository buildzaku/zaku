#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

cfg_select! {
    any(target_os = "linux", target_os = "macos") => {
        fn main() {}
    }
    target_os = "windows" => {
        mod dialog;
        mod updater;

        use anyhow::Context as _;
        use std::borrow::Cow;
        use windows::{
            Win32::{
                Foundation::{HWND, LPARAM, WPARAM},
                UI::WindowsAndMessaging::{
                    DispatchMessageW, GetMessageW, MB_ICONERROR, MB_SYSTEMMODAL, MSG, MessageBoxW,
                    PostMessageW, WM_USER,
                },
            },
            core::{BOOL, Error as WindowsError, HSTRING},
        };

        use crate::{dialog::create_dialog_window, updater::perform_update};

        const WM_TERMINATE: u32 = WM_USER + 1;

        struct Args {
            launch: bool,
        }

        fn main() {
            if let Err(error) = run() {
                log::error!("Zaku update failed: {error:?}");
                show_error(format!("{error:#}"));
            }
        }

        fn run() -> anyhow::Result<()> {
            let helper_dir = std::env::current_exe()?
                .parent()
                .context("no parent directory for updater_windows.exe")?
                .to_path_buf();
            init_log()?;
            let app_dir = helper_dir
                .parent()
                .context("no parent installation directory")?
                .to_path_buf();

            log::info!("Starting Zaku update");
            let dialog = create_dialog_window()?;
            let window_handle = dialog.window.0 as isize;
            let progress_handle = dialog.progress_bar.0 as isize;
            let args = parse_args(std::env::args().skip(1));
            drop(std::thread::spawn(move || {
                if let Err(error) = perform_update(&app_dir, Some(progress_handle), args.launch) {
                    log::error!("Zaku update failed: {error:?}");
                    show_error(format!("{error:#}"));
                }

                // SAFETY: `window_handle` reconstructs the updater window `HWND`, which remains
                // valid until `WM_TERMINATE`.
                if let Err(error) = unsafe {
                    PostMessageW(
                        Some(HWND(window_handle as *mut core::ffi::c_void)),
                        WM_TERMINATE,
                        WPARAM(0),
                        LPARAM(0),
                    )
                } {
                    log::error!("Failed to close updater window: {error}");
                }
            }));

            let mut message = MSG::default();
            loop {
                // SAFETY: `message` provides writable storage for the duration of this call.
                let BOOL(message_status) =
                    unsafe { GetMessageW(&raw mut message, None, 0, 0) };
                match message_status {
                    -1 => {
                        return Err(WindowsError::from_thread())
                            .context("failed to read updater window message");
                    }
                    0 => break,
                    _ => {
                        // SAFETY: `message` was populated by `GetMessageW` and remains valid for the
                        // duration of this call.
                        unsafe { DispatchMessageW(&raw const message) };
                    }
                }
            }

            Ok(())
        }

        fn init_log() -> anyhow::Result<()> {
            std::fs::create_dir_all(path::logs_dir())?;
            logger::init();
            logger::init_output_file(
                path::updater_log_file().clone(),
                Some(path::old_updater_log_file().clone()),
            )?;
            Ok(())
        }

        fn parse_args(input: impl IntoIterator<Item = String>) -> Args {
            let mut args = Args { launch: true };
            let mut input = input.into_iter();

            if let Some(argument) = input.next() {
                let launch_argument = if argument == "--launch" {
                    input.next().map(Cow::Owned)
                } else {
                    argument.strip_prefix("--launch=").map(Cow::Borrowed)
                };

                if launch_argument.as_deref() == Some("false") {
                    args.launch = false;
                }
            }

            args
        }

        fn show_error(mut content: String) {
            if let Some((index, _)) = content.char_indices().nth(600) {
                content.truncate(index);
                content.push_str("...\n");
            }

            // SAFETY: `HSTRING::from(content)` and the window title remain valid for the duration
            // of this call.
            let result = unsafe {
                MessageBoxW(
                    None,
                    &HSTRING::from(content),
                    windows::core::w!("Zaku update failed"),
                    MB_ICONERROR | MB_SYSTEMMODAL,
                )
            };
            if result.0 == 0 {
                log::error!("Failed to show updater error dialog");
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;

            #[test]
            fn test_parse_args() {
                assert!(parse_args(["--launch".into(), "true".into()]).launch);
                assert!(!parse_args(["--launch".into(), "false".into()]).launch);
                assert!(parse_args(["--launch=true".into()]).launch);
                assert!(!parse_args(["--launch=false".into()]).launch);
                assert!(parse_args([]).launch);
                assert!(parse_args(["--launch".into()]).launch);
                assert!(parse_args(["--launch=".into()]).launch);
                assert!(parse_args(["--launch=invalid".into()]).launch);
            }
        }
    }
}
