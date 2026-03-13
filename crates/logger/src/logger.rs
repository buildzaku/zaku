mod env_config;
pub mod filter;
pub mod sink;

pub use log as log_impl;
pub use sink::{flush, init_output_file, init_output_stderr, init_output_stdout};

use log::{LevelFilter, Log, Metadata, Record};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

pub const SCOPE_DEPTH_MAX: usize = 4;

pub fn init() {
    if let Err(error) = try_init(None) {
        log::error!("{error}");
        eprintln!("{error}");
    }
}

pub fn try_init(filter: Option<String>) -> anyhow::Result<()> {
    log::set_logger(&LOGGER)?;
    log::set_max_level(LevelFilter::max());
    process_env(filter);
    filter::refresh_from_settings(&HashMap::default());
    Ok(())
}

pub fn init_test() {
    if get_env_config().is_some() && try_init(None).is_ok() {
        init_output_stdout();
    }
}

fn get_env_config() -> Option<String> {
    std::env::var("ZAKU_LOG")
        .or_else(|_| std::env::var("RUST_LOG"))
        .ok()
        .or_else(|| {
            if std::env::var("CI").is_ok() {
                Some("info".to_owned())
            } else {
                None
            }
        })
}

pub fn process_env(filter: Option<String>) {
    let Some(env_config) = get_env_config().or(filter) else {
        return;
    };
    match env_config::parse(&env_config) {
        Ok(filter) => {
            filter::init_env_filter(filter);
        }
        Err(error) => {
            eprintln!("Failed to parse log filter: {error}");
        }
    }
}

static LOGGER: GlobalLogger = GlobalLogger {};

pub struct GlobalLogger {}

impl Log for GlobalLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        filter::is_possibly_enabled_level(metadata.level())
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let module_path = record.module_path().or(record.file());

        let (crate_name_scope, module_scope) = match module_path {
            Some(module_path) => {
                let crate_name = private::extract_crate_name_from_module_path(module_path);
                let crate_name_scope = private::scope_ref_new(&[crate_name]);
                let module_scope = private::scope_ref_new(&[module_path]);
                (crate_name_scope, module_scope)
            }
            None => (private::scope_new(&[]), private::scope_new(&["*unknown*"])),
        };

        let level = record.metadata().level();

        if !filter::is_scope_enabled(&crate_name_scope, Some(record.target()), level) {
            return;
        }

        sink::submit(sink::Record {
            scope: module_scope,
            level,
            message: record.args(),
            module_path,
            line: record.line(),
        });
    }

    fn flush(&self) {
        sink::flush();
    }
}

#[macro_export]
macro_rules! log {
    ($logger:expr, $level:expr, $($arg:tt)+) => {
        let level = $level;
        let logger = $logger;
        let enabled = $crate::filter::is_scope_enabled(&logger.scope, Some(module_path!()), level);

        if enabled {
            $crate::sink::submit($crate::sink::Record {
                scope: logger.scope,
                level,
                message: &format_args!($($arg)+),
                module_path: Some(module_path!()),
                line: Some(line!()),
            });
        }
    }
}

#[macro_export]
macro_rules! trace {
    ($logger:expr => $($arg:tt)+) => {
        $crate::log!($logger, $crate::log_impl::Level::Trace, $($arg)+);
    };
    ($($arg:tt)+) => {
        $crate::log!($crate::default_logger!(), $crate::log_impl::Level::Trace, $($arg)+);
    };
}

#[macro_export]
macro_rules! debug {
    ($logger:expr => $($arg:tt)+) => {
        $crate::log!($logger, $crate::log_impl::Level::Debug, $($arg)+);
    };
    ($($arg:tt)+) => {
        $crate::log!($crate::default_logger!(), $crate::log_impl::Level::Debug, $($arg)+);
    };
}

#[macro_export]
macro_rules! info {
    ($logger:expr => $($arg:tt)+) => {
        $crate::log!($logger, $crate::log_impl::Level::Info, $($arg)+);
    };
    ($($arg:tt)+) => {
        $crate::log!($crate::default_logger!(), $crate::log_impl::Level::Info, $($arg)+);
    };
}

#[macro_export]
macro_rules! warn {
    ($logger:expr => $($arg:tt)+) => {
        $crate::log!($logger, $crate::log_impl::Level::Warn, $($arg)+);
    };
    ($($arg:tt)+) => {
        $crate::log!($crate::default_logger!(), $crate::log_impl::Level::Warn, $($arg)+);
    };
}

#[macro_export]
macro_rules! error {
    ($logger:expr => $($arg:tt)+) => {
        $crate::log!($logger, $crate::log_impl::Level::Error, $($arg)+);
    };
    ($($arg:tt)+) => {
        $crate::log!($crate::default_logger!(), $crate::log_impl::Level::Error, $($arg)+);
    };
}

#[macro_export]
macro_rules! time {
    ($logger:expr => $name:expr) => {
        $crate::Timer::new($logger, $name)
    };
    ($name:expr) => {
        $crate::time!($crate::default_logger!() => $name)
    };
}

#[macro_export]
macro_rules! scoped {
    ($parent:expr => $name:expr) => {{
        $crate::private::scoped_logger($parent, $name)
    }};
    ($name:expr) => {
        $crate::scoped!($crate::default_logger!() => $name)
    };
}

#[macro_export]
macro_rules! default_logger {
    () => {
        $crate::Logger {
            scope: $crate::private::scope_new(&[$crate::crate_name!()]),
        }
    };
}

#[macro_export]
macro_rules! crate_name {
    () => {
        $crate::private::extract_crate_name_from_module_path(module_path!())
    };
}

pub mod private {
    use super::*;

    pub const fn extract_crate_name_from_module_path(module_path: &str) -> &str {
        let module_path_bytes = module_path.as_bytes();
        let mut index = module_path_bytes.len();
        let mut byte_index = 0;

        while byte_index + 1 < module_path_bytes.len() {
            if module_path_bytes[byte_index] == b':' && module_path_bytes[byte_index + 1] == b':' {
                index = byte_index;
                break;
            }
            byte_index += 1;
        }

        let Some((crate_name, _)) = module_path.split_at_checked(index) else {
            return module_path;
        };

        crate_name
    }

    pub const fn scoped_logger(parent: Logger, name: &'static str) -> Logger {
        let mut scope = parent.scope;
        let mut index = 1;

        while index < scope.len() && !scope[index].is_empty() {
            index += 1;
        }

        if index >= scope.len() {
            #[cfg(debug_assertions)]
            {
                panic!("Scope overflow trying to add scope... ignoring scope");
            }
        }

        scope[index] = name;
        Logger { scope }
    }

    pub const fn scope_new(scopes: &[&'static str]) -> Scope {
        scope_ref_new(scopes)
    }

    pub const fn scope_ref_new<'a>(scopes: &[&'a str]) -> ScopeRef<'a> {
        assert!(scopes.len() <= SCOPE_DEPTH_MAX);
        let mut scope = [""; SCOPE_DEPTH_MAX];
        let mut index = 0;

        while index < scopes.len() {
            scope[index] = scopes[index];
            index += 1;
        }

        scope
    }

    pub fn scope_alloc_new(scopes: &[&str]) -> ScopeAlloc {
        assert!(scopes.len() <= SCOPE_DEPTH_MAX);
        let mut scope = [""; SCOPE_DEPTH_MAX];
        scope[0..scopes.len()].copy_from_slice(scopes);
        scope.map(|scope_name| scope_name.to_string())
    }

    pub fn scope_to_alloc(scope: &Scope) -> ScopeAlloc {
        scope.map(|scope_name| scope_name.to_string())
    }
}

pub type Scope = [&'static str; SCOPE_DEPTH_MAX];
pub type ScopeRef<'a> = [&'a str; SCOPE_DEPTH_MAX];
pub type ScopeAlloc = [String; SCOPE_DEPTH_MAX];
const SCOPE_STRING_SEP_STR: &str = ".";
const SCOPE_STRING_SEP_CHAR: char = '.';

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Logger {
    pub scope: Scope,
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        filter::is_possibly_enabled_level(metadata.level())
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let level = record.metadata().level();

        if !filter::is_scope_enabled(&self.scope, Some(record.target()), level) {
            return;
        }

        sink::submit(sink::Record {
            scope: self.scope,
            level,
            message: record.args(),
            module_path: record.module_path(),
            line: record.line(),
        });
    }

    fn flush(&self) {
        sink::flush();
    }
}

pub struct Timer {
    pub logger: Logger,
    pub start_time: Instant,
    pub name: &'static str,
    pub warn_if_longer_than: Option<Duration>,
    pub done: bool,
}

impl Drop for Timer {
    fn drop(&mut self) {
        self.finish();
    }
}

impl Timer {
    #[must_use = "Timer will stop when dropped, the result of this function should be saved in a variable prefixed with `_` if it should stop when dropped"]
    pub fn new(logger: Logger, name: &'static str) -> Self {
        Self {
            logger,
            name,
            start_time: Instant::now(),
            warn_if_longer_than: None,
            done: false,
        }
    }

    pub fn warn_if_gt(mut self, warn_limit: Duration) -> Self {
        self.warn_if_longer_than = Some(warn_limit);
        self
    }

    pub fn end(mut self) {
        self.finish();
    }

    fn finish(&mut self) {
        if self.done {
            return;
        }

        let elapsed = self.start_time.elapsed();

        if let Some(warn_limit) = self.warn_if_longer_than
            && elapsed > warn_limit
        {
            crate::warn!(
                self.logger =>
                "Timer '{}' took {:?}. Which was longer than the expected limit of {:?}",
                self.name,
                elapsed,
                warn_limit
            );
            self.done = true;
            return;
        }

        crate::trace!(
            self.logger =>
            "Timer '{}' finished in {:?}",
            self.name,
            elapsed
        );
        self.done = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crate_name() {
        assert_eq!(crate_name!(), "logger");
        assert_eq!(
            private::extract_crate_name_from_module_path(
                "test_\u{26A1}\u{FE0F}_crate::some_module",
            ),
            "test_\u{26A1}\u{FE0F}_crate"
        );
        assert_eq!(
            private::extract_crate_name_from_module_path(
                "test_crate_\u{26A1}\u{FE0F}::some_module",
            ),
            "test_crate_\u{26A1}\u{FE0F}"
        );
        assert_eq!(
            private::extract_crate_name_from_module_path(
                "test_crate_:\u{26A1}\u{FE0F}:some_module",
            ),
            "test_crate_:\u{26A1}\u{FE0F}:some_module"
        );
        assert_eq!(
            private::extract_crate_name_from_module_path(
                "test_crate_::\u{26A1}\u{FE0F}some_module",
            ),
            "test_crate_"
        );
    }
}
