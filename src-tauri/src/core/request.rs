use std::fs;
use std::io::{Error, ErrorKind};
use std::path::PathBuf;
use toml;

use crate::models::request::RequestFile;

pub fn parse_request_file(path: PathBuf) -> Result<RequestFile, Error> {
    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(err) => {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("Failed to load {}: {}", path.display(), err),
            ));
        }
    };

    let parsed_request = match toml::from_str(&content) {
        Ok(parsed_content) => parsed_content,
        Err(err) => {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse {}: {}", path.display(), err),
            ));
        }
    };

    return Ok(parsed_request);
}
