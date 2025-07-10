use derive_more::From;
use std::fmt;
use std::io;

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug, From)]
pub enum Error {
    FileNotFound(String),
    FileReadError(String),

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
    Rodio(rodio::StreamError),

    #[from]
    CookieStore(cookie_store::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Error {}
