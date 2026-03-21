pub mod command;
pub mod path;
pub mod rel_path;
pub mod test;

use log::{Level, Record};
use std::{borrow::Cow, cmp::Ordering, fmt::Debug, panic::Location};

/// Get an embedded file as a string.
pub fn asset_str<A: rust_embed::RustEmbed>(path: &str) -> Cow<'static, str> {
    match A::get(path).expect(path).data {
        Cow::Borrowed(bytes) => Cow::Borrowed(std::str::from_utf8(bytes).unwrap()),
        Cow::Owned(bytes) => Cow::Owned(String::from_utf8(bytes).unwrap()),
    }
}

pub fn capitalize(str: &str) -> String {
    let mut chars = str.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

pub trait ResultExt<E> {
    type Ok;

    fn log_err(self) -> Option<Self::Ok>;
}

impl<T, E> ResultExt<E> for Result<T, E>
where
    E: Debug,
{
    type Ok = T;

    #[track_caller]
    fn log_err(self) -> Option<T> {
        match self {
            Ok(value) => Some(value),
            Err(error) => {
                log_error_with_caller(*Location::caller(), error);
                None
            }
        }
    }
}

fn log_error_with_caller<E>(caller: Location<'_>, error: E)
where
    E: Debug,
{
    #[cfg(not(windows))]
    let file = caller.file();

    #[cfg(windows)]
    let file = caller.file().replace('\\', "/");

    let file = file.split_once("crates/");
    let target = file.as_ref().and_then(|(_, path)| path.split_once("/src/"));
    let module_path = target.map(|(crate_name, module)| {
        if module.starts_with(crate_name) {
            module.trim_end_matches(".rs").replace('/', "::")
        } else {
            crate_name.to_owned() + "::" + &module.trim_end_matches(".rs").replace('/', "::")
        }
    });
    let file = file.map(|(_, path)| format!("crates/{path}"));

    log::logger().log(
        &Record::builder()
            .target(module_path.as_deref().unwrap_or(""))
            .module_path(file.as_deref())
            .args(format_args!("{error:?}"))
            .file(Some(caller.file()))
            .line(Some(caller.line()))
            .level(Level::Error)
            .build(),
    );
}

/// Inserts incoming entries into entries, skipping duplicates and preserving order.
/// Result stays capped at `limit` by dropping the current last entry when a newly
/// inserted entry sorts earlier. Both inputs must already be sorted using `cmp`.
pub fn extend_sorted<T, I, F>(entries: &mut Vec<T>, incoming_entries: I, limit: usize, mut cmp: F)
where
    I: IntoIterator<Item = T>,
    F: FnMut(&T, &T) -> Ordering,
{
    let mut start_index = 0;

    for incoming_entry in incoming_entries {
        if let Err(idx) =
            entries[start_index..].binary_search_by(|entry| cmp(entry, &incoming_entry))
        {
            let index = start_index + idx;
            if entries.len() < limit {
                entries.insert(index, incoming_entry);
            } else if index < entries.len() {
                entries.pop();
                entries.insert(index, incoming_entry);
            }
            start_index = index;
        }
    }
}
