use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Error, ErrorKind, Write};
use std::path::Path;
use toml;

pub fn display_name_by_relative_path(
    active_space_absolute_path: &Path,
) -> Result<HashMap<String, String>, Error> {
    let display_names_file_absolute_path =
        active_space_absolute_path.join(".zaku/collections/display_name.toml");

    let content = match fs::read_to_string(&display_names_file_absolute_path) {
        Ok(content) => content,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            let initialized_hash_map: HashMap<String, String> = HashMap::new();

            if let Some(parent) = display_names_file_absolute_path.parent() {
                fs::create_dir_all(parent)
                    .expect("Failed to create display name's parent directories");
            }

            let mut display_name_file =
                File::create(&display_names_file_absolute_path.with_extension("toml"))
                    .expect("Failed to create `display_name.toml`");
            display_name_file
                .write_all(
                    toml::to_string_pretty(&initialized_hash_map)
                        .expect("Failed to serialize empty TOML")
                        .as_bytes(),
                )
                .expect("Failed to write to config file");

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

    let mut collection_name_by_relative_path =
        display_name_by_relative_path(&active_space_absolute_path)
            .expect("Failed to get display names by relative path");

    collection_name_by_relative_path.insert(
        collection_relative_path.to_string(),
        collection_display_name.to_string(),
    );

    let toml_content = toml::to_string_pretty(&collection_name_by_relative_path)
        .expect("Failed to serialize TOML");

    fs::write(&display_names_file_absolute_path, toml_content)
        .expect("Failed to write display names to file");

    return Ok(());
}
