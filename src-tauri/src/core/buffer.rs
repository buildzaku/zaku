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
    pub fn load(abs_spacepath: &Path) -> RwLock<Self> {
        let space_buffer_file = ZAKU_DATA_DIR
            .join(SPACE_BUFFER_DIR)
            .join(&hashed_filename(&abs_spacepath.to_string_lossy()))
            .with_extension("json");

        if space_buffer_file.exists() {
            let content =
                fs::read_to_string(&space_buffer_file).expect("Failed to read from space buffer");
            let space_buffer: Result<SpaceBuf, _> = serde_json::from_str(&content);

            return RwLock::new(space_buffer.unwrap_or_else(|_| SpaceBuf {
                absolute_path: abs_spacepath.to_string_lossy().to_string(),
                requests: HashMap::new(),
            }));
        } else {
            return RwLock::new(SpaceBuf {
                absolute_path: abs_spacepath.to_string_lossy().to_string(),
                requests: HashMap::new(),
            });
        }
    }
    pub fn acquire_read_lock(space_buffer: &RwLock<Self>) -> RwLockReadGuard<Self> {
        return space_buffer.read().expect("Failed to acquire read lock");
    }
    pub fn acquire_write_lock(space_buffer: &RwLock<Self>) -> RwLockWriteGuard<Self> {
        return space_buffer.write().expect("Failed to acquire write lock");
    }
    pub fn persist(&self) {
        let buffer_file_path = ZAKU_DATA_DIR
            .join(SPACE_BUFFER_DIR)
            .join(&hashed_filename(&self.absolute_path))
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

pub fn save_request_to_space_buffer(
    abs_spacepath: &Path,
    parent_relative_path: &Path,
    request: HttpReq,
) {
    let space_buffer = SpaceBuf::load(abs_spacepath);
    let mut space_buffer_wlock = SpaceBuf::acquire_write_lock(&space_buffer);
    let request_relative_path = PathBuf::from(parent_relative_path)
        .join(&request.meta.file_name)
        .to_string_lossy()
        .to_string();
    let req_buf = ReqBuf::from_req(&request);

    space_buffer_wlock
        .requests
        .insert(request_relative_path, req_buf);

    space_buffer_wlock.persist();
}

pub fn write_buffer_request_to_fs(
    abs_spacepath: &Path,
    request_relative_path: &Path,
) -> Result<(), Error> {
    let space_buf = SpaceBuf::load(abs_spacepath);
    let mut space_buf_wlock = SpaceBuf::acquire_write_lock(&space_buf);

    let req_buf = space_buf_wlock
        .requests
        .get(&request_relative_path.to_string_lossy().to_string());

    if let Some(req_buf) = req_buf {
        let req_toml = ReqToml::from_reqbuf(req_buf);
        request::save_to_request_file(&abs_spacepath.join(request_relative_path), &req_toml)
            .unwrap();
    }

    space_buf_wlock
        .requests
        .remove(&request_relative_path.to_string_lossy().to_string());
    space_buf_wlock.persist();

    return Ok(());
}
