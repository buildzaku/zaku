use std::{
    collections::HashMap,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use crate::{
    collection,
    error::{Error, Result},
    space::models::{
        CreateSpaceDto, SerializedCookie, Space, SpaceConfigFile, SpaceMeta, SpaceReference,
    },
    store::{self, SpaceCookieStore, SpaceSettingsStore, StateStore},
    utils,
};

pub mod models;

pub fn create_space(dto: CreateSpaceDto, state_store: &mut StateStore) -> Result<SpaceReference> {
    let location = PathBuf::from(dto.location.as_str());
    if !location.exists() {
        return Err(Error::FileNotFound(format!(
            "Location does not exist: {}",
            dto.location
        )));
    }

    let space_dirname = utils::to_fsname(&dto.name)?;
    let space_abspath = location.join(&space_dirname);
    if space_abspath.exists() {
        return Err(Error::FileConflict(format!(
            "Directory with this name already exists: {}",
            space_abspath.to_string_lossy()
        )));
    }

    if state_store
        .spacerefs
        .iter()
        .any(|sr| sr.abspath == *space_abspath)
    {
        return Err(Error::FileNotFound(format!(
            "Space already exists in saved spaces: {}",
            space_abspath.to_string_lossy()
        )));
    }

    fs::create_dir(&space_abspath)?;
    let config_dir = space_abspath.join(".zaku");
    fs::create_dir(&config_dir)?;

    let mut config_file = File::create(config_dir.join("config.toml"))?;
    let config = SpaceConfigFile {
        meta: SpaceMeta {
            name: dto.name.clone(),
        },
    };

    config_file.write_all(toml::to_string_pretty(&config)?.as_bytes())?;

    let spaceref = SpaceReference {
        abspath: space_abspath.to_path_buf(),
        name: dto.name,
    };

    state_store.update(|state| {
        state.spaceref = Some(spaceref.clone());
        state.spacerefs.push(spaceref.clone());
    })?;

    Ok(spaceref)
}

pub fn parse_space(space_abspath: &Path, state_store: &StateStore) -> Result<Space> {
    let space_abspath_str = space_abspath.to_string_lossy();
    let root_collection = collection::parse_root_collection(space_abspath, state_store)?;
    let space_config_file = parse_spacecfg(space_abspath)?;

    let sck_store_abspath =
        store::utils::sck_store_abspath(state_store.datadir_abspath(), space_abspath);
    let sck_store = SpaceCookieStore::get(&sck_store_abspath)?;
    let sck_store_mtx = sck_store.cookies.lock().unwrap();
    let cookies: Vec<SerializedCookie> = sck_store_mtx
        .iter_any()
        .map(SerializedCookie::from_cookie_store)
        .collect();
    let cookies_by_domain: HashMap<String, Vec<SerializedCookie>> =
        cookies.into_iter().fold(HashMap::new(), |mut acc, ck| {
            acc.entry(ck.domain.clone()).or_default().push(ck);
            acc
        });

    let sst_store_abspath =
        store::utils::sst_store_abspath(state_store.datadir_abspath(), space_abspath);
    let space_settings = SpaceSettingsStore::get(&sst_store_abspath)?.into_inner();

    Ok(Space {
        abspath: space_abspath_str.into_owned(),
        meta: space_config_file.meta,
        root_collection,
        cookies: cookies_by_domain,
        settings: space_settings,
    })
}

pub fn parse_spacecfg(space_abspath: &Path) -> Result<SpaceConfigFile> {
    let path = space_abspath.join(".zaku/config.toml");
    let content =
        fs::read_to_string(&path).map_err(|_| Error::FileNotFound(path.display().to_string()))?;
    let config = toml::from_str(&content)
        .map_err(|e| Error::FileReadError(format!("{}: {}", path.display(), e)))?;

    Ok(config)
}

pub fn first_valid_spaceref(state_store: &StateStore) -> Option<SpaceReference> {
    let spacerefs = state_store.spacerefs.clone();

    spacerefs.into_iter().find_map(|space_reference| {
        let space_abspath = &space_reference.abspath;

        match parse_spacecfg(space_abspath) {
            Ok(_) => Some(space_reference),
            Err(_) => None,
        }
    })
}
