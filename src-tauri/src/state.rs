use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::PathBuf;

use crate::{
    error::Result,
    space::{
        self,
        models::{Space, SpaceReference},
    },
    store,
};

#[derive(Clone, Debug, Serialize, Deserialize, Type, Default)]
pub struct SharedState {
    pub space: Option<Space>,
    pub spacerefs: Vec<SpaceReference>,
}

pub fn load_sharedstate() -> Result<SharedState> {
    let spaceref = store::get_spaceref();
    let fallback_spaceref = space::first_valid_spaceref();
    let spacerefs = store::get_spacerefs();

    let parsed_space = spaceref
        .or(fallback_spaceref.clone())
        .map(|spaceref| PathBuf::from(spaceref.path))
        .and_then(|space_abspath| {
            space::parse_space(&space_abspath).ok().or_else(|| {
                fallback_spaceref.as_ref().and_then(|space_ref| {
                    if let Err(e) = store::set_spaceref(space_ref.clone()) {
                        eprintln!("Failed to set spaceref: {e}");
                    }
                    let space_abspath = PathBuf::from(&space_ref.path);
                    space::parse_space(&space_abspath).ok()
                })
            })
        });

    Ok(SharedState {
        space: parsed_space,
        spacerefs,
    })
}
