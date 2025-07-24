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
        CreateSpaceDto, Space, SpaceConfigFile, SpaceCookie, SpaceMeta, SpaceReference,
    },
    state::SharedState,
    store::{self, SpaceCookieStore, SpaceSettingsStore, Store},
    utils,
};

pub mod models;

pub fn create_space(
    dto: CreateSpaceDto,
    sharedstate: &mut SharedState,
    store: &mut Store,
) -> Result<SpaceReference> {
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
        return Err(Error::FileNotFound(format!(
            "Directory with this name already exists: {}",
            space_abspath.to_string_lossy()
        )));
    }

    if store
        .spacerefs
        .iter()
        .any(|sr| sr.path == space_abspath.to_string_lossy())
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
        path: space_abspath.to_string_lossy().to_string(),
        name: dto.name,
    };

    store.update(|store| {
        store.spaceref = Some(spaceref.clone());
        store.spacerefs.push(spaceref.clone());
    })?;

    match parse_space(&PathBuf::from(&spaceref.path)) {
        Ok(space) => {
            sharedstate.space = Some(space);
            sharedstate.spacerefs = store.spacerefs.clone();
        }
        Err(e) => {
            #[cfg(debug_assertions)]
            eprintln!("Warning: Failed to parse space: {e:?}");
        }
    }

    Ok(spaceref)
}

pub fn parse_space(space_abspath: &Path) -> Result<Space> {
    let space_abspath_str = space_abspath.to_string_lossy();
    let root_collection = collection::parse_root_collection(space_abspath)?;
    let space_config_file = parse_spacecfg(space_abspath)?;

    let datadir_abspath = store::utils::datadir_abspath();
    let sck_store_abspath = store::utils::sck_store_abspath(&datadir_abspath, space_abspath);
    let sck_store = SpaceCookieStore::get(&sck_store_abspath)?;
    let sck_store_mtx = sck_store.cookies.lock().unwrap();
    let cookies: Vec<SpaceCookie> = sck_store_mtx
        .iter_any()
        .map(SpaceCookie::from_cookie_store)
        .collect();
    let cookies_by_domain: HashMap<String, Vec<SpaceCookie>> =
        cookies.into_iter().fold(HashMap::new(), |mut acc, ck| {
            acc.entry(ck.domain.clone()).or_default().push(ck);
            acc
        });

    let sst_store_abspath = store::utils::sst_store_abspath(&datadir_abspath, space_abspath);
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

pub fn first_valid_spaceref() -> Option<SpaceReference> {
    let store_abspath = store::utils::store_abspath(&store::utils::datadir_abspath());
    let spacerefs = Store::get(&store_abspath).ok()?.spacerefs;

    spacerefs.into_iter().find_map(|space_reference| {
        let space_abspath = PathBuf::from(&space_reference.path);

        match parse_spacecfg(&space_abspath) {
            Ok(_) => Some(space_reference),
            Err(_) => None,
        }
    })
}
