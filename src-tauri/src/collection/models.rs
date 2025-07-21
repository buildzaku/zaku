use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{error::Result, request::models::HttpReq};

#[derive(Clone, Debug, Serialize, Deserialize, Default, Type)]
pub struct CollectionMeta {
    pub fsname: String,
    pub name: Option<String>,
    pub is_expanded: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, Type)]
pub struct Collection {
    pub meta: CollectionMeta,
    pub requests: Vec<HttpReq>,
    pub collections: Vec<Collection>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct CreateCollectionDto {
    pub location_relpath: PathBuf,
    pub relpath: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct CreateNewCollection {
    pub parent_relpath: String,
    pub relpath: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ColName {
    mappings: BTreeMap<String, String>,

    #[serde(skip)]
    space_abspath: PathBuf,
}

impl ColName {
    pub fn load(space_abspath: &Path) -> Result<Self> {
        let file_abspath = space_abspath
            .join(".zaku")
            .join("collections")
            .join("name.toml");

        let content = match fs::read_to_string(&file_abspath) {
            Ok(content) => content,
            Err(_) => {
                if let Some(parent) = file_abspath.parent() {
                    fs::create_dir_all(parent)?;
                }

                let colname = Self {
                    mappings: BTreeMap::new(),
                    space_abspath: space_abspath.to_path_buf(),
                };

                let serialized = toml::to_string_pretty(&colname)?;
                fs::write(&file_abspath, &serialized)?;
                serialized
            }
        };

        let mut colname: ColName = toml::from_str(&content)?;
        colname.space_abspath = space_abspath.to_path_buf();

        Ok(colname)
    }

    pub fn get(&self, relpath: &Path) -> Option<String> {
        self.mappings
            .get(&relpath.to_string_lossy().to_string())
            .cloned()
    }

    pub fn set(&mut self, relpath: &Path, name: &str) -> Result<()> {
        let mapping_exists = self
            .mappings
            .contains_key(&relpath.to_string_lossy().to_string());
        if !mapping_exists {
            self.mappings
                .insert(relpath.to_string_lossy().to_string(), name.into());
            self.save()?;
        }

        Ok(())
    }

    fn save(&self) -> Result<()> {
        let file_abspath = self
            .space_abspath
            .join(".zaku")
            .join("collections")
            .join("name.toml");

        let toml_content = toml::to_string_pretty(self)?;
        fs::write(&file_abspath, toml_content)?;

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct CollectionRcRefCell {
    pub meta: CollectionMeta,
    pub requests: Vec<HttpReq>,
    pub collections: Vec<Rc<RefCell<CollectionRcRefCell>>>,
}
