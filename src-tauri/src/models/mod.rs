use serde::{Deserialize, Serialize};
use specta::Type;

pub mod buffer;
pub mod collection;
pub mod request;
pub mod space;
pub mod toml;
pub mod zaku;

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct OpenDirDialogOpt {
    pub title: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct DispatchNotificationOptions {
    pub title: String,
    pub body: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct CreateNewCollection {
    pub parent_relative_path: String,
    pub relative_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct CreateNewRequest {
    pub parent_relative_path: String,
    pub relative_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct MoveTreeItemDto {
    pub source_relative_path: String,
    pub destination_relative_path: String,
}
