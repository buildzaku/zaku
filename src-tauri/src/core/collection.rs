use std::collections::HashMap;
use std::fs;
use std::io::{Error, ErrorKind};
use std::path::Path;
use toml;

pub fn display_name(absolute_space_root: &Path) -> Result<HashMap<String, String>, Error> {
    let content = match fs::read_to_string(
        &absolute_space_root.join(".zaku/collections/display_name.toml"),
    ) {
        Ok(content) => content,
        Err(err) => {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("Failed to load {}: {}", absolute_space_root.display(), err),
            ));
        }
    };

    match toml::from_str(&content) {
        Ok(parsed_content) => {
            return Ok(parsed_content);
        }
        Err(err) => {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse TOML: {}", err),
            ));
        }
    };
}
