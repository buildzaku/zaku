use anyhow::anyhow;
use jiff::Zoned;
use log::Level;
use std::{
    fmt::{Arguments, Display, Formatter, Write as FmtWrite},
    fs::{File, OpenOptions},
    io::{self, Write as IoWrite},
    path::{Path, PathBuf},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use crate::{SCOPE_STRING_SEP_CHAR, ScopeRef};

static ENABLED_SINKS_FILE: Mutex<Option<File>> = Mutex::new(None);
static SINK_FILE_PATH: OnceLock<&'static PathBuf> = OnceLock::new();
static SINK_FILE_PATH_ROTATE: OnceLock<&'static PathBuf> = OnceLock::new();
static ENABLED_SINKS_STDOUT: AtomicBool = AtomicBool::new(false);
static ENABLED_SINKS_STDERR: AtomicBool = AtomicBool::new(false);
static SINK_FILE_SIZE_BYTES: AtomicU64 = AtomicU64::new(0);

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_BOLD: &str = "\x1b[1m";
const ANSI_RED: &str = "\x1b[31m";
const ANSI_YELLOW: &str = "\x1b[33m";
const ANSI_GREEN: &str = "\x1b[32m";
const ANSI_BLUE: &str = "\x1b[34m";
const ANSI_MAGENTA: &str = "\x1b[35m";
const SINK_FILE_SIZE_BYTES_MAX: u64 = 1024 * 1024;

pub struct Record<'a> {
    pub scope: ScopeRef<'a>,
    pub level: Level,
    pub message: &'a Arguments<'a>,
    pub module_path: Option<&'a str>,
    pub line: Option<u32>,
}

pub fn init_output_stdout() {
    ENABLED_SINKS_STDOUT.store(true, Ordering::Release);
}

pub fn init_output_stderr() {
    ENABLED_SINKS_STDERR.store(true, Ordering::Release);
}

pub fn init_output_file(
    path: &'static PathBuf,
    path_rotate: Option<&'static PathBuf>,
) -> io::Result<()> {
    let mut enabled_sinks_file = ENABLED_SINKS_FILE.try_lock().map_err(|error| {
        io::Error::other(format!("log file lock unavailable during init: {error}"))
    })?;

    SINK_FILE_PATH.set(path).map_err(|path| {
        io::Error::other(format!(
            "init file output already set at {}",
            path.display()
        ))
    })?;

    if let Some(path_rotate) = path_rotate {
        SINK_FILE_PATH_ROTATE
            .set(path_rotate)
            .map_err(|path_rotate| {
                io::Error::other(format!(
                    "init file output rotation already set at {}",
                    path_rotate.display()
                ))
            })?;
    }

    let file = open_or_create_log_file(path, path_rotate, SINK_FILE_SIZE_BYTES_MAX)?;
    SINK_FILE_SIZE_BYTES.store(
        file.metadata().map_or(0, |metadata| metadata.len()),
        Ordering::Release,
    );
    *enabled_sinks_file = Some(file);

    Ok(())
}

fn open_or_create_log_file(
    path: &PathBuf,
    path_rotate: Option<&PathBuf>,
    sink_file_size_bytes_max: u64,
) -> io::Result<File> {
    let size_bytes = std::fs::metadata(path).map(|metadata| metadata.len());

    match size_bytes {
        Ok(size_bytes) if size_bytes >= sink_file_size_bytes_max => {
            let file = rotate_log_file(Some(path.as_path()), path_rotate.map(PathBuf::as_path))?;

            match file {
                Some(file) => Ok(file),
                None => Err(io::Error::other("rotation did not return a log file")),
            }
        }
        _ => OpenOptions::new().create(true).append(true).open(path),
    }
}

pub fn submit(mut record: Record) {
    if record.module_path.is_none_or(|module_path| {
        !Path::new(module_path)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("rs"))
    }) {
        record.line.take();
    }

    let (text, ansi_color) = match record.level {
        Level::Error => ("ERROR", ANSI_RED),
        Level::Warn => ("WARN ", ANSI_YELLOW),
        Level::Info => ("INFO ", ANSI_GREEN),
        Level::Debug => ("DEBUG", ANSI_BLUE),
        Level::Trace => ("TRACE", ANSI_MAGENTA),
    };

    if ENABLED_SINKS_STDOUT.load(Ordering::Acquire) {
        let mut stdout = std::io::stdout().lock();
        _ = writeln!(
            &mut stdout,
            "{} {ANSI_BOLD}{}{}{ANSI_RESET} {} {}",
            Zoned::now().strftime("%Y-%m-%dT%H:%M:%S%:z"),
            ansi_color,
            text,
            SourceFmt {
                scope: record.scope,
                module_path: record.module_path,
                line: record.line,
                ansi: true,
            },
            record.message
        );
    } else if ENABLED_SINKS_STDERR.load(Ordering::Acquire) {
        let mut stdout = std::io::stderr().lock();
        _ = writeln!(
            &mut stdout,
            "{} {ANSI_BOLD}{}{}{ANSI_RESET} {} {}",
            Zoned::now().strftime("%Y-%m-%dT%H:%M:%S%:z"),
            ansi_color,
            text,
            SourceFmt {
                scope: record.scope,
                module_path: record.module_path,
                line: record.line,
                ansi: true,
            },
            record.message
        );
    }

    let mut file_guard = ENABLED_SINKS_FILE.lock().unwrap_or_else(|handle| {
        ENABLED_SINKS_FILE.clear_poison();
        handle.into_inner()
    });

    if let Some(file) = file_guard.as_mut() {
        struct SizedWriter<'a> {
            file: &'a mut File,
            written: u64,
        }

        impl io::Write for SizedWriter<'_> {
            fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
                self.file.write(buffer)?;
                self.written += buffer.len() as u64;
                Ok(buffer.len())
            }

            fn flush(&mut self) -> io::Result<()> {
                self.file.flush()
            }
        }

        let file_size_bytes = {
            let mut writer = SizedWriter { file, written: 0 };
            _ = writeln!(
                &mut writer,
                "{} {} {} {}",
                Zoned::now().strftime("%Y-%m-%dT%H:%M:%S%:z"),
                text,
                SourceFmt {
                    scope: record.scope,
                    module_path: record.module_path,
                    line: record.line,
                    ansi: false,
                },
                record.message
            );
            SINK_FILE_SIZE_BYTES.fetch_add(writer.written, Ordering::AcqRel) + writer.written
        };

        if file_size_bytes > SINK_FILE_SIZE_BYTES_MAX {
            *file_guard = None;
            let file = rotate_log_file(
                SINK_FILE_PATH.get().map(|path| path.as_path()),
                SINK_FILE_PATH_ROTATE.get().map(|path| path.as_path()),
            );

            match file {
                Ok(Some(file)) => *file_guard = Some(file),
                Ok(None) => {}
                Err(error) => {
                    eprintln!("Failed to open log file: {error}");
                }
            }
            SINK_FILE_SIZE_BYTES.store(0, Ordering::Release);
        }
    }
}

pub fn flush() {
    if ENABLED_SINKS_STDOUT.load(Ordering::Acquire) {
        _ = std::io::stdout().lock().flush();
    }

    let mut file = ENABLED_SINKS_FILE.lock().unwrap_or_else(|handle| {
        ENABLED_SINKS_FILE.clear_poison();
        handle.into_inner()
    });

    if let Some(file) = file.as_mut()
        && let Err(error) = file.flush()
    {
        eprintln!("Failed to flush log file: {error}");
    }
}

struct SourceFmt<'a> {
    scope: ScopeRef<'a>,
    module_path: Option<&'a str>,
    line: Option<u32>,
    ansi: bool,
}

impl Display for SourceFmt<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_char('[')?;

        if self.ansi {
            formatter.write_str(ANSI_BOLD)?;
        }

        let [first_scope, second_scope, remaining_scopes @ ..] = self.scope;

        if (second_scope.is_empty() && self.module_path.is_some()) || first_scope.is_empty() {
            formatter.write_str(self.module_path.unwrap_or("?"))?;
        } else {
            formatter.write_str(first_scope)?;

            for subscope in std::iter::once(second_scope).chain(remaining_scopes) {
                if subscope.is_empty() {
                    break;
                }
                formatter.write_char(SCOPE_STRING_SEP_CHAR)?;
                formatter.write_str(subscope)?;
            }
        }

        if let Some(line) = self.line {
            formatter.write_char(':')?;
            line.fmt(formatter)?;
        }

        if self.ansi {
            formatter.write_str(ANSI_RESET)?;
        }

        formatter.write_char(']')?;
        Ok(())
    }
}

fn rotate_log_file(path: Option<&Path>, path_rotate: Option<&Path>) -> io::Result<Option<File>> {
    let rotation_error = match (path, path_rotate) {
        (Some(_), None) => Some(anyhow!("No rotation log file path configured")),
        (None, _) => Some(anyhow!("No log file path configured")),
        (Some(path), Some(path_rotate)) => std::fs::copy(path, path_rotate)
            .err()
            .map(|error| anyhow!(error)),
    };

    if let Some(error) = rotation_error {
        eprintln!("Log file rotation failed. Truncating log file anyways: {error}");
    }

    path.map(|path| {
        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
    })
    .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    use fs::TempFs;
    use gpui::TestAppContext;

    #[gpui::test]
    fn test_open_or_create_log_file_rotate(cx: &mut TestAppContext) {
        let temp_fs = TempFs::new(cx.executor());
        let log_file_path = temp_fs.path().join("Zaku.log");
        let old_log_file_path = temp_fs.path().join("Zaku.log.old");

        let contents = String::from("Hello, world!");
        std::fs::write(&log_file_path, &contents).unwrap();

        open_or_create_log_file(&log_file_path, Some(&old_log_file_path), 4).unwrap();

        assert!(log_file_path.exists());
        assert_eq!(log_file_path.metadata().unwrap().len(), 0);
        assert!(old_log_file_path.exists());
        assert_eq!(std::fs::read_to_string(&log_file_path).unwrap(), "");
    }

    #[gpui::test]
    fn test_open_or_create_log_file(cx: &mut TestAppContext) {
        let temp_fs = TempFs::new(cx.executor());
        let log_file_path = temp_fs.path().join("Zaku.log");
        let old_log_file_path = temp_fs.path().join("Zaku.log.old");

        let contents = String::from("Hello, world!");
        std::fs::write(&log_file_path, &contents).unwrap();

        open_or_create_log_file(&log_file_path, Some(&old_log_file_path), !0).unwrap();

        assert!(log_file_path.exists());
        assert_eq!(log_file_path.metadata().unwrap().len(), 13);
        assert!(!old_log_file_path.exists());
        assert_eq!(std::fs::read_to_string(&log_file_path).unwrap(), contents);
    }
}
