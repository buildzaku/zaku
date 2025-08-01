use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs,
    ops::Deref,
    path::{Path, PathBuf},
    rc::Rc,
};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{error::Result, request::models::HttpReq, store};

#[derive(Clone, Debug, Serialize, Deserialize, Default, Type)]
pub struct CollectionMeta {
    pub fsname: String,
    pub name: Option<String>,
    pub is_expanded: bool,
    pub relpath: PathBuf,
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
    pub relpath: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct CreateNewCollection {
    pub location_relpath: PathBuf,
    pub relpath: PathBuf,
}

#[derive(Clone, Debug)]
pub struct CollectionRcRefCell {
    pub meta: CollectionMeta,
    pub requests: Vec<HttpReq>,
    pub collections: Vec<Rc<RefCell<CollectionRcRefCell>>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SpaceCollectionsMetadata {
    pub mappings: BTreeMap<PathBuf, String>,
}

#[derive(Debug)]
pub struct SpaceCollectionsMetadataStore {
    metadata: SpaceCollectionsMetadata,
    abspath: PathBuf,
}

impl Deref for SpaceCollectionsMetadataStore {
    type Target = SpaceCollectionsMetadata;

    fn deref(&self) -> &Self::Target {
        &self.metadata
    }
}

impl SpaceCollectionsMetadataStore {
    fn new(space_abspath: &Path) -> Self {
        let file_abspath = store::utils::scmt_store_abspath(space_abspath);

        Self {
            metadata: SpaceCollectionsMetadata {
                mappings: BTreeMap::new(),
            },
            abspath: file_abspath,
        }
    }

    fn init(space_abspath: &Path) -> Result<Self> {
        let file_abspath = store::utils::scmt_store_abspath(space_abspath);
        if !file_abspath.exists() {
            let store = Self::new(space_abspath);
            store.fswrite()?;

            return Ok(store);
        }

        let content = fs::read_to_string(&file_abspath)?;

        match toml::from_str::<SpaceCollectionsMetadata>(&content) {
            Ok(metadata) => Ok(Self {
                metadata,
                abspath: file_abspath,
            }),
            Err(_) => {
                // corrupt JSON, use default
                let store = Self::new(space_abspath);
                store.fswrite()?;

                Ok(store)
            }
        }
    }

    fn fswrite(&self) -> Result<()> {
        if let Some(parent) = self.abspath.parent() {
            fs::create_dir_all(parent)?;
        }

        let toml_content = toml::to_string_pretty(&self.metadata)?;
        fs::write(&self.abspath, toml_content)?;

        Ok(())
    }

    pub fn get(space_abspath: &Path) -> Result<Self> {
        Self::init(space_abspath)
    }

    /// Updates the store using the provided mutator function and
    /// persists changes to the filesystem
    pub fn update<F>(&mut self, mutator: F) -> Result<()>
    where
        F: FnOnce(&mut SpaceCollectionsMetadata),
    {
        mutator(&mut self.metadata);
        self.fswrite()
    }

    /// Consumes the store and returns the inner `SpaceCollectionsMetadata`
    pub fn into_inner(self) -> SpaceCollectionsMetadata {
        self.metadata
    }
}
