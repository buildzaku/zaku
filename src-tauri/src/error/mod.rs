use derive_more::From;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::{fmt, io};

#[derive(Debug, From)]
pub enum Error {
    FileNotFound(String),
    FileReadError(String),
    LockError(String),
    InvalidPath(String),
    InvalidName(String),

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
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Error {}

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(tag = "type")]
pub enum CmdErr {
    Err { message: String },
    Http { message: String, code: Option<u16> },
}

pub type CmdResult<T> = core::result::Result<T, CmdErr>;
