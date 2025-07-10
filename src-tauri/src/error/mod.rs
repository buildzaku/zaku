use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct ZakuError {
    pub error: String,
    pub message: String,
}
