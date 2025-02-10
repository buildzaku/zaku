use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self};
use std::path::{Path, PathBuf};
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use super::utils::ZAKU_DATA_DIR;
use crate::models::request::Request;
use crate::models::space::SpaceBuffer;

fn hashed_file_name(absolute_space_path: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(absolute_space_path.to_string_lossy().as_bytes());
    let hash_value = hasher.finalize();

    return format!("{:x}.json", hash_value);
}

const SPACE_BUFFER_DIR: &str = "buffer/spaces";

impl SpaceBuffer {
    pub fn load(absolute_space_path: &Path) -> RwLock<Self> {
        let space_data_dir = ZAKU_DATA_DIR
            .join(SPACE_BUFFER_DIR)
            .join(&hashed_file_name(absolute_space_path))
            .with_extension("json");

        if space_data_dir.exists() {
            let content = fs::read_to_string(&space_data_dir).expect("Failed to read from store");
            let store: SpaceBuffer =
                serde_json::from_str(&content).expect("Failed to deserialize data");

            return RwLock::new(store);
        } else {
            return RwLock::new(SpaceBuffer {
                absolute_path: absolute_space_path.to_string_lossy().to_string(),
                requests_by_relative_path: HashMap::new(),
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
            .join(&hashed_file_name(Path::new(&self.absolute_path)))
            .with_extension("json");

        println!(
            "BUFFER FILE PATH: {}",
            buffer_file_path.to_string_lossy().to_string()
        );

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
    absolute_space_path: &Path,
    parent_relative_path: &Path,
    request: Request,
) {
    let space_buffer = SpaceBuffer::load(absolute_space_path);
    let mut space_buffer_wlock = SpaceBuffer::acquire_write_lock(&space_buffer);
    let request_relative_path = PathBuf::from(parent_relative_path)
        .join(&request.meta.file_name)
        .to_string_lossy()
        .into_owned();

    space_buffer_wlock
        .requests_by_relative_path
        .insert(request_relative_path, request);

    space_buffer_wlock.persist();
}
