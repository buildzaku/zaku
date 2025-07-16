use std::{
    collections::HashMap,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use crate::{
    collection,
    error::{Error, Result},
    request,
    space::models::{
        CreateSpaceDto, Space, SpaceConfigFile, SpaceCookie, SpaceMeta, SpaceReference,
    },
    state::SharedState,
    store::{
        self,
        models::{SpaceCookies, SpaceSettings},
    },
    utils,
};

pub mod models;

pub fn create_space(dto: CreateSpaceDto, sharedstate: &mut SharedState) -> Result<SpaceReference> {
    let location = PathBuf::from(dto.location.as_str());
    if !location.exists() {
        return Err(Error::FileNotFound(format!(
            "Location does not exist: {}",
            dto.location
        )));
    }

    let space_dirname = utils::sanitize_path_segment(&dto.name);
    let space_abspath = location.join(&space_dirname);
    let mut spacerefs = store::get_spacerefs();

    if spacerefs
        .iter()
        .any(|sr| sr.path == space_abspath.to_string_lossy())
    {
        return Err(Error::FileNotFound(format!(
            "Space already exists in saved spaces: {}",
            space_abspath.to_string_lossy()
        )));
    }
    if space_abspath.exists() {
        return Err(Error::FileNotFound(format!(
            "Directory with this name already exists: {}",
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
            is_expanded: true,
        },
    };

    config_file.write_all(toml::to_string_pretty(&config)?.as_bytes())?;

    let spaceref = SpaceReference {
        path: space_abspath.to_string_lossy().to_string(),
        name: dto.name,
    };

    store::set_active_spaceref(spaceref.clone())?;
    spacerefs.push(spaceref.clone());
    store::set_spacerefs(spacerefs.clone())?;

    if let Ok(active_space) = parse_space(&PathBuf::from(&spaceref.path)) {
        sharedstate.active_space = Some(active_space);
        sharedstate.spacerefs = spacerefs;
    }

    Ok(spaceref)
}

pub fn parse_space(space_abspath: &Path) -> Result<Space> {
    let space_abspath_str = space_abspath.to_string_lossy();
    let collections = collection::parse_cols("", &space_abspath_str)?;
    let requests = request::parse_reqs(&space_abspath_str)?;
    let space_config_file = parse_spacecfg(space_abspath)?;
    let cookie_store = SpaceCookies::load(space_abspath_str.as_ref())?;
    let cookie_store_mtx = cookie_store.lock().unwrap();
    let cookies: Vec<SpaceCookie> = cookie_store_mtx
        .iter_any()
        .map(SpaceCookie::from_cookie_store)
        .collect();
    let cookies_by_domain: HashMap<String, Vec<SpaceCookie>> =
        cookies.into_iter().fold(HashMap::new(), |mut acc, ck| {
            acc.entry(ck.domain.clone()).or_default().push(ck);
            acc
        });

    let settings = SpaceSettings::load(&space_abspath_str)?;

    Ok(Space {
        abspath: space_abspath_str.into_owned(),
        meta: space_config_file.meta,
        collections,
        requests,
        cookies: cookies_by_domain,
        settings,
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
    store::get_spacerefs()
        .into_iter()
        .find_map(|space_reference| {
            let space_abspath = PathBuf::from(&space_reference.path);

            match parse_spacecfg(&space_abspath) {
                Ok(_) => Some(space_reference),
                Err(_) => None,
            }
        })
}
