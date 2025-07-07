use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::State;

use crate::core::cookie::SpaceCookies;
use crate::core::{space, store};
use crate::models::space::{CreateSpaceDto, SpaceConfigFile, SpaceMeta, SpaceReference};
use crate::models::zaku::{ZakuError, ZakuState};

#[specta::specta]
#[tauri::command]
pub fn create_space(
    create_space_dto: CreateSpaceDto,
    state: State<Mutex<ZakuState>>,
) -> Result<SpaceReference, ZakuError> {
    let location = PathBuf::from(create_space_dto.location.as_str());
    if !location.exists() {
        return Err(ZakuError {
            error: create_space_dto.location,
            message: "Location does not exist.".to_string(),
        });
    }

    let space_abspath = location.join(create_space_dto.name.clone());
    let mut spacerefs = store::get_spacerefs();
    let mut zaku_state = state.lock().unwrap();

    if spacerefs
        .iter()
        .any(|sr| sr.path == space_abspath.to_string_lossy())
    {
        return Err(ZakuError {
            error: space_abspath.to_string_lossy().to_string(),
            message: "Space already exists in saved spaces.".to_string(),
        });
    }
    if space_abspath.exists() {
        return Err(ZakuError {
            error: space_abspath.to_string_lossy().to_string(),
            message: "Directory with this name already exists.".to_string(),
        });
    }

    fs::create_dir(&space_abspath).expect("Failed to create space directory");

    let space_config_dir = space_abspath.join(".zaku");
    fs::create_dir(&space_config_dir).expect("Failed to create `.zaku` directory");

    let mut space_config_file =
        File::create(&space_config_dir.join("config").with_extension("toml"))
            .expect("Failed to create `config.toml`");

    let space_config = SpaceConfigFile {
        meta: SpaceMeta {
            name: create_space_dto.name.clone(),
        },
    };

    space_config_file
        .write_all(
            toml::to_string_pretty(&space_config)
                .expect("Failed to serialize space config")
                .as_bytes(),
        )
        .expect("Failed to write to config file");

    let spaceref = SpaceReference {
        path: space_abspath.to_string_lossy().to_string(),
        name: create_space_dto.name,
    };

    store::set_active_spaceref(spaceref.clone());
    spacerefs.push(spaceref.clone());
    store::set_spacerefs(spacerefs.clone());

    match space::parse_space(&PathBuf::from(spaceref.clone().path)) {
        Ok(active_space) => {
            zaku_state.active_space = Some(active_space);
            zaku_state.spacerefs = spacerefs;
        }
        Err(_) => {
            // TODO - handle
        }
    }

    return Ok(spaceref);
}

#[specta::specta]
#[tauri::command]
pub fn set_active_space(
    space_reference: SpaceReference,
    state: State<Mutex<ZakuState>>,
) -> Result<(), ZakuError> {
    let mut zaku_state = state.lock().unwrap();
    let space_abspath = PathBuf::from(space_reference.path.as_str());

    if !space_abspath.exists() {
        return Err(ZakuError {
            error: space_abspath.to_string_lossy().to_string(),
            message: "Directory does not exist.".to_string(),
        });
    }

    match space::parse_space(&space_abspath) {
        Ok(space) => {
            store::set_active_spaceref(space_reference.clone());
            store::insert_spaceref_if_missing(space_reference.clone());

            zaku_state.active_space = Some(space);
            zaku_state.spacerefs = store::get_spacerefs();

            return Ok(());
        }
        Err(err) => Err(ZakuError {
            error: err.to_string(),
            message: "Unable to parse space.".to_string(),
        }),
    }
}

#[specta::specta]
#[tauri::command]
pub fn delete_space(space_reference: SpaceReference, state: State<Mutex<ZakuState>>) -> () {
    let mut zaku_state = state.lock().unwrap();
    store::delete_spaceref(space_reference);

    let active_space = store::get_active_spaceref();

    if let None = active_space {
        zaku_state.active_space = None;

        match space::first_valid_spaceref() {
            Some(valid_space_reference) => {
                store::set_active_spaceref(valid_space_reference.clone());

                match space::parse_space(&PathBuf::from(valid_space_reference.clone().path)) {
                    Ok(active_space) => {
                        zaku_state.active_space = Some(active_space);
                    }
                    Err(_) => {}
                }
            }
            None => {}
        }
    }

    zaku_state.spacerefs = store::get_spacerefs();

    return ();
}

#[specta::specta]
#[tauri::command]
pub fn get_spaceref(path: String) -> Result<SpaceReference, ZakuError> {
    let space_abspath = PathBuf::from(path.as_str());

    match space::parse_spacecfg(&space_abspath) {
        Ok(space_config_file) => {
            let space_reference = SpaceReference {
                path: space_abspath.to_string_lossy().to_string(),
                name: space_config_file.meta.name,
            };

            return Ok(space_reference);
        }
        Err(err) => {
            return Err(ZakuError {
                error: err.to_string(),
                message: "Unable to parse space.".to_string(),
            });
        }
    }
}

#[specta::specta]
#[tauri::command]
pub fn delete_cookie(name: &str, domain: &str, path: &str) -> Result<(), ZakuError> {
    let active_space = store::get_active_spaceref().ok_or(ZakuError {
        error: "No active space found.".into(),
        message: "No active space found.".into(),
    })?;
    let space_abspath = active_space.path.as_str();
    SpaceCookies::remove(space_abspath, name, domain, path);

    return Ok(());
}
