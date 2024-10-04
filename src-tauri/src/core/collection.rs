use std::collections::HashMap;
use std::fs;
use std::io::{Error, ErrorKind};
use std::path::Path;
use toml;

pub fn display_name(active_space_absolute_path: &Path) -> Result<HashMap<String, String>, Error> {
    let display_names_file_absolute_path =
        active_space_absolute_path.join(".zaku/collections/display_name.toml");

    let content = match fs::read_to_string(&display_names_file_absolute_path) {
        Ok(content) => content,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            let initialized_hash_map: HashMap<String, String> = HashMap::new();
            let toml_content = toml::to_string(&initialized_hash_map).map_err(|err| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Failed to serialize empty TOML: {}", err),
                )
            })?;

            fs::write(&display_names_file_absolute_path, toml_content).map_err(|err| {
                Error::new(
                    ErrorKind::Other,
                    format!(
                        "Failed to create {}: {}",
                        display_names_file_absolute_path.display(),
                        err
                    ),
                )
            })?;

            return Ok(initialized_hash_map);
        }
        Err(err) => {
            return Err(Error::new(
                ErrorKind::Other,
                format!(
                    "Failed to load {}: {}",
                    display_names_file_absolute_path.display(),
                    err
                ),
            ));
        }
    };

    let parsed_content: HashMap<String, String> = match toml::from_str(&content) {
        Ok(parsed_content) => parsed_content,
        Err(err) => {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Failed to parse TOML: {}", err),
            ));
        }
    };

    return Ok(parsed_content);
}

pub fn save_display_name(
    active_space_absolute_path: &Path,
    collection_relative_path: &str,
    collection_display_name: &str,
) -> Result<(), Error> {
    let display_names_file_absolute_path =
        active_space_absolute_path.join(".zaku/collections/display_name.toml");

    let mut display_names = match display_name(&active_space_absolute_path) {
        Ok(names) => names,
        Err(err) => {
            return Err(Error::new(
                ErrorKind::Other,
                format!("Error reading display names: {}", err),
            ))
        }
    };

    display_names.insert(
        collection_relative_path.to_string(),
        collection_display_name.to_string(),
    );

    let toml_content = toml::to_string(&display_names).map_err(|err| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Failed to serialize TOML: {}", err),
        )
    })?;

    fs::write(&display_names_file_absolute_path, toml_content).map_err(|err| {
        Error::new(
            ErrorKind::Other,
            format!(
                "Failed to write to {}: {}",
                display_names_file_absolute_path.display(),
                err
            ),
        )
    })?;

    return Ok(());
}
