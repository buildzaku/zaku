#[cfg(target_os = "windows")]
use anyhow::Context;

#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;

use std::{
    borrow::Cow,
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

    #[cfg(not(target_os = "windows"))]
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

impl<T: AsRef<Path>> PathExt for T {
    fn try_from_bytes<'a>(bytes: &'a [u8]) -> anyhow::Result<Self>
    where
        Self: From<&'a Path>,
    {
        #[cfg(target_family = "wasm")]
        {
            std::str::from_utf8(bytes)
                .map(Path::new)
                .map(Into::into)
                .map_err(Into::into)
        }
        #[cfg(unix)]
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
        #[cfg(not(target_os = "windows"))]
        return Self::unchecked_new(path.as_ref());

        #[cfg(target_os = "windows")]
        return Self::unchecked_new(dunce::simplified(path.as_ref()));
    }

    pub fn unchecked_new<T: AsRef<Path> + ?Sized>(path: &T) -> &Self {
        // Safety: `SanitizedPath` is a transparent wrapper around `Path` and adds no
        // extra invariants, so this shared reference cast is valid.
        unsafe { std::mem::transmute::<&Path, &Self>(path.as_ref()) }
    }

    pub fn from_arc(path: Arc<Path>) -> Arc<Self> {
        #[cfg(not(target_os = "windows"))]
        // Safety: `SanitizedPath` is a transparent wrapper around `Path` and adds no
        // extra invariants, so this `Arc` cast is valid.
        return unsafe { std::mem::transmute::<Arc<Path>, Arc<Self>>(path) };

        #[cfg(target_os = "windows")]
        {
            let simplified = dunce::simplified(path.as_ref());
            if simplified == path.as_ref() {
                // Safety: `SanitizedPath` is a transparent wrapper around `Path` and adds no
                // extra invariants, so this `Arc` cast is valid.
                unsafe { std::mem::transmute::<Arc<Path>, Arc<Self>>(path) }
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
        unsafe { std::mem::transmute::<Arc<Self>, Arc<Path>>(path) }
    }

    pub fn cast_arc_ref(path: &Arc<Self>) -> &Arc<Path> {
        // Safety: `SanitizedPath` is a transparent wrapper around `Path` and adds no
        // extra invariants, so this reference to `Arc` cast is valid.
        unsafe { std::mem::transmute::<&Arc<Self>, &Arc<Path>>(path) }
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
        unsafe { std::mem::transmute(path) }
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
