use log::Level;
use std::{
    fmt::{Arguments, Display, Formatter, Write as FmtWrite},
    fs::{File, OpenOptions},
    io::Write as IoWrite,
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
) -> std::io::Result<()> {
    let mut enabled_sinks_file = ENABLED_SINKS_FILE
        .try_lock()
        .expect("Log file lock is available during init");

    SINK_FILE_PATH
        .set(path)
        .expect("Init file output should only be called once");

    if let Some(path_rotate) = path_rotate {
        SINK_FILE_PATH_ROTATE
            .set(path_rotate)
            .expect("Init file output should only be called once");
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
) -> std::io::Result<File> {
    let size_bytes = std::fs::metadata(path).map(|metadata| metadata.len());

    match size_bytes {
        Ok(size_bytes) if size_bytes >= sink_file_size_bytes_max => {
            let file = rotate_log_file(Some(path), path_rotate)?;

            match file {
                Some(file) => Ok(file),
                None => Err(std::io::Error::other("rotation did not return a log file")),
            }
        }
        _ => OpenOptions::new().create(true).append(true).open(path),
    }
}

const LEVEL_OUTPUT_STRINGS: [&str; 6] = ["     ", "ERROR", "WARN ", "INFO ", "DEBUG", "TRACE"];

static LEVEL_ANSI_COLORS: [&str; 6] = [
    "",
    ANSI_RED,
    ANSI_YELLOW,
    ANSI_GREEN,
    ANSI_BLUE,
    ANSI_MAGENTA,
];

pub fn submit(mut record: Record) {
    if record
        .module_path
        .is_none_or(|module_path| !module_path.ends_with(".rs"))
    {
        record.line.take();
    }

    if ENABLED_SINKS_STDOUT.load(Ordering::Acquire) {
        let mut stdout = std::io::stdout().lock();
        _ = writeln!(
            &mut stdout,
            "{} {ANSI_BOLD}{}{}{ANSI_RESET} {} {}",
            chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%:z"),
            LEVEL_ANSI_COLORS[record.level as usize],
            LEVEL_OUTPUT_STRINGS[record.level as usize],
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
            chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%:z"),
            LEVEL_ANSI_COLORS[record.level as usize],
            LEVEL_OUTPUT_STRINGS[record.level as usize],
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

        impl std::io::Write for SizedWriter<'_> {
            fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
                self.file.write(buffer)?;
                self.written += buffer.len() as u64;
                Ok(buffer.len())
            }

            fn flush(&mut self) -> std::io::Result<()> {
                self.file.flush()
            }
        }

        let file_size_bytes = {
            let mut writer = SizedWriter { file, written: 0 };
            _ = writeln!(
                &mut writer,
                "{} {} {} {}",
                chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%:z"),
                LEVEL_OUTPUT_STRINGS[record.level as usize],
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
            let file = rotate_log_file(SINK_FILE_PATH.get(), SINK_FILE_PATH_ROTATE.get());

            match file {
                Ok(Some(file)) => *file_guard = Some(file),
                Ok(None) => {}
                Err(error) => {
                    eprintln!("Failed to open log file: {error}")
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

        if (self.scope[1].is_empty() && self.module_path.is_some()) || self.scope[0].is_empty() {
            formatter.write_str(self.module_path.unwrap_or("?"))?;
        } else {
            formatter.write_str(self.scope[0])?;

            for subscope in &self.scope[1..] {
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

fn rotate_log_file<PathRef>(
    path: Option<PathRef>,
    path_rotate: Option<PathRef>,
) -> std::io::Result<Option<File>>
where
    PathRef: AsRef<Path>,
{
    let path = path.as_ref().map(PathRef::as_ref);
    let rotation_error = match (path, path_rotate) {
        (Some(_), None) => Some(anyhow::anyhow!("No rotation log file path configured")),
        (None, _) => Some(anyhow::anyhow!("No log file path configured")),
        (Some(path), Some(path_rotate)) => std::fs::copy(path, path_rotate)
            .err()
            .map(|error| anyhow::anyhow!(error)),
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

    #[test]
    fn test_open_or_create_log_file_rotate() {
        let temp_fs = TempFs::new();
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

    #[test]
    fn test_open_or_create_log_file() {
        let temp_fs = TempFs::new();
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

    #[test]
    fn test_log_level_names() {
        assert_eq!(LEVEL_OUTPUT_STRINGS[Level::Error as usize], "ERROR");
        assert_eq!(LEVEL_OUTPUT_STRINGS[Level::Warn as usize], "WARN ");
        assert_eq!(LEVEL_OUTPUT_STRINGS[Level::Info as usize], "INFO ");
        assert_eq!(LEVEL_OUTPUT_STRINGS[Level::Debug as usize], "DEBUG");
        assert_eq!(LEVEL_OUTPUT_STRINGS[Level::Trace as usize], "TRACE");
    }
}
