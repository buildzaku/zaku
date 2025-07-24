use serde::{Deserialize, Serialize};
use specta::Type;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::{
    error::{Error, Result},
    request::models::{HttpReq, ReqCfg, ReqMeta},
    store::{self},
    utils,
};

static SPACEBUF_UPDATE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, Serialize, Deserialize, Default, Type)]
pub struct SpaceBuf {
    pub abspath: PathBuf,
    pub requests: HashMap<String, ReqBuf>,
}

impl SpaceBuf {
    fn filename() -> &'static str {
        "buffer.json"
    }

    pub fn filepath(space_abspath: &Path) -> PathBuf {
        let hsh = utils::hashed_filename(space_abspath);

        store::utils::datadir_abspath()
            .join(store::utils::SPACES_STORE_FSNAME)
            .join(hsh)
            .join(Self::filename())
    }

    fn init(space_abspath: &Path) -> Result<Arc<Mutex<SpaceBuf>>> {
        let spacebuf_filepath = Self::filepath(space_abspath);
        if !spacebuf_filepath.exists() {
            let default_buffer = Arc::new(Mutex::new(SpaceBuf {
                abspath: space_abspath.to_path_buf(),
                requests: HashMap::new(),
            }));
            Self::fswrite(space_abspath, &default_buffer)?;

            return Ok(default_buffer);
        }

        let content = fs::read_to_string(&spacebuf_filepath)
            .map_err(|_| Error::FileReadError("Failed to read from space buffer".into()))?;

        let space_buffer = match serde_json::from_str(&content) {
            Ok(buffer) => buffer,
            Err(_) => {
                // corrupt JSON, use default
                let default_buffer = SpaceBuf {
                    abspath: space_abspath.to_path_buf(),
                    requests: HashMap::new(),
                };
                let buffer_arc = Arc::new(Mutex::new(default_buffer));
                Self::fswrite(space_abspath, &buffer_arc)?;

                return Ok(buffer_arc);
            }
        };

        Ok(Arc::new(Mutex::new(space_buffer)))
    }

    fn fswrite(space_abspath: &Path, buffer: &Arc<Mutex<SpaceBuf>>) -> Result<()> {
        let buf = buffer
            .lock()
            .map_err(|_| Error::LockError("Failed to acquire mutex lock".into()))?;

        let buf_filepath = Self::filepath(space_abspath);

        if let Some(parent) = buf_filepath.parent() {
            fs::create_dir_all(parent)?;
        }

        let serialized_store = serde_json::to_string_pretty(&*buf)?;
        fs::write(&buf_filepath, serialized_store)?;

        Ok(())
    }

    pub fn get(space_abspath: &Path) -> Result<Arc<Mutex<SpaceBuf>>> {
        Self::init(space_abspath)
    }

    /// Updates space buffer using a mutator function and persists changes to filesystem
    ///
    /// Acquires an exclusive lock to ensure thread-safe updates, loads the current buffer,
    /// applies the mutator function and writes the changes to the filesystem.
    /// The update operation is serialized across all concurrent calls for the same space.
    ///
    /// - `space_abspath`: Absolute path to the space directory
    /// - `mutator`: Function that receives the buffer and applies modifications
    ///
    /// Returns a `Result<Arc<Mutex<SpaceBuf>>>` containing the updated buffer
    pub fn update<F>(space_abspath: &Path, mutator: F) -> Result<Arc<Mutex<SpaceBuf>>>
    where
        F: FnOnce(&Arc<Mutex<SpaceBuf>>),
    {
        let _guard = SPACEBUF_UPDATE_LOCK.lock().unwrap();

        let buffer = Self::get(space_abspath)?;
        mutator(&buffer);
        Self::fswrite(space_abspath, &buffer)?;

        Ok(buffer)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct ReqBuf {
    pub meta: ReqMeta,
    pub config: ReqCfg,
}

impl ReqBuf {
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
