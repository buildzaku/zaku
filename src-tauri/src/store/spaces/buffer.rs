use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::{
    error::{Error, Result},
    request::models::HttpReq,
    store, utils,
};

static SBF_STORE_UPDATE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpaceBuffer {
    pub requests: HashMap<PathBuf, ReqBuffer>,
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
    fn new(space_abspath: &Path) -> Self {
        let sbf_store_abspath = store::utils::sbf_store_abspath(space_abspath);

        Self {
            buffer: SpaceBuffer {
                requests: HashMap::<PathBuf, ReqBuffer>::new(),
            },
            abspath: sbf_store_abspath,
        }
    }

    fn init(space_abspath: &Path) -> Result<Arc<Mutex<SpaceBufferStore>>> {
        let sbf_store_abspath = store::utils::sbf_store_abspath(space_abspath);

        if !sbf_store_abspath.exists() {
            let default_buffer = Arc::new(Mutex::new(Self::new(space_abspath)));
            Self::fswrite(&default_buffer)?;

            return Ok(default_buffer);
        }

        let content = fs::read_to_string(&sbf_store_abspath)
            .map_err(|_| Error::FileReadError("Failed to read from space buffer".into()))?;

        let sbf_store = match serde_json::from_str::<SpaceBuffer>(&content) {
            Ok(buffer) => Self {
                buffer,
                abspath: sbf_store_abspath,
            },
            Err(_) => {
                // corrupt JSON, use default
                let default_buffer = Self::new(space_abspath);
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

    pub fn get(space_abspath: &Path) -> Result<Arc<Mutex<SpaceBufferStore>>> {
        Self::init(space_abspath)
    }

    /// Updates the store using the provided mutator function and
    /// persists changes to the filesystem
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReqBufferMeta {
    pub fsname: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReqBufferUrl {
    #[serde(skip_serializing_if = "utils::is_string_none_or_empty")]
    pub raw: Option<String>,

    #[serde(skip_serializing_if = "utils::is_string_none_or_empty")]
    pub protocol: Option<String>,

    #[serde(skip_serializing_if = "utils::is_string_none_or_empty")]
    pub host: Option<String>,

    #[serde(skip_serializing_if = "utils::is_string_none_or_empty")]
    pub path: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReqBufferCfg {
    pub method: String,
    pub url: ReqBufferUrl,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<(bool, String, String)>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<(bool, String, String)>,

    #[serde(skip_serializing_if = "utils::is_string_none_or_empty")]
    pub content_type: Option<String>,

    #[serde(skip_serializing_if = "utils::is_string_none_or_empty")]
    pub body: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReqBuffer {
    pub meta: ReqBufferMeta,
    pub config: ReqBufferCfg,
}

impl ReqBuffer {
    pub fn from_req(req: &HttpReq) -> Self {
        let meta = ReqBufferMeta {
            fsname: req.meta.fsname.clone(),
            name: req.meta.name.clone(),
        };

        let config = ReqBufferCfg {
            method: req.config.method.clone(),
            url: ReqBufferUrl {
                raw: req.config.url.raw.clone(),
                protocol: req.config.url.protocol.clone(),
                host: req.config.url.host.clone(),
                path: req.config.url.path.clone(),
            },
            headers: req.config.headers.clone(),
            parameters: req.config.parameters.clone(),
            content_type: req.config.content_type.clone(),
            body: req.config.body.clone(),
        };

        Self { meta, config }
    }
}
