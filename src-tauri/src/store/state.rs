use serde::{Deserialize, Serialize};
use specta::Type;
use std::{
    fs,
    ops::Deref,
    path::{Path, PathBuf},
};

use crate::{
    error::Result,
    space::{
        self,
        models::{Space, SpaceReference},
    },
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Type)]
pub enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct UserSettings {
    pub default_theme: Theme,
}

#[derive(Debug, Serialize, Deserialize, Type)]
pub struct SharedState {
    pub space: Option<Space>,
    pub spacerefs: Vec<SpaceReference>,
    pub user_settings: UserSettings,
}

impl SharedState {
    pub fn from_state_store(state_store: &StateStore) -> Result<Self> {
        let parsed_space = match &state_store.spaceref {
            Some(spaceref) => Some(space::parse_space(&spaceref.abspath, state_store)?),
            None => None,
        };

        Ok(SharedState {
            space: parsed_space,
            spacerefs: state_store.spacerefs.clone(),
            user_settings: state_store.user_settings.clone(),
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct State {
    pub spaceref: Option<SpaceReference>,
    pub spacerefs: Vec<SpaceReference>,
    pub user_settings: UserSettings,
}

#[derive(Debug)]
pub struct StateStore {
    state: State,
    pub abspath: PathBuf,
}

impl Deref for StateStore {
    type Target = State;

    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl StateStore {
    pub fn new(state_store_abspath: &Path) -> Self {
        Self {
            state: State {
                spaceref: None,
                spacerefs: Vec::new(),
                user_settings: UserSettings {
                    default_theme: Theme::System,
                },
            },
            abspath: state_store_abspath.to_path_buf(),
        }
    }

    fn init(state_store_abspath: &Path) -> Result<StateStore> {
        if !state_store_abspath.exists() {
            let default_store = Self::new(state_store_abspath);
            default_store.fswrite()?;

            return Ok(default_store);
        }

        let state_content = fs::read_to_string(state_store_abspath)?;

        match serde_json::from_str::<State>(&state_content) {
            Ok(state) => Ok(Self {
                state,
                abspath: state_store_abspath.to_path_buf(),
            }),
            Err(_) => {
                // corrupt JSON, use default
                let default_store = Self::new(state_store_abspath);
                default_store.fswrite()?;

                Ok(default_store)
            }
        }
    }

    fn fswrite(&self) -> Result<()> {
        if let Some(parent) = self.abspath.parent() {
            fs::create_dir_all(parent)?;
        }

        let serialized_state = serde_json::to_string_pretty(&self.state)?;
        fs::write(&self.abspath, serialized_state)?;

        Ok(())
    }

    pub fn get(state_store_abspath: &Path) -> Result<StateStore> {
        Self::init(state_store_abspath)
    }

    /// Updates the store using the provided mutator function and
    /// persists changes to the filesystem
    pub fn update<F>(&mut self, mutator: F) -> Result<()>
    where
        F: FnOnce(&mut State),
    {
        mutator(&mut self.state);
        self.fswrite()
    }

    /// Consumes the store and returns the inner `State`
    pub fn into_inner(self) -> State {
        self.state
    }

    /// Returns the data directory, which is the parent directory of the state store file
    pub fn datadir_abspath(&self) -> &Path {
        self.abspath
            .parent()
            .expect("StateStore abspath should have a parent directory")
    }
}
