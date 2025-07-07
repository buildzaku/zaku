use std::fs::{self, File};
use std::io::{Error, ErrorKind, Write};
use std::path::{Path, PathBuf};
use toml;

use crate::models::toml::{ReqToml, ReqTomlConfig, ReqTomlMeta};

pub fn create_reqtoml(abspath: &Path, display_name: &str) -> Result<(), Error> {
    let mut reqtoml_file = File::create_new(&abspath.with_extension("toml")).map_err(|err| {
        Error::new(
            ErrorKind::Other,
            format!("Failed to create request file: {}", err),
        )
    })?;

    let req_toml = ReqToml {
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

    let toml_str = toml::to_string_pretty(&req_toml).map_err(|err| {
        Error::new(
            ErrorKind::Other,
            format!("Failed to serialize request file: {}", err),
        )
    })?;

    reqtoml_file.write_all(toml_str.as_bytes()).map_err(|err| {
        Error::new(
            ErrorKind::Other,
            format!("Failed to write to request file: {}", err),
        )
    })?;

    return Ok(());
}

pub fn parse_reqtoml(abspath: &PathBuf) -> Result<ReqToml, Error> {
    let toml_str = match fs::read_to_string(abspath) {
        Ok(toml_str) => toml_str,
        Err(err) => {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("Failed to load {}: {}", abspath.display(), err),
            ));
        }
    };

    let req_toml = match toml::from_str(&toml_str) {
        Ok(req_toml) => req_toml,
        Err(err) => {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse {}: {}", abspath.display(), err),
            ));
        }
    };

    return Ok(req_toml);
}

pub fn persist_reqtoml(req_abspath: &Path, req_toml: &ReqToml) -> Result<(), Error> {
    if !req_abspath.exists() {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("Request file does not exist: {:?}", req_abspath),
        ));
    }

    let toml_str = toml::to_string_pretty(&req_toml).unwrap();
    fs::write(req_abspath, toml_str).unwrap();

    return Ok(());
}
