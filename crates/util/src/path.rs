#[cfg(windows)]
use anyhow::Context;

#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;

use std::{ffi::OsStr, path::Path};

#[cfg(windows)]
use tendril::fmt::{Format, WTF8};

pub trait PathExt {
    fn try_from_bytes<'a>(bytes: &'a [u8]) -> anyhow::Result<Self>
    where
        Self: From<&'a Path>;
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
        #[cfg(windows)]
        {
            WTF8::validate(bytes)
                .then(|| {
                    Self::from(Path::new(
                        // Safety: WTF8::validate(bytes) above guarantees that bytes are valid WTF-8
                        // for OsStr::from_encoded_bytes_unchecked on Windows.
                        unsafe { OsStr::from_encoded_bytes_unchecked(bytes) },
                    ))
                })
                .with_context(|| format!("Invalid WTF-8 sequence: {bytes:?}"))
        }
    }
}
