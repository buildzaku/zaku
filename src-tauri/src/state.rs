use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::PathBuf;

use crate::{
    error::Result,
    space::{
        self,
        models::{Space, SpaceReference},
    },
    store::{self, Store, UserSettings, UserSettingsStore},
};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct SharedState {
    pub space: Option<Space>,
    pub spacerefs: Vec<SpaceReference>,
    pub user_settings: UserSettings,
}

pub fn load_sharedstate() -> Result<SharedState> {
    let datadir_abspath = store::utils::datadir_abspath();
    let store_abspath = store::utils::store_abspath(&datadir_abspath);
    let mut store = Store::get(&store_abspath)?;

    let fallback_spaceref = space::first_valid_spaceref();

    let parsed_space = store
        .spaceref
        .clone()
        .or(fallback_spaceref.clone())
        .map(|spaceref| PathBuf::from(spaceref.path))
        .and_then(|space_abspath| {
            space::parse_space(&space_abspath).ok().or_else(|| {
                fallback_spaceref.as_ref().and_then(|space_ref| {
                    if let Err(e) = store.update(|store| {
                        store.spaceref = Some(space_ref.clone());
                    }) {
                        eprintln!("Failed to set spaceref: {e}");
                    }
                    let space_abspath = PathBuf::from(&space_ref.path);
                    space::parse_space(&space_abspath).ok()
                })
            })
        });

    let ust_store_abspath = store::utils::ust_store_abspath(&datadir_abspath);
    let user_settings = UserSettingsStore::get(&ust_store_abspath)
        .expect("Failed to get UserSettings")
        .into_inner();

    Ok(SharedState {
        space: parsed_space,
        spacerefs: store.spacerefs,
        user_settings,
    })
}
