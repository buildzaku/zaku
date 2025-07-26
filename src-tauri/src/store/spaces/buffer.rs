use serde::{Deserialize, Serialize};
use specta::Type;
use std::{
    collections::HashMap,
    fs,
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::{
    error::{Error, Result},
    request::models::{HttpReq, ReqCfg, ReqMeta},
};

static SBF_STORE_UPDATE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, Serialize, Deserialize, Default, Type)]
pub struct SpaceBuffer {
    pub requests: HashMap<String, ReqBuffer>,
}

#[derive(Debug, Clone)]
pub struct SpaceBufferStore {
    buffer: SpaceBuffer,
    abspath: PathBuf,
}

impl Deref for SpaceBufferStore {
    type Target = SpaceBuffer;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl DerefMut for SpaceBufferStore {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buffer
    }
}

impl SpaceBufferStore {
    fn new(sbf_store_abspath: PathBuf) -> Self {
        Self {
            buffer: SpaceBuffer {
                requests: HashMap::new(),
            },
            abspath: sbf_store_abspath,
        }
    }

    fn init(sbf_store_abspath: &Path) -> Result<Arc<Mutex<SpaceBufferStore>>> {
        if !sbf_store_abspath.exists() {
            let default_buffer = Arc::new(Mutex::new(Self::new(sbf_store_abspath.to_path_buf())));
            Self::fswrite(&default_buffer)?;

            return Ok(default_buffer);
        }

        let content = fs::read_to_string(sbf_store_abspath)
            .map_err(|_| Error::FileReadError("Failed to read from space buffer".into()))?;

        let sbf_store = match serde_json::from_str::<SpaceBuffer>(&content) {
            Ok(buffer) => Self {
                buffer,
                abspath: sbf_store_abspath.to_path_buf(),
            },
            Err(_) => {
                // corrupt JSON, use default
                let default_buffer = Self::new(sbf_store_abspath.to_path_buf());
                let buffer_arc = Arc::new(Mutex::new(default_buffer));
                Self::fswrite(&buffer_arc)?;

                return Ok(buffer_arc);
            }
        };

        Ok(Arc::new(Mutex::new(sbf_store)))
    }

    fn fswrite(sbf_store: &Arc<Mutex<SpaceBufferStore>>) -> Result<()> {
        let sbf_store_mtx = sbf_store
            .lock()
            .map_err(|_| Error::LockError("Failed to acquire mutex lock".into()))?;

        if let Some(parent) = sbf_store_mtx.abspath.parent() {
            fs::create_dir_all(parent)?;
        }

        let serialized_store = serde_json::to_string_pretty(&sbf_store_mtx.buffer)?;
        fs::write(&sbf_store_mtx.abspath, serialized_store)?;

        Ok(())
    }

    pub fn get(sbf_store_abspath: &Path) -> Result<Arc<Mutex<SpaceBufferStore>>> {
        Self::init(sbf_store_abspath)
    }

    pub fn update<F>(sbf_store: &Arc<Mutex<SpaceBufferStore>>, mutator: F) -> Result<()>
    where
        F: FnOnce(&Arc<Mutex<SpaceBufferStore>>),
    {
        let _guard = SBF_STORE_UPDATE_LOCK.lock().unwrap();

        mutator(sbf_store);
        Self::fswrite(sbf_store)?;

        Ok(())
    }

    /// Consumes the store and returns the inner `SpaceBuffer`
    pub fn into_inner(self) -> SpaceBuffer {
        self.buffer
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct ReqBuffer {
    pub meta: ReqMeta,
    pub config: ReqCfg,
}

impl ReqBuffer {
    pub fn from_req(req: &HttpReq) -> Self {
        let meta = ReqMeta {
            fsname: req.meta.fsname.clone(),
            name: req.meta.name.clone(),
            has_unsaved_changes: true,
        };

        let config = ReqCfg {
            method: req.config.method.clone(),
            url: req.config.url.clone(),
            headers: req.config.headers.clone(),
            parameters: req.config.parameters.clone(),
            content_type: req.config.content_type.clone(),
            body: req.config.body.clone(),
        };

        Self { meta, config }
    }
}
