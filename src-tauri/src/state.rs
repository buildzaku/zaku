use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::PathBuf;
use std::sync::Mutex;
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
    pub active_space: Option<Space>,
    pub spacerefs: Vec<SpaceReference>,
}

pub fn initialize(app: &mut App) -> Result<()> {
    let active_spaceref = store::get_active_spaceref().or_else(space::first_valid_spaceref);
    let spacerefs = store::get_spacerefs();
    let sharedstate_mtx = app.app_handle().state::<Mutex<SharedState>>();
    let mut sharedstate = sharedstate_mtx.lock().unwrap();

    if let Some(active_spaceref) = active_spaceref {
        let active_spacepath = PathBuf::from(active_spaceref.path);

        space::parse_space(&active_spacepath)
            .map(|active_space| {
                sharedstate.active_space = Some(active_space);
            })
            .or_else(|_| {
                if let Some(valid_space_reference) = space::first_valid_spaceref() {
                    store::set_active_spaceref(valid_space_reference.clone())?;

                    let valid_space_path = PathBuf::from(&valid_space_reference.path);
                    space::parse_space(&valid_space_path)
                        .map(|valid_space| {
                            sharedstate.active_space = Some(valid_space);
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
