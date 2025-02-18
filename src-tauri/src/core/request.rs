use std::fs::{self, File};
use std::io::{Error, ErrorKind, Write};
use std::path::{Path, PathBuf};
use toml;

use crate::models::request::{Request, RequestConfig, RequestFile, RequestFileMeta};

pub fn create_request_file(absolute_path: &Path, display_name: &str) -> Result<(), Error> {
    let mut request_file =
        File::create_new(&absolute_path.with_extension("toml")).map_err(|err| {
            Error::new(
                ErrorKind::Other,
                format!("Failed to create request file: {}", err),
            )
        })?;

    let request = RequestFile {
        meta: RequestFileMeta {
            name: display_name.to_string(),
        },
        config: RequestConfig {
            method: "GET".to_string(),
            url: None,
        },
    };

    let toml_string = toml::to_string_pretty(&request).map_err(|err| {
        Error::new(
            ErrorKind::Other,
            format!("Failed to serialize request file: {}", err),
        )
    })?;

    request_file
        .write_all(toml_string.as_bytes())
        .map_err(|err| {
            Error::new(
                ErrorKind::Other,
                format!("Failed to write to request file: {}", err),
            )
        })?;

    return Ok(());
}

pub fn parse_request_file(absolute_path: &PathBuf) -> Result<RequestFile, Error> {
    let content = match fs::read_to_string(absolute_path) {
        Ok(content) => content,
        Err(err) => {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("Failed to load {}: {}", absolute_path.display(), err),
            ));
        }
    };

    let parsed_request = match toml::from_str(&content) {
        Ok(parsed_content) => parsed_content,
        Err(err) => {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse {}: {}", absolute_path.display(), err),
            ));
        }
    };

    return Ok(parsed_request);
}

pub fn save_to_request_file(absolute_request_path: &Path, request: &Request) -> Result<(), Error> {
    if !absolute_request_path.exists() {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("Request file does not exist: {:?}", absolute_request_path),
        ));
    }

    let toml_string = toml::to_string_pretty(request).map_err(|err| {
        Error::new(
            ErrorKind::Other,
            format!("Failed to serialize request file: {}", err),
        )
    })?;

    fs::write(absolute_request_path, toml_string).map_err(|err| {
        Error::new(
            ErrorKind::Other,
            format!("Failed to write to request file: {}", err),
        )
    })?;

    return Ok(());
}
