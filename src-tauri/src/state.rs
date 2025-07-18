use serde::{Deserialize, Serialize};
use specta::Type;
use std::{path::PathBuf, sync::Mutex};
use tauri::{App, Manager};

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

pub fn initialize(app: &mut App) -> Result<()> {
    let spaceref = store::get_spaceref().or_else(space::first_valid_spaceref);
    let spacerefs = store::get_spacerefs();
    let sharedstate_mtx = app.app_handle().state::<Mutex<SharedState>>();
    let mut sharedstate = sharedstate_mtx.lock().unwrap();

    if let Some(spaceref) = spaceref {
        let spacepath = PathBuf::from(spaceref.path);

        space::parse_space(&spacepath)
            .map(|space| {
                sharedstate.space = Some(space);
            })
            .or_else(|_| {
                if let Some(valid_space_reference) = space::first_valid_spaceref() {
                    store::set_spaceref(valid_space_reference.clone())?;

                    let valid_space_path = PathBuf::from(&valid_space_reference.path);
                    space::parse_space(&valid_space_path)
                        .map(|valid_space| {
                            sharedstate.space = Some(valid_space);
                        })
                        .map_err(|e| {
                            eprintln!("Error parsing space: {e}");

                            e
                        })
                } else {
                    Ok(())
                }
            })?;
    }

    sharedstate.spacerefs = spacerefs;

    Ok(())
}
