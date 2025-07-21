use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SanitizedSegment {
    pub name: String,
    pub fsname: String,
}
