use std::collections::HashMap;
use std::fs::{self};
use std::io::Error;
use std::path::{Path, PathBuf};
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::{
    request,
    request::models::{HttpReq, ReqToml},
    store::models::{ReqBuf, SpaceBuf},
    utils::{hashed_filename, ZAKU_DATA_DIR},
};

const SETTINGS_FILENAME: &str = "buffer.json";

impl SpaceBuf {
    pub fn load(space_abspath: &Path) -> RwLock<Self> {
        let spacebuf_file = ZAKU_DATA_DIR
            .join(super::SPACES_STORE_DIR)
            .join(&hashed_filename(&space_abspath.to_string_lossy()))
            .join(SETTINGS_FILENAME);

        if spacebuf_file.exists() {
            let content =
                fs::read_to_string(&spacebuf_file).expect("Failed to read from space buffer");
            let space_buffer: Result<SpaceBuf, _> = serde_json::from_str(&content);

            return RwLock::new(space_buffer.unwrap_or_else(|_| SpaceBuf {
                abspath: space_abspath.to_string_lossy().to_string(),
                requests: HashMap::new(),
            }));
        } else {
            return RwLock::new(SpaceBuf {
                abspath: space_abspath.to_string_lossy().to_string(),
                requests: HashMap::new(),
            });
        }
    }

    pub fn acq_rlock(space_buffer: &RwLock<Self>) -> RwLockReadGuard<Self> {
        return space_buffer.read().expect("Failed to acquire read lock");
    }

    pub fn acq_wlock(space_buffer: &RwLock<Self>) -> RwLockWriteGuard<Self> {
        return space_buffer.write().expect("Failed to acquire write lock");
    }

    pub fn persist(&self) {
        let buf_filepath = ZAKU_DATA_DIR
            .join(super::SPACES_STORE_DIR)
            .join(&hashed_filename(&self.abspath))
            .join(SETTINGS_FILENAME);

        if let Some(parent) = buf_filepath.parent() {
            fs::create_dir_all(parent).expect("Failed to create parent directories");
        }

        let serialized_store =
            serde_json::to_string_pretty(self).expect("Failed to serialize store data");

        fs::write(&buf_filepath, serialized_store)
            .expect("Failed to write serialized store to disk");
    }
}

pub fn persist_req_to_spacebuf(space_abspath: &Path, parent_relpath: &Path, request: HttpReq) {
    let space_buffer = SpaceBuf::load(space_abspath);
    let mut spacebuf_wlock = SpaceBuf::acq_wlock(&space_buffer);
    let req_relpath = PathBuf::from(parent_relpath)
        .join(&request.meta.file_name)
        .to_string_lossy()
        .to_string();
    let req_buf = ReqBuf::from_req(&request);

    spacebuf_wlock.requests.insert(req_relpath, req_buf);

    spacebuf_wlock.persist();
}

pub fn write_reqbuf_to_reqtoml(space_abspath: &Path, req_relpath: &Path) -> Result<(), Error> {
    let space_buf = SpaceBuf::load(space_abspath);
    let mut spacebuf_wlock = SpaceBuf::acq_wlock(&space_buf);

    let req_buf = spacebuf_wlock
        .requests
        .get(&req_relpath.to_string_lossy().to_string());

    if let Some(req_buf) = req_buf {
        let req_toml = ReqToml::from_reqbuf(req_buf);
        request::persist_reqtoml(&space_abspath.join(req_relpath), &req_toml).unwrap();
    }

    spacebuf_wlock
        .requests
        .remove(&req_relpath.to_string_lossy().to_string());
    spacebuf_wlock.persist();

    Ok(())
}
