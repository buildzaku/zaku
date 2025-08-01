use derive_more::From;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::{fmt, io};

#[derive(Debug, From)]
pub enum Error {
    FileNotFound(String),
    FileReadError(String),
    FileConflict(String),
    LockError(String),
    InvalidPath(String),
    InvalidName(String),
    SanitizationError(String),

    #[from]
    Io(io::Error),

    #[from]
    SerdeJson(serde_json::Error),

    #[from]
    Tauri(tauri::Error),

    #[from]
    Url(url::ParseError),

    #[from]
    Reqwest(reqwest::Error),

    #[from]
    TomlDe(toml::de::Error),

    #[from]
    TomlSer(toml::ser::Error),

    #[from]
    Time(time::error::Error),

    #[from]
    RodioStream(rodio::StreamError),

    #[from]
    RodioPlay(rodio::PlayError),

    #[from]
    CookieStore(cookie_store::Error),

    #[from]
    ShortcutPlugin(tauri_plugin_global_shortcut::Error),

    #[from]
    StripPrefix(std::path::StripPrefixError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Error {}

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub enum ErrorKind {
    SpaceNotFoundError,
    FileNotFoundError,
    FileReadError,
    FileWriteError,
    LockError,
    DialogOpenError,
    NotificationDispatchError,
    NotificationPermissionError,
    InvalidUrlError,
    InvalidPathError,
    InvalidNameError,
    SanitizationError,
    NetworkError,
    ParseError,
    CookieError,
    InternalError,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct CmdErr {
    pub kind: ErrorKind,
    pub message: String,
    pub details: Option<String>,
}

pub type CmdResult<T> = core::result::Result<T, CmdErr>;
