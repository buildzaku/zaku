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
    store::{self, models::ReqBuf},
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
            .map_err(|_| Error::LockError("Failed to acquire lock".into()))?;

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
