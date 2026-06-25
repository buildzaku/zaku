use gpui::SharedString;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    borrow::Borrow,
    fmt,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_LANGUAGE_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LanguageId(usize);

impl LanguageId {
    pub fn new() -> Self {
        Self(NEXT_LANGUAGE_ID.fetch_add(1, Ordering::SeqCst))
    }
}

impl Default for LanguageId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct LanguageName(pub SharedString);

impl LanguageName {
    pub fn new(string: &str) -> Self {
        Self(SharedString::new(string))
    }

    pub fn new_static(string: &'static str) -> Self {
        Self(SharedString::new_static(string))
    }
}

impl From<SharedString> for LanguageName {
    fn from(value: SharedString) -> Self {
        LanguageName(value)
    }
}

impl From<LanguageName> for SharedString {
    fn from(value: LanguageName) -> Self {
        value.0
    }
}

impl AsRef<str> for LanguageName {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl Borrow<str> for LanguageName {
    fn borrow(&self) -> &str {
        self.0.as_ref()
    }
}

impl PartialEq<str> for LanguageName {
    fn eq(&self, other: &str) -> bool {
        self.0.as_ref() == other
    }
}

impl PartialEq<&str> for LanguageName {
    fn eq(&self, other: &&str) -> bool {
        self.0.as_ref() == *other
    }
}

impl fmt::Display for LanguageName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl From<&'static str> for LanguageName {
    fn from(string: &'static str) -> Self {
        Self(SharedString::new_static(string))
    }
}

impl From<LanguageName> for String {
    fn from(value: LanguageName) -> Self {
        let value: &str = &value.0;
        Self::from(value)
    }
}
