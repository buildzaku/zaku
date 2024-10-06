use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Error, ErrorKind, Write};
use std::path::Path;
use toml;

use crate::models::collection::CreateCollectionDto;
use crate::utils;

pub fn display_name_by_relative_path(
    space_absolute_path: &Path,
) -> Result<HashMap<String, String>, Error> {
    let display_name_file_absolute_path =
        space_absolute_path.join(".zaku/collections/display_name");

    let content = match fs::read_to_string(&display_name_file_absolute_path.with_extension("toml"))
    {
        Ok(content) => content,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            let initialized_hash_map: HashMap<String, String> = HashMap::new();

            if let Some(parent) = display_name_file_absolute_path.parent() {
                fs::create_dir_all(parent)
                    .expect("Failed to create display name's parent directories");
            }

            let mut display_name_file =
                File::create(&display_name_file_absolute_path.with_extension("toml"))
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
                    display_name_file_absolute_path.display(),
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

pub fn save_display_name_if_not_exists(
    space_absolute_path: &Path,
    collection_relative_path_from_root: &str,
    collection_display_name: &str,
) -> Result<(), Error> {
    let display_name_file_absolute_path =
        space_absolute_path.join(".zaku/collections/display_name");

    let mut collection_name_by_relative_path = display_name_by_relative_path(&space_absolute_path)
        .expect("Failed to get display names by relative path");

    collection_name_by_relative_path
        .entry(collection_relative_path_from_root.to_string())
        .or_insert(collection_display_name.to_string());

    let toml_content = toml::to_string_pretty(&collection_name_by_relative_path)
        .expect("Failed to serialize TOML");

    fs::write(
        &display_name_file_absolute_path.with_extension("toml"),
        toml_content,
    )
    .expect("Failed to write display names to file");

    return Ok(());
}

pub fn create_collections_all(
    space_absolute_path: &Path,
    create_collection_dto: &CreateCollectionDto,
) -> Result<String, Error> {
    let mut dirs = Vec::new();
    for dir_display_name in create_collection_dto.relative_path.split('/') {
        let dir_display_name = dir_display_name.trim();
        let dir_sanitized_name = dir_display_name
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<&str>>()
            .join("-");

        if dir_display_name.is_empty() || dir_sanitized_name.is_empty() {
            continue;
        }

        dirs.push((dir_sanitized_name.clone(), dir_display_name.to_string()));
    }

    let collection_parent_absolute_path =
        space_absolute_path.join(create_collection_dto.parent_relative_path.clone());
    let mut collections_relative_path = String::new();

    for (dir_sanitized_name, dir_display_name) in &dirs {
        let mut current_collection_relative_path = collections_relative_path.clone();

        if !current_collection_relative_path.is_empty() {
            current_collection_relative_path.push_str("/");
        }
        current_collection_relative_path.push_str(dir_sanitized_name);

        fs::create_dir(
            &collection_parent_absolute_path.join(current_collection_relative_path.clone()),
        )
        .unwrap_or_else(|err| {
            if err.kind() != ErrorKind::AlreadyExists {
                panic!("Failed to create collection directory: {:?}", err);
            }
        });

        let current_collection_relative_path_from_root = utils::join_str_paths(vec![
            create_collection_dto.parent_relative_path.as_str(),
            current_collection_relative_path.as_str(),
        ]);

        save_display_name_if_not_exists(
            &space_absolute_path,
            &current_collection_relative_path_from_root,
            &dir_display_name,
        )
        .unwrap_or_else(|err| {
            eprintln!("Failed to save display name {}", err);
        });

        if !collections_relative_path.is_empty() {
            collections_relative_path.push_str("/");
        }
        collections_relative_path.push_str(&dir_sanitized_name);
    }

    return Ok(collections_relative_path);
}
