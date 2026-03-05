#[cfg(feature = "test-support")]
use std::{
    path::{Component, Path, PathBuf},
    sync::Arc,
};

#[cfg(feature = "test-support")]
use serde_json::Value;

#[cfg(feature = "test-support")]
use tempfile::TempDir;

#[cfg(feature = "test-support")]
pub struct TempFs {
    _temp_dir: TempDir,
    path: PathBuf,
}

#[cfg(feature = "test-support")]
impl TempFs {
    pub fn new() -> Self {
        let temp_dir = TempDir::new().unwrap();
        let path = std::fs::canonicalize(temp_dir.path()).unwrap();

        Self {
            _temp_dir: temp_dir,
            path,
        }
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    pub fn insert_tree(&self, path: impl AsRef<Path>, tree: Value) {
        fn inner(directory: &Path, path: Arc<Path>, tree: Value) {
            match tree {
                Value::Object(map) => {
                    let absolute_path = resolve_path(directory, path.as_ref());
                    std::fs::create_dir_all(&absolute_path).unwrap();
                    for (name, contents) in map {
                        let mut new_path = PathBuf::from(path.as_ref());
                        new_path.push(name);
                        inner(directory, Arc::from(new_path), contents);
                    }
                }
                Value::Null => {
                    let absolute_path = resolve_path(directory, path.as_ref());
                    std::fs::create_dir_all(&absolute_path).unwrap();
                }
                Value::String(contents) => {
                    let absolute_path = resolve_path(directory, path.as_ref());
                    if let Some(parent) = absolute_path.parent() {
                        std::fs::create_dir_all(parent).unwrap();
                    }
                    std::fs::write(&absolute_path, contents.as_bytes()).unwrap();
                }
                _ => {
                    panic!("JSON object must contain only objects, strings, or null");
                }
            }
        }

        inner(self.path(), Arc::from(path.as_ref()), tree)
    }
}

#[cfg(feature = "test-support")]
fn resolve_path(root: &Path, path: &Path) -> PathBuf {
    if !path.is_absolute() {
        return root.join(path);
    }

    path.components()
        .fold(root.to_path_buf(), |mut resolved, component| {
            match component {
                Component::Prefix(_) | Component::RootDir => {}
                Component::CurDir => {}
                Component::ParentDir => resolved.push(".."),
                Component::Normal(part) => resolved.push(part),
            }
            resolved
        })
}
