use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{App, Manager};

use crate::{
    space::{
        self,
        models::{Space, SpaceReference},
    },
    store,
};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct ZakuState {
    pub active_space: Option<Space>,
    pub spacerefs: Vec<SpaceReference>,
}

pub fn initialize(app: &mut App) {
    let active_spaceref = store::get_active_spaceref().or_else(|| space::first_valid_spaceref());
    let spacerefs = store::get_spacerefs();
    let state = app.app_handle().state::<Mutex<ZakuState>>();
    let mut zaku_state = state.lock().unwrap();

    if let Some(active_spaceref) = active_spaceref {
        let active_spacepath = PathBuf::from(active_spaceref.path);

        match space::parse_space(&active_spacepath) {
            Ok(active_space) => {
                zaku_state.active_space = Some(active_space);
            }
            Err(_) => match space::first_valid_spaceref() {
                Some(valid_space_reference) => {
                    store::set_active_spaceref(valid_space_reference.clone());

                    let valid_space_path = PathBuf::from(valid_space_reference.path);

                    match space::parse_space(&valid_space_path) {
                        Ok(valid_space) => {
                            zaku_state.active_space = Some(valid_space);
                        }
                        Err(err) => {
                            eprintln!("Error parsing space: {}", err);
                        }
                    }
                }
                None => {}
            },
        };
    }

    zaku_state.spacerefs = spacerefs;

    return ();
}
