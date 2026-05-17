#[cfg(target_os = "windows")]
use anyhow::Context;

#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::os::unix::ffi::OsStrExt;

use std::{
    borrow::Cow,
    cmp::Ordering,
    ffi::OsStr,
    fmt::{self, Debug, Display, Formatter},
    path::Path,
    sync::Arc,
};

#[cfg(target_os = "windows")]
use tendril::fmt::{Format, WTF8};

use crate::rel_path::RelPath;

pub trait PathExt {
    fn try_from_bytes<'a>(bytes: &'a [u8]) -> anyhow::Result<Self>
    where
        Self: From<&'a Path>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PathStyle {
    Posix,
    Windows,
}

impl PathStyle {
    #[cfg(target_os = "windows")]
    pub const fn local() -> Self {
        Self::Windows
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    pub const fn local() -> Self {
        Self::Posix
    }

    pub fn primary_separator(self) -> &'static str {
        match self {
            Self::Posix => "/",
            Self::Windows => "\\",
        }
    }

    pub fn separators(self) -> &'static [&'static str] {
        match self {
            Self::Posix => &["/"],
            Self::Windows => &["\\", "/"],
        }
    }

    pub fn is_windows(self) -> bool {
        self == Self::Windows
    }

    pub fn strip_prefix<'a>(self, child: &'a Path, parent: &'a Path) -> Option<Cow<'a, RelPath>> {
        let parent = parent.to_str()?;
        if parent.is_empty() {
            return RelPath::new(child, self).ok();
        }

        let parent = self
            .separators()
            .iter()
            .find_map(|separator| parent.strip_suffix(separator))
            .unwrap_or(parent);
        let child = child.to_str()?;

        let stripped = if self.is_windows()
            && child.as_bytes().get(1) == Some(&b':')
            && parent.as_bytes().get(1) == Some(&b':')
            && child.as_bytes()[0].eq_ignore_ascii_case(&parent.as_bytes()[0])
        {
            child[2..].strip_prefix(&parent[2..])?
        } else {
            child.strip_prefix(parent)?
        };

        if let Some(relative_path) = self
            .separators()
            .iter()
            .find_map(|separator| stripped.strip_prefix(separator))
        {
            RelPath::new(relative_path.as_ref(), self).ok()
        } else if stripped.is_empty() {
            Some(Cow::Borrowed(RelPath::empty()))
        } else {
            None
        }
    }
}

pub fn is_absolute(path: &str, path_style: PathStyle) -> bool {
    path.starts_with('/')
        || path_style == PathStyle::Windows
            && (path.starts_with('\\')
                || path
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_ascii_alphabetic())
                    && path[1..]
                        .strip_prefix(':')
                        .is_some_and(|suffix| suffix.starts_with('/') || suffix.starts_with('\\')))
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum SortOrder {
    #[default]
    Default,
    Upper,
    Lower,
    Unicode,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum SortMode {
    #[default]
    DirectoriesFirst,
    Mixed,
    FilesFirst,
}

fn compare_numeric_segments<I>(
    left_chars: &mut std::iter::Peekable<I>,
    right_chars: &mut std::iter::Peekable<I>,
) -> Ordering
where
    I: Iterator<Item = char>,
{
    let mut left_digits = String::new();
    let mut right_digits = String::new();

    while let Some(&c) = left_chars.peek() {
        if !c.is_ascii_digit() {
            break;
        }

        left_digits.push(c);
        left_chars.next();
    }

    while let Some(&c) = right_chars.peek() {
        if !c.is_ascii_digit() {
            break;
        }

        right_digits.push(c);
        right_chars.next();
    }

    match left_digits.len().cmp(&right_digits.len()) {
        Ordering::Equal => match left_digits.cmp(&right_digits) {
            Ordering::Equal => Ordering::Equal,
            ordering => ordering,
        },
        ordering => {
            if let (Ok(left_value), Ok(right_value)) =
                (left_digits.parse::<u128>(), right_digits.parse::<u128>())
            {
                match left_value.cmp(&right_value) {
                    Ordering::Equal => ordering,
                    ord => ord,
                }
            } else {
                left_digits.cmp(&right_digits)
            }
        }
    }
}

pub fn natural_sort(left: &str, right: &str) -> Ordering {
    let mut left_chars = left.chars().peekable();
    let mut right_chars = right.chars().peekable();

    loop {
        match (left_chars.peek(), right_chars.peek()) {
            (None, None) => {
                return right.cmp(left);
            }
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(&left_char), Some(&right_char)) => {
                if left_char.is_ascii_digit() && right_char.is_ascii_digit() {
                    match compare_numeric_segments(&mut left_chars, &mut right_chars) {
                        Ordering::Equal => {}
                        ordering => return ordering,
                    }
                } else {
                    match left_char
                        .to_ascii_lowercase()
                        .cmp(&right_char.to_ascii_lowercase())
                    {
                        Ordering::Equal => {
                            left_chars.next();
                            right_chars.next();
                        }
                        ordering => return ordering,
                    }
                }
            }
        }
    }
}

fn natural_sort_no_tiebreak(left: &str, right: &str) -> Ordering {
    if left.eq_ignore_ascii_case(right) {
        Ordering::Equal
    } else {
        natural_sort(left, right)
    }
}

fn stem_and_extension(file_name: &str) -> (Option<&str>, Option<&str>) {
    if file_name.is_empty() {
        return (None, None);
    }

    match file_name.rsplit_once('.') {
        None => (Some(file_name), None),
        Some((before, after)) => {
            if before.is_empty() {
                (Some(file_name), None)
            } else {
                (Some(before), Some(after))
            }
        }
    }
}

fn case_group_key(name: &str, order: SortOrder) -> u8 {
    let Some(first) = name.chars().next() else {
        return 0;
    };

    match order {
        SortOrder::Upper => u8::from(first.is_lowercase()),
        SortOrder::Lower => u8::from(first.is_uppercase()),
        SortOrder::Default | SortOrder::Unicode => 0,
    }
}

fn compare_strings(left: &str, right: &str, order: SortOrder) -> Ordering {
    match order {
        SortOrder::Unicode => left.cmp(right),
        SortOrder::Default | SortOrder::Upper | SortOrder::Lower => natural_sort(left, right),
    }
}

fn compare_strings_no_tiebreak(left: &str, right: &str, order: SortOrder) -> Ordering {
    match order {
        SortOrder::Unicode => left.cmp(right),
        SortOrder::Default | SortOrder::Upper | SortOrder::Lower => {
            natural_sort_no_tiebreak(left, right)
        }
    }
}

pub fn compare_rel_paths(
    (left_path, left_is_file): (&RelPath, bool),
    (right_path, right_is_file): (&RelPath, bool),
) -> Ordering {
    compare_rel_paths_by(
        (left_path, left_is_file),
        (right_path, right_is_file),
        SortMode::DirectoriesFirst,
        SortOrder::Default,
    )
}

pub fn compare_rel_paths_by(
    (left_path, left_is_file): (&RelPath, bool),
    (right_path, right_is_file): (&RelPath, bool),
    mode: SortMode,
    order: SortOrder,
) -> Ordering {
    let needs_final_tiebreak = mode != SortMode::DirectoriesFirst
        && !(std::ptr::eq(left_path, right_path) || left_path == right_path);

    let mut left_components = left_path.components();
    let mut right_components = right_path.components();

    loop {
        match (left_components.next(), right_components.next()) {
            (Some(left_component), Some(right_component)) => {
                let left_leaf_file = left_is_file && left_components.rest().is_empty();
                let right_leaf_file = right_is_file && right_components.rest().is_empty();

                let file_dir_ordering = match mode {
                    SortMode::DirectoriesFirst => left_leaf_file.cmp(&right_leaf_file),
                    SortMode::FilesFirst => right_leaf_file.cmp(&left_leaf_file),
                    SortMode::Mixed => Ordering::Equal,
                };

                if !file_dir_ordering.is_eq() {
                    return file_dir_ordering;
                }

                let (left_stem, left_extension) = if left_leaf_file {
                    stem_and_extension(left_component)
                } else {
                    (None, None)
                };
                let (right_stem, right_extension) = if right_leaf_file {
                    stem_and_extension(right_component)
                } else {
                    (None, None)
                };
                let left_key = if left_leaf_file {
                    left_stem
                } else {
                    Some(left_component)
                };
                let right_key = if right_leaf_file {
                    right_stem
                } else {
                    Some(right_component)
                };

                let ordering = match (left_key, right_key) {
                    (Some(left), Some(right)) => {
                        let name_cmp = case_group_key(left, order)
                            .cmp(&case_group_key(right, order))
                            .then_with(|| match mode {
                                SortMode::DirectoriesFirst => compare_strings(left, right, order),
                                SortMode::Mixed | SortMode::FilesFirst => {
                                    compare_strings_no_tiebreak(left, right, order)
                                }
                            });

                        let name_cmp = if mode == SortMode::Mixed {
                            name_cmp.then_with(|| match (left_leaf_file, right_leaf_file) {
                                (true, false) if left.eq_ignore_ascii_case(right) => {
                                    Ordering::Greater
                                }
                                (false, true) if left.eq_ignore_ascii_case(right) => Ordering::Less,
                                _ => Ordering::Equal,
                            })
                        } else {
                            name_cmp
                        };

                        name_cmp.then_with(|| {
                            if left_leaf_file && right_leaf_file {
                                match order {
                                    SortOrder::Unicode => left_extension
                                        .unwrap_or_default()
                                        .cmp(right_extension.unwrap_or_default()),
                                    SortOrder::Default | SortOrder::Upper | SortOrder::Lower => {
                                        let left_extension_name =
                                            left_extension.unwrap_or_default().to_lowercase();
                                        let right_extension_name =
                                            right_extension.unwrap_or_default().to_lowercase();
                                        left_extension_name.cmp(&right_extension_name)
                                    }
                                }
                            } else {
                                Ordering::Equal
                            }
                        })
                    }
                    (Some(_), None) => Ordering::Greater,
                    (None, Some(_)) => Ordering::Less,
                    (None, None) => Ordering::Equal,
                };

                if !ordering.is_eq() {
                    return ordering;
                }
            }
            (Some(_), None) => return Ordering::Greater,
            (None, Some(_)) => return Ordering::Less,
            (None, None) => {
                if needs_final_tiebreak {
                    return compare_strings(
                        left_path.as_unix_str(),
                        right_path.as_unix_str(),
                        order,
                    );
                }
                return Ordering::Equal;
            }
        }
    }
}

impl<T: AsRef<Path>> PathExt for T {
    fn try_from_bytes<'a>(bytes: &'a [u8]) -> anyhow::Result<Self>
    where
        Self: From<&'a Path>,
    {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            Ok(Self::from(Path::new(OsStr::from_bytes(bytes))))
        }

        #[cfg(target_os = "windows")]
        {
            WTF8::validate(bytes)
                .then(|| {
                    Self::from(Path::new(
                        // Safety: `WTF8::validate(bytes)` above guarantees that bytes are valid WTF-8
                        // for `OsStr::from_encoded_bytes_unchecked` on Windows.
                        unsafe { OsStr::from_encoded_bytes_unchecked(bytes) },
                    ))
                })
                .with_context(|| format!("Invalid WTF-8 sequence: {bytes:?}"))
        }
    }
}

/// In memory, this is identical to `Path`. On non-Windows conversions to this
/// type are no-ops. On Windows, these conversions sanitize UNC paths by
/// removing the `\\?\` prefix.
#[derive(Eq, PartialEq, Hash, Ord, PartialOrd)]
#[repr(transparent)]
pub struct SanitizedPath(Path);

impl SanitizedPath {
    pub fn new<T: AsRef<Path> + ?Sized>(path: &T) -> &Self {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        return Self::unchecked_new(path.as_ref());

        #[cfg(target_os = "windows")]
        return Self::unchecked_new(dunce::simplified(path.as_ref()));
    }

    pub fn unchecked_new<T: AsRef<Path> + ?Sized>(path: &T) -> &Self {
        // Safety: `SanitizedPath` is a transparent wrapper around `Path` and adds no
        // extra invariants, so this shared reference cast is valid.
        unsafe { &*(std::ptr::from_ref::<Path>(path.as_ref()) as *const Self) }
    }

    pub fn from_arc(path: Arc<Path>) -> Arc<Self> {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        // Safety: `SanitizedPath` is a transparent wrapper around `Path` and adds no
        // extra invariants, so this `Arc` cast is valid.
        return unsafe { Arc::from_raw(Arc::into_raw(path) as *const Self) };

        #[cfg(target_os = "windows")]
        {
            let simplified = dunce::simplified(path.as_ref());
            if simplified == path.as_ref() {
                // Safety: `SanitizedPath` is a transparent wrapper around `Path` and adds no
                // extra invariants, so this `Arc` cast is valid.
                unsafe { Arc::from_raw(Arc::into_raw(path) as *const Self) }
            } else {
                Self::unchecked_new(simplified).into()
            }
        }
    }

    pub fn new_arc<T: AsRef<Path> + ?Sized>(path: &T) -> Arc<Self> {
        Self::new(path).into()
    }

    pub fn cast_arc(path: Arc<Self>) -> Arc<Path> {
        // Safety: `SanitizedPath` is a transparent wrapper around `Path` and adds no
        // extra invariants, so this `Arc` cast is valid.
        unsafe { Arc::from_raw(Arc::into_raw(path) as *const Path) }
    }

    pub fn cast_arc_ref(path: &Arc<Self>) -> &Arc<Path> {
        // Safety: `SanitizedPath` is a transparent wrapper around `Path` and adds no
        // extra invariants, so this reference to `Arc` cast is valid.
        unsafe { &*std::ptr::from_ref::<Arc<Self>>(path).cast::<Arc<Path>>() }
    }

    pub fn starts_with(&self, prefix: &Self) -> bool {
        self.0.starts_with(&prefix.0)
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

impl Debug for SanitizedPath {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        Debug::fmt(&self.0, formatter)
    }
}

impl Display for SanitizedPath {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0.display())
    }
}

impl From<&SanitizedPath> for Arc<SanitizedPath> {
    fn from(sanitized_path: &SanitizedPath) -> Self {
        let path: Arc<Path> = sanitized_path.0.into();

        // Safety: `SanitizedPath` is a transparent wrapper around `Path` and adds no
        // extra invariants, so this `Arc` cast is valid.
        unsafe { Arc::from_raw(Arc::into_raw(path) as *const SanitizedPath) }
    }
}

impl AsRef<Path> for SanitizedPath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel_path_entry(path: &'static str, is_file: bool) -> (&'static RelPath, bool) {
        (RelPath::unix(path).unwrap(), is_file)
    }

    fn sorted_rel_paths(
        mut paths: Vec<(&'static RelPath, bool)>,
        mode: SortMode,
        order: SortOrder,
    ) -> Vec<(&'static RelPath, bool)> {
        paths.sort_by(|&left, &right| compare_rel_paths_by(left, right, mode, order));
        paths
    }

    #[test]
    fn test_natural_sort() {
        assert_eq!(natural_sort("a", "b"), Ordering::Less);
        assert_eq!(natural_sort("b", "a"), Ordering::Greater);
        assert_eq!(natural_sort("a", "a"), Ordering::Equal);

        assert_eq!(natural_sort("a", "A"), Ordering::Less);
        assert_eq!(natural_sort("A", "a"), Ordering::Greater);
        assert_eq!(natural_sort("aA", "aa"), Ordering::Greater);
        assert_eq!(natural_sort("aa", "aA"), Ordering::Less);

        assert_eq!(natural_sort("1", "2"), Ordering::Less);
        assert_eq!(natural_sort("2", "10"), Ordering::Less);
        assert_eq!(natural_sort("02", "10"), Ordering::Less);
        assert_eq!(natural_sort("02", "2"), Ordering::Greater);

        assert_eq!(natural_sort("a1", "a2"), Ordering::Less);
        assert_eq!(natural_sort("a2", "a10"), Ordering::Less);
        assert_eq!(natural_sort("a02", "a2"), Ordering::Greater);
        assert_eq!(natural_sort("a1b", "a1c"), Ordering::Less);

        assert_eq!(natural_sort("1a2", "1a10"), Ordering::Less);
        assert_eq!(natural_sort("1a10", "1a2"), Ordering::Greater);
        assert_eq!(natural_sort("2a1", "10a1"), Ordering::Less);

        assert_eq!(natural_sort("a-1", "a-2"), Ordering::Less);
        assert_eq!(natural_sort("a_1", "a_2"), Ordering::Less);
        assert_eq!(natural_sort("a.1", "a.2"), Ordering::Less);

        assert_eq!(natural_sort("文1", "文2"), Ordering::Less);
        assert_eq!(natural_sort("文2", "文10"), Ordering::Less);
        assert_eq!(natural_sort("🔤1", "🔤2"), Ordering::Less);

        assert_eq!(natural_sort("", ""), Ordering::Equal);
        assert_eq!(natural_sort("", "a"), Ordering::Less);
        assert_eq!(natural_sort("a", ""), Ordering::Greater);
        assert_eq!(natural_sort(" ", "  "), Ordering::Less);

        assert_eq!(natural_sort("File-1.txt", "File-2.txt"), Ordering::Less);
        assert_eq!(natural_sort("File-02.txt", "File-2.txt"), Ordering::Greater);
        assert_eq!(natural_sort("File-2.txt", "File-10.txt"), Ordering::Less);
        assert_eq!(natural_sort("File_A1", "File_A2"), Ordering::Less);
        assert_eq!(natural_sort("File_a1", "File_A1"), Ordering::Less);
    }

    #[test]
    fn test_compare_rel_paths_mixed_case_insensitive() {
        let mut paths = vec![
            rel_path_entry("zebra.txt", true),
            rel_path_entry("Apple", false),
            rel_path_entry("banana.rs", true),
            rel_path_entry("Carrot", false),
            rel_path_entry("aardvark.txt", true),
        ];

        paths.sort_by(|&left, &right| {
            compare_rel_paths_by(left, right, SortMode::Mixed, SortOrder::Default)
        });

        assert_eq!(
            paths,
            vec![
                rel_path_entry("aardvark.txt", true),
                rel_path_entry("Apple", false),
                rel_path_entry("banana.rs", true),
                rel_path_entry("Carrot", false),
                rel_path_entry("zebra.txt", true),
            ]
        );
    }

    #[test]
    fn test_compare_rel_paths_files_first_basic() {
        let mut paths = vec![
            rel_path_entry("zebra.txt", true),
            rel_path_entry("Apple", false),
            rel_path_entry("banana.rs", true),
            rel_path_entry("Carrot", false),
            rel_path_entry("aardvark.txt", true),
        ];

        paths.sort_by(|&left, &right| {
            compare_rel_paths_by(left, right, SortMode::FilesFirst, SortOrder::Default)
        });

        assert_eq!(
            paths,
            vec![
                rel_path_entry("aardvark.txt", true),
                rel_path_entry("banana.rs", true),
                rel_path_entry("zebra.txt", true),
                rel_path_entry("Apple", false),
                rel_path_entry("Carrot", false),
            ]
        );
    }

    #[test]
    fn test_compare_rel_paths_files_first_case_insensitive() {
        let mut paths = vec![
            rel_path_entry("Zebra.txt", true),
            rel_path_entry("apple", false),
            rel_path_entry("Banana.rs", true),
            rel_path_entry("carrot", false),
            rel_path_entry("Aardvark.txt", true),
        ];

        paths.sort_by(|&left, &right| {
            compare_rel_paths_by(left, right, SortMode::FilesFirst, SortOrder::Default)
        });

        assert_eq!(
            paths,
            vec![
                rel_path_entry("Aardvark.txt", true),
                rel_path_entry("Banana.rs", true),
                rel_path_entry("Zebra.txt", true),
                rel_path_entry("apple", false),
                rel_path_entry("carrot", false),
            ]
        );
    }

    #[test]
    fn test_compare_rel_paths_files_first_numeric() {
        let mut paths = vec![
            rel_path_entry("file10.txt", true),
            rel_path_entry("dir2", false),
            rel_path_entry("file2.txt", true),
            rel_path_entry("dir10", false),
            rel_path_entry("file1.txt", true),
        ];

        paths.sort_by(|&left, &right| {
            compare_rel_paths_by(left, right, SortMode::FilesFirst, SortOrder::Default)
        });

        assert_eq!(
            paths,
            vec![
                rel_path_entry("file1.txt", true),
                rel_path_entry("file2.txt", true),
                rel_path_entry("file10.txt", true),
                rel_path_entry("dir2", false),
                rel_path_entry("dir10", false),
            ]
        );
    }

    #[test]
    fn test_compare_rel_paths_mixed_case() {
        let mut paths = vec![
            rel_path_entry("README.md", true),
            rel_path_entry("readme.txt", true),
            rel_path_entry("ReadMe.rs", true),
        ];

        paths.sort_by(|&left, &right| {
            compare_rel_paths_by(left, right, SortMode::Mixed, SortOrder::Default)
        });

        assert_eq!(
            paths,
            vec![
                rel_path_entry("README.md", true),
                rel_path_entry("ReadMe.rs", true),
                rel_path_entry("readme.txt", true),
            ]
        );
    }

    #[test]
    fn test_compare_rel_paths_mixed_files_and_dirs() {
        let mut paths = vec![
            rel_path_entry("file2.txt", true),
            rel_path_entry("Dir1", false),
            rel_path_entry("file1.txt", true),
            rel_path_entry("dir2", false),
        ];

        paths.sort_by(|&left, &right| {
            compare_rel_paths_by(left, right, SortMode::Mixed, SortOrder::Default)
        });

        assert_eq!(
            paths,
            vec![
                rel_path_entry("Dir1", false),
                rel_path_entry("dir2", false),
                rel_path_entry("file1.txt", true),
                rel_path_entry("file2.txt", true),
            ]
        );
    }

    #[test]
    fn test_compare_rel_paths_mixed_same_name_different_case_file_and_dir() {
        let mut paths = vec![
            rel_path_entry("Hello.txt", true),
            rel_path_entry("hello", false),
        ];

        paths.sort_by(|&left, &right| {
            compare_rel_paths_by(left, right, SortMode::Mixed, SortOrder::Default)
        });

        assert_eq!(
            paths,
            vec![
                rel_path_entry("hello", false),
                rel_path_entry("Hello.txt", true),
            ]
        );

        let mut paths = vec![
            rel_path_entry("hello", false),
            rel_path_entry("Hello.txt", true),
        ];

        paths.sort_by(|&left, &right| {
            compare_rel_paths_by(left, right, SortMode::Mixed, SortOrder::Default)
        });

        assert_eq!(
            paths,
            vec![
                rel_path_entry("hello", false),
                rel_path_entry("Hello.txt", true),
            ]
        );
    }

    #[test]
    fn test_compare_rel_paths_mixed_with_nested_paths() {
        let mut paths = vec![
            rel_path_entry("src/main.rs", true),
            rel_path_entry("Cargo.toml", true),
            rel_path_entry("src", false),
            rel_path_entry("target", false),
        ];

        paths.sort_by(|&left, &right| {
            compare_rel_paths_by(left, right, SortMode::Mixed, SortOrder::Default)
        });

        assert_eq!(
            paths,
            vec![
                rel_path_entry("Cargo.toml", true),
                rel_path_entry("src", false),
                rel_path_entry("src/main.rs", true),
                rel_path_entry("target", false),
            ]
        );
    }

    #[test]
    fn test_compare_rel_paths_files_first_with_nested() {
        let mut paths = vec![
            rel_path_entry("src/lib.rs", true),
            rel_path_entry("README.md", true),
            rel_path_entry("src", false),
            rel_path_entry("tests", false),
        ];

        paths.sort_by(|&left, &right| {
            compare_rel_paths_by(left, right, SortMode::FilesFirst, SortOrder::Default)
        });

        assert_eq!(
            paths,
            vec![
                rel_path_entry("README.md", true),
                rel_path_entry("src", false),
                rel_path_entry("src/lib.rs", true),
                rel_path_entry("tests", false),
            ]
        );
    }

    #[test]
    fn test_compare_rel_paths_mixed_dotfiles() {
        let mut paths = vec![
            rel_path_entry(".gitignore", true),
            rel_path_entry("README.md", true),
            rel_path_entry(".github", false),
            rel_path_entry("src", false),
        ];

        paths.sort_by(|&left, &right| {
            compare_rel_paths_by(left, right, SortMode::Mixed, SortOrder::Default)
        });

        assert_eq!(
            paths,
            vec![
                rel_path_entry(".github", false),
                rel_path_entry(".gitignore", true),
                rel_path_entry("README.md", true),
                rel_path_entry("src", false),
            ]
        );
    }

    #[test]
    fn test_compare_rel_paths_files_first_dotfiles() {
        let mut paths = vec![
            rel_path_entry(".gitignore", true),
            rel_path_entry("README.md", true),
            rel_path_entry(".github", false),
            rel_path_entry("src", false),
        ];

        paths.sort_by(|&left, &right| {
            compare_rel_paths_by(left, right, SortMode::FilesFirst, SortOrder::Default)
        });

        assert_eq!(
            paths,
            vec![
                rel_path_entry(".gitignore", true),
                rel_path_entry("README.md", true),
                rel_path_entry(".github", false),
                rel_path_entry("src", false),
            ]
        );
    }

    #[test]
    fn test_compare_rel_paths_mixed_same_stem_different_extension() {
        let mut paths = vec![
            rel_path_entry("file.rs", true),
            rel_path_entry("file.md", true),
            rel_path_entry("file.txt", true),
        ];

        paths.sort_by(|&left, &right| {
            compare_rel_paths_by(left, right, SortMode::Mixed, SortOrder::Default)
        });

        assert_eq!(
            paths,
            vec![
                rel_path_entry("file.md", true),
                rel_path_entry("file.rs", true),
                rel_path_entry("file.txt", true),
            ]
        );
    }

    #[test]
    fn test_compare_rel_paths_files_first_same_stem() {
        let mut paths = vec![
            rel_path_entry("main.rs", true),
            rel_path_entry("main.c", true),
            rel_path_entry("main", false),
        ];

        paths.sort_by(|&left, &right| {
            compare_rel_paths_by(left, right, SortMode::FilesFirst, SortOrder::Default)
        });

        assert_eq!(
            paths,
            vec![
                rel_path_entry("main.c", true),
                rel_path_entry("main.rs", true),
                rel_path_entry("main", false),
            ]
        );
    }

    #[test]
    fn test_compare_rel_paths_mixed_deep_nesting() {
        let mut paths = vec![
            rel_path_entry("a/b/c.txt", true),
            rel_path_entry("A/B.txt", true),
            rel_path_entry("a.txt", true),
            rel_path_entry("A.txt", true),
        ];

        paths.sort_by(|&left, &right| {
            compare_rel_paths_by(left, right, SortMode::Mixed, SortOrder::Default)
        });

        assert_eq!(
            paths,
            vec![
                rel_path_entry("a/b/c.txt", true),
                rel_path_entry("A/B.txt", true),
                rel_path_entry("a.txt", true),
                rel_path_entry("A.txt", true),
            ]
        );
    }

    #[test]
    fn test_compare_rel_paths_upper() {
        let directories_only_paths = vec![
            rel_path_entry("mixedCase", false),
            rel_path_entry("Zebra", false),
            rel_path_entry("banana", false),
            rel_path_entry("ALLCAPS", false),
            rel_path_entry("Apple", false),
            rel_path_entry("dog", false),
            rel_path_entry(".hidden", false),
            rel_path_entry("Carrot", false),
        ];

        assert_eq!(
            sorted_rel_paths(
                directories_only_paths,
                SortMode::DirectoriesFirst,
                SortOrder::Upper,
            ),
            vec![
                rel_path_entry(".hidden", false),
                rel_path_entry("ALLCAPS", false),
                rel_path_entry("Apple", false),
                rel_path_entry("Carrot", false),
                rel_path_entry("Zebra", false),
                rel_path_entry("banana", false),
                rel_path_entry("dog", false),
                rel_path_entry("mixedCase", false),
            ]
        );

        let file_and_directory_paths = vec![
            rel_path_entry("banana", false),
            rel_path_entry("Apple.txt", true),
            rel_path_entry("dog.md", true),
            rel_path_entry("ALLCAPS", false),
            rel_path_entry("file1.txt", true),
            rel_path_entry("File2.txt", true),
            rel_path_entry(".hidden", false),
        ];

        assert_eq!(
            sorted_rel_paths(
                file_and_directory_paths.clone(),
                SortMode::DirectoriesFirst,
                SortOrder::Upper,
            ),
            vec![
                rel_path_entry(".hidden", false),
                rel_path_entry("ALLCAPS", false),
                rel_path_entry("banana", false),
                rel_path_entry("Apple.txt", true),
                rel_path_entry("File2.txt", true),
                rel_path_entry("dog.md", true),
                rel_path_entry("file1.txt", true),
            ]
        );
        assert_eq!(
            sorted_rel_paths(
                file_and_directory_paths.clone(),
                SortMode::Mixed,
                SortOrder::Upper,
            ),
            vec![
                rel_path_entry(".hidden", false),
                rel_path_entry("ALLCAPS", false),
                rel_path_entry("Apple.txt", true),
                rel_path_entry("File2.txt", true),
                rel_path_entry("banana", false),
                rel_path_entry("dog.md", true),
                rel_path_entry("file1.txt", true),
            ]
        );
        assert_eq!(
            sorted_rel_paths(
                file_and_directory_paths,
                SortMode::FilesFirst,
                SortOrder::Upper,
            ),
            vec![
                rel_path_entry("Apple.txt", true),
                rel_path_entry("File2.txt", true),
                rel_path_entry("dog.md", true),
                rel_path_entry("file1.txt", true),
                rel_path_entry(".hidden", false),
                rel_path_entry("ALLCAPS", false),
                rel_path_entry("banana", false),
            ]
        );

        let natural_sort_paths = vec![
            rel_path_entry("file10.txt", true),
            rel_path_entry("file1.txt", true),
            rel_path_entry("file20.txt", true),
            rel_path_entry("file2.txt", true),
        ];

        assert_eq!(
            sorted_rel_paths(natural_sort_paths, SortMode::Mixed, SortOrder::Upper),
            vec![
                rel_path_entry("file1.txt", true),
                rel_path_entry("file2.txt", true),
                rel_path_entry("file10.txt", true),
                rel_path_entry("file20.txt", true),
            ]
        );

        let accented_paths = vec![
            rel_path_entry("\u{00C9}something.txt", true),
            rel_path_entry("zebra.txt", true),
            rel_path_entry("Apple.txt", true),
        ];

        assert_eq!(
            sorted_rel_paths(accented_paths, SortMode::Mixed, SortOrder::Upper),
            vec![
                rel_path_entry("Apple.txt", true),
                rel_path_entry("\u{00C9}something.txt", true),
                rel_path_entry("zebra.txt", true),
            ]
        );
    }

    #[test]
    fn test_compare_rel_paths_lower() {
        let directories_only_paths = vec![
            rel_path_entry("mixedCase", false),
            rel_path_entry("Zebra", false),
            rel_path_entry("banana", false),
            rel_path_entry("ALLCAPS", false),
            rel_path_entry("Apple", false),
            rel_path_entry("dog", false),
            rel_path_entry(".hidden", false),
            rel_path_entry("Carrot", false),
        ];

        assert_eq!(
            sorted_rel_paths(
                directories_only_paths,
                SortMode::DirectoriesFirst,
                SortOrder::Lower,
            ),
            vec![
                rel_path_entry(".hidden", false),
                rel_path_entry("banana", false),
                rel_path_entry("dog", false),
                rel_path_entry("mixedCase", false),
                rel_path_entry("ALLCAPS", false),
                rel_path_entry("Apple", false),
                rel_path_entry("Carrot", false),
                rel_path_entry("Zebra", false),
            ]
        );

        let file_and_directory_paths = vec![
            rel_path_entry("banana", false),
            rel_path_entry("Apple.txt", true),
            rel_path_entry("dog.md", true),
            rel_path_entry("ALLCAPS", false),
            rel_path_entry("file1.txt", true),
            rel_path_entry("File2.txt", true),
            rel_path_entry(".hidden", false),
        ];

        assert_eq!(
            sorted_rel_paths(
                file_and_directory_paths.clone(),
                SortMode::DirectoriesFirst,
                SortOrder::Lower,
            ),
            vec![
                rel_path_entry(".hidden", false),
                rel_path_entry("banana", false),
                rel_path_entry("ALLCAPS", false),
                rel_path_entry("dog.md", true),
                rel_path_entry("file1.txt", true),
                rel_path_entry("Apple.txt", true),
                rel_path_entry("File2.txt", true),
            ]
        );
        assert_eq!(
            sorted_rel_paths(
                file_and_directory_paths.clone(),
                SortMode::Mixed,
                SortOrder::Lower,
            ),
            vec![
                rel_path_entry(".hidden", false),
                rel_path_entry("banana", false),
                rel_path_entry("dog.md", true),
                rel_path_entry("file1.txt", true),
                rel_path_entry("ALLCAPS", false),
                rel_path_entry("Apple.txt", true),
                rel_path_entry("File2.txt", true),
            ]
        );
        assert_eq!(
            sorted_rel_paths(
                file_and_directory_paths,
                SortMode::FilesFirst,
                SortOrder::Lower,
            ),
            vec![
                rel_path_entry("dog.md", true),
                rel_path_entry("file1.txt", true),
                rel_path_entry("Apple.txt", true),
                rel_path_entry("File2.txt", true),
                rel_path_entry(".hidden", false),
                rel_path_entry("banana", false),
                rel_path_entry("ALLCAPS", false),
            ]
        );
    }

    #[test]
    fn test_compare_rel_paths_unicode() {
        let directories_only_paths = vec![
            rel_path_entry("mixedCase", false),
            rel_path_entry("Zebra", false),
            rel_path_entry("banana", false),
            rel_path_entry("ALLCAPS", false),
            rel_path_entry("Apple", false),
            rel_path_entry("dog", false),
            rel_path_entry(".hidden", false),
            rel_path_entry("Carrot", false),
        ];

        assert_eq!(
            sorted_rel_paths(
                directories_only_paths,
                SortMode::DirectoriesFirst,
                SortOrder::Unicode,
            ),
            vec![
                rel_path_entry(".hidden", false),
                rel_path_entry("ALLCAPS", false),
                rel_path_entry("Apple", false),
                rel_path_entry("Carrot", false),
                rel_path_entry("Zebra", false),
                rel_path_entry("banana", false),
                rel_path_entry("dog", false),
                rel_path_entry("mixedCase", false),
            ]
        );

        let file_and_directory_paths = vec![
            rel_path_entry("banana", false),
            rel_path_entry("Apple.txt", true),
            rel_path_entry("dog.md", true),
            rel_path_entry("ALLCAPS", false),
            rel_path_entry("file1.txt", true),
            rel_path_entry("File2.txt", true),
            rel_path_entry(".hidden", false),
        ];

        assert_eq!(
            sorted_rel_paths(
                file_and_directory_paths.clone(),
                SortMode::DirectoriesFirst,
                SortOrder::Unicode,
            ),
            vec![
                rel_path_entry(".hidden", false),
                rel_path_entry("ALLCAPS", false),
                rel_path_entry("banana", false),
                rel_path_entry("Apple.txt", true),
                rel_path_entry("File2.txt", true),
                rel_path_entry("dog.md", true),
                rel_path_entry("file1.txt", true),
            ]
        );
        assert_eq!(
            sorted_rel_paths(
                file_and_directory_paths.clone(),
                SortMode::Mixed,
                SortOrder::Unicode,
            ),
            vec![
                rel_path_entry(".hidden", false),
                rel_path_entry("ALLCAPS", false),
                rel_path_entry("Apple.txt", true),
                rel_path_entry("File2.txt", true),
                rel_path_entry("banana", false),
                rel_path_entry("dog.md", true),
                rel_path_entry("file1.txt", true),
            ]
        );
        assert_eq!(
            sorted_rel_paths(
                file_and_directory_paths,
                SortMode::FilesFirst,
                SortOrder::Unicode,
            ),
            vec![
                rel_path_entry("Apple.txt", true),
                rel_path_entry("File2.txt", true),
                rel_path_entry("dog.md", true),
                rel_path_entry("file1.txt", true),
                rel_path_entry(".hidden", false),
                rel_path_entry("ALLCAPS", false),
                rel_path_entry("banana", false),
            ]
        );

        let numeric_paths = vec![
            rel_path_entry("file10.txt", true),
            rel_path_entry("file1.txt", true),
            rel_path_entry("file2.txt", true),
            rel_path_entry("file20.txt", true),
        ];

        assert_eq!(
            sorted_rel_paths(numeric_paths, SortMode::Mixed, SortOrder::Unicode),
            vec![
                rel_path_entry("file1.txt", true),
                rel_path_entry("file10.txt", true),
                rel_path_entry("file2.txt", true),
                rel_path_entry("file20.txt", true),
            ]
        );

        let accented_paths = vec![
            rel_path_entry("\u{00C9}something.txt", true),
            rel_path_entry("zebra.txt", true),
            rel_path_entry("Apple.txt", true),
        ];

        assert_eq!(
            sorted_rel_paths(accented_paths, SortMode::Mixed, SortOrder::Unicode),
            vec![
                rel_path_entry("Apple.txt", true),
                rel_path_entry("zebra.txt", true),
                rel_path_entry("\u{00C9}something.txt", true),
            ]
        );
    }

    #[test]
    fn test_path_style_strip_prefix() {
        let expected = [
            (
                PathStyle::Posix,
                "/a/b/c",
                "/a/b",
                Some(RelPath::unix("c").unwrap().into_arc()),
            ),
            (
                PathStyle::Posix,
                "/a/b/c",
                "/a/b/",
                Some(RelPath::unix("c").unwrap().into_arc()),
            ),
            (
                PathStyle::Posix,
                "/a/b/c",
                "/",
                Some(RelPath::unix("a/b/c").unwrap().into_arc()),
            ),
            (PathStyle::Posix, "/a/b/c", "", None),
            (PathStyle::Posix, "/a/b//c", "/a/b/", None),
            (PathStyle::Posix, "/a/bc", "/a/b", None),
            (
                PathStyle::Posix,
                "/a/b/c",
                "/a/b/c",
                Some(RelPath::unix("").unwrap().into_arc()),
            ),
            (
                PathStyle::Windows,
                "C:\\a\\b\\c",
                "C:\\a\\b",
                Some(RelPath::unix("c").unwrap().into_arc()),
            ),
            (
                PathStyle::Windows,
                "C:\\a\\b\\c",
                "C:\\a\\b\\",
                Some(RelPath::unix("c").unwrap().into_arc()),
            ),
            (
                PathStyle::Windows,
                "C:\\a\\b\\c",
                "C:\\",
                Some(RelPath::unix("a/b/c").unwrap().into_arc()),
            ),
            (PathStyle::Windows, "C:\\a\\b\\c", "", None),
            (PathStyle::Windows, "C:\\a\\b\\\\c", "C:\\a\\b\\", None),
            (PathStyle::Windows, "C:\\a\\bc", "C:\\a\\b", None),
            (
                PathStyle::Windows,
                "C:\\a\\b/c",
                "C:\\a\\b",
                Some(RelPath::unix("c").unwrap().into_arc()),
            ),
            (
                PathStyle::Windows,
                "C:\\a\\b/c",
                "C:\\a\\b\\",
                Some(RelPath::unix("c").unwrap().into_arc()),
            ),
            (
                PathStyle::Windows,
                "C:\\a\\b/c",
                "C:\\a\\b/",
                Some(RelPath::unix("c").unwrap().into_arc()),
            ),
        ];
        let actual = expected.clone().map(|(style, child, parent, _)| {
            (
                style,
                child,
                parent,
                style
                    .strip_prefix(child.as_ref(), parent.as_ref())
                    .map(|relative_path| relative_path.into_arc()),
            )
        });
        assert_eq!(actual, expected);
    }
}
