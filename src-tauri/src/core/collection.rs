use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Error, ErrorKind, Write};
use std::path::Path;
use toml;

use crate::core::utils;
use crate::models::collection::CreateCollectionDto;

pub fn display_name_by_relpath(space_abspath: &Path) -> Result<HashMap<String, String>, Error> {
    let display_name_file_abspath = space_abspath.join(".zaku/collections/display_name");

    let content = match fs::read_to_string(&display_name_file_abspath.with_extension("toml")) {
        Ok(content) => content,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            let initialized_hash_map: HashMap<String, String> = HashMap::new();

            if let Some(parent) = display_name_file_abspath.parent() {
                fs::create_dir_all(parent)
                    .expect("Failed to create display name's parent directories");
            }

            let mut display_name_file =
                File::create(&display_name_file_abspath.with_extension("toml"))
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
                    display_name_file_abspath.display(),
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
    space_abspath: &Path,
    collection_relpath_from_root: &str,
    collection_display_name: &str,
) -> Result<(), Error> {
    let display_name_file_abspath = space_abspath.join(".zaku/collections/display_name");

    let mut collection_name_by_relpath = display_name_by_relpath(&space_abspath)
        .expect("Failed to get display names by relative path");

    collection_name_by_relpath
        .entry(collection_relpath_from_root.to_string())
        .or_insert(collection_display_name.to_string());

    let toml_content =
        toml::to_string_pretty(&collection_name_by_relpath).expect("Failed to serialize TOML");

    fs::write(
        &display_name_file_abspath.with_extension("toml"),
        toml_content,
    )
    .expect("Failed to write display names to file");

    return Ok(());
}

pub fn create_collections_all(
    space_abspath: &Path,
    create_collection_dto: &CreateCollectionDto,
) -> Result<String, Error> {
    let mut dirs = Vec::new();
    for dir_display_name in create_collection_dto.relpath.split('/') {
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

    let collection_parent_abspath =
        space_abspath.join(create_collection_dto.parent_relpath.clone());
    let mut collections_relpath = String::new();

    for (dir_sanitized_name, dir_display_name) in &dirs {
        let mut cur_collection_relpath = collections_relpath.clone();

        if !cur_collection_relpath.is_empty() {
            cur_collection_relpath.push_str("/");
        }
        cur_collection_relpath.push_str(dir_sanitized_name);

        fs::create_dir(&collection_parent_abspath.join(cur_collection_relpath.clone()))
            .unwrap_or_else(|err| {
                if err.kind() != ErrorKind::AlreadyExists {
                    panic!("Failed to create collection directory: {:?}", err);
                }
            });

        let cur_collection_relpath_from_root = utils::join_str_paths(vec![
            create_collection_dto.parent_relpath.as_str(),
            cur_collection_relpath.as_str(),
        ]);

        save_display_name_if_not_exists(
            &space_abspath,
            &cur_collection_relpath_from_root,
            &dir_display_name,
        )
        .unwrap_or_else(|err| {
            eprintln!("Failed to save display name {}", err);
        });

        if !collections_relpath.is_empty() {
            collections_relpath.push_str("/");
        }
        collections_relpath.push_str(&dir_sanitized_name);
    }

    return Ok(collections_relpath);
}
