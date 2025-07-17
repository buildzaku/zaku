use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;
use std::fs::{self};
use std::path::Path;
use std::sync::RwLock;

use crate::{
    error::{Error, Result},
    request,
    request::models::{HttpReq, ReqToml},
    store::models::ReqBuf,
    utils::{hashed_filename, APP_DATA_DIR},
};

const SETTINGS_FILENAME: &str = "buffer.json";

#[derive(Clone, Debug, Serialize, Deserialize, Default, Type)]
pub struct SpaceBuf {
    pub abspath: String,
    pub requests: HashMap<String, ReqBuf>,
}

impl SpaceBuf {
    pub fn load(space_abspath: &Path) -> Result<RwLock<Self>> {
        let spacebuf_file = APP_DATA_DIR
            .join(super::SPACES_STORE_DIR)
            .join(hashed_filename(&space_abspath.to_string_lossy()))
            .join(SETTINGS_FILENAME);

        if spacebuf_file.exists() {
            let content = fs::read_to_string(&spacebuf_file)
                .map_err(|_| Error::FileReadError("Failed to read from space buffer".into()))?;

            let space_buffer = serde_json::from_str(&content).unwrap_or_else(|_| SpaceBuf {
                abspath: space_abspath.to_string_lossy().to_string(),
                requests: HashMap::new(),
            });

            Ok(RwLock::new(space_buffer))
        } else {
            Ok(RwLock::new(SpaceBuf {
                abspath: space_abspath.to_string_lossy().to_string(),
                requests: HashMap::new(),
            }))
        }
    }

    pub fn persist(&self) -> Result<()> {
        let buf_filepath = APP_DATA_DIR
            .join(super::SPACES_STORE_DIR)
            .join(hashed_filename(&self.abspath))
            .join(SETTINGS_FILENAME);

        if let Some(parent) = buf_filepath.parent() {
            fs::create_dir_all(parent)?;
        }

        let serialized_store = serde_json::to_string_pretty(self)?;
        fs::write(&buf_filepath, serialized_store)?;

        Ok(())
    }
}

pub fn persist_req_to_spacebuf(
    space_abspath: &Path,
    parent_relpath: &Path,
    request: HttpReq,
) -> Result<()> {
    let space_buffer = SpaceBuf::load(space_abspath)?;
    let mut spacebuf_wlock = space_buffer
        .write()
        .map_err(|_| Error::LockError("Failed to acquire write lock".into()))?;

    let req_relpath = parent_relpath
        .join(&request.meta.fsname)
        .to_string_lossy()
        .to_string();
    let req_buf = ReqBuf::from_req(&request);

    spacebuf_wlock.requests.insert(req_relpath, req_buf);
    spacebuf_wlock.persist()?;

    Ok(())
}

pub fn write_reqbuf_to_reqtoml(space_abspath: &Path, req_relpath: &Path) -> Result<()> {
    let space_buffer = SpaceBuf::load(space_abspath)?;
    let mut spacebuf_wlock = space_buffer
        .write()
        .map_err(|_| Error::LockError("Failed to acquire write lock".into()))?;

    let relpath_str = req_relpath.to_string_lossy().to_string();
    if let Some(req_buf) = spacebuf_wlock.requests.get(&relpath_str) {
        let req_toml = ReqToml::from_reqbuf(req_buf);
        request::persist_reqtoml(&space_abspath.join(req_relpath), &req_toml)?;
    }

    spacebuf_wlock.requests.remove(&relpath_str);
    spacebuf_wlock.persist()?;

    Ok(())
}
