use std::collections::HashMap;
use std::fs::{self};
use std::io::Error;
use std::path::{Path, PathBuf};
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::core::request;
use crate::core::utils::{hashed_filename, ZAKU_DATA_DIR};
use crate::models::buffer::{ReqBuf, SpaceBuf};
use crate::models::request::HttpReq;
use crate::models::toml::ReqToml;

const SPACE_BUFFER_DIR: &str = "buffer/spaces";

impl SpaceBuf {
    pub fn load(space_abspath: &Path) -> RwLock<Self> {
        let space_buffer_file = ZAKU_DATA_DIR
            .join(SPACE_BUFFER_DIR)
            .join(&hashed_filename(&space_abspath.to_string_lossy()))
            .with_extension("json");

        if space_buffer_file.exists() {
            let content =
                fs::read_to_string(&space_buffer_file).expect("Failed to read from space buffer");
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
        let buffer_file_path = ZAKU_DATA_DIR
            .join(SPACE_BUFFER_DIR)
            .join(&hashed_filename(&self.abspath))
            .with_extension("json");

        if let Some(parent) = buffer_file_path.parent() {
            fs::create_dir_all(parent).expect("Failed to create parent directories");
        }

        let serialized_store =
            serde_json::to_string_pretty(self).expect("Failed to serialize store data");

        fs::write(&buffer_file_path, serialized_store)
            .expect("Failed to write serialized store to disk");
    }
}

pub fn save_req_to_space_buffer(space_abspath: &Path, parent_relpath: &Path, request: HttpReq) {
    let space_buffer = SpaceBuf::load(space_abspath);
    let mut space_buffer_wlock = SpaceBuf::acq_wlock(&space_buffer);
    let req_relpath = PathBuf::from(parent_relpath)
        .join(&request.meta.file_name)
        .to_string_lossy()
        .to_string();
    let req_buf = ReqBuf::from_req(&request);

    space_buffer_wlock.requests.insert(req_relpath, req_buf);

    space_buffer_wlock.persist();
}

pub fn write_buffer_req_to_fs(space_abspath: &Path, req_relpath: &Path) -> Result<(), Error> {
    let space_buf = SpaceBuf::load(space_abspath);
    let mut space_buf_wlock = SpaceBuf::acq_wlock(&space_buf);

    let req_buf = space_buf_wlock
        .requests
        .get(&req_relpath.to_string_lossy().to_string());

    if let Some(req_buf) = req_buf {
        let req_toml = ReqToml::from_reqbuf(req_buf);
        request::save_to_req_toml(&space_abspath.join(req_relpath), &req_toml).unwrap();
    }

    space_buf_wlock
        .requests
        .remove(&req_relpath.to_string_lossy().to_string());
    space_buf_wlock.persist();

    return Ok(());
}
