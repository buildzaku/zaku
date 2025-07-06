use std::fs::{self, File};
use std::io::{Error, ErrorKind, Write};
use std::path::{Path, PathBuf};
use toml;

use crate::models::toml::{ReqToml, ReqTomlConfig, ReqTomlMeta};

pub fn create_req_toml(abspath: &Path, display_name: &str) -> Result<(), Error> {
    let mut req_toml = File::create_new(&abspath.with_extension("toml")).map_err(|err| {
        Error::new(
            ErrorKind::Other,
            format!("Failed to create request file: {}", err),
        )
    })?;

    let request = ReqToml {
        meta: ReqTomlMeta {
            name: display_name.to_string(),
        },
        config: ReqTomlConfig {
            method: "GET".to_string(),
            url: None,
            headers: None,
            parameters: None,
            content_type: None,
            body: None,
        },
    };

    let toml_string = toml::to_string_pretty(&request).map_err(|err| {
        Error::new(
            ErrorKind::Other,
            format!("Failed to serialize request file: {}", err),
        )
    })?;

    req_toml.write_all(toml_string.as_bytes()).map_err(|err| {
        Error::new(
            ErrorKind::Other,
            format!("Failed to write to request file: {}", err),
        )
    })?;

    return Ok(());
}

pub fn parse_req_toml(abspath: &PathBuf) -> Result<ReqToml, Error> {
    let content = match fs::read_to_string(abspath) {
        Ok(content) => content,
        Err(err) => {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("Failed to load {}: {}", abspath.display(), err),
            ));
        }
    };

    let parsed_request = match toml::from_str(&content) {
        Ok(parsed_content) => parsed_content,
        Err(err) => {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse {}: {}", abspath.display(), err),
            ));
        }
    };

    return Ok(parsed_request);
}

pub fn save_to_req_toml(req_abspath: &Path, req_toml: &ReqToml) -> Result<(), Error> {
    if !req_abspath.exists() {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("Request file does not exist: {:?}", req_abspath),
        ));
    }

    let toml_string = toml::to_string_pretty(&req_toml).unwrap();
    fs::write(req_abspath, toml_string).unwrap();

    return Ok(());
}
