use serde::{Deserialize, Serialize};
use specta::Type;

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
pub struct CreateNewRequest {
    pub parent_relpath: String,
    pub relpath: String,
}
